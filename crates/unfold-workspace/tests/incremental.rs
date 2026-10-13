//! Incremental equals full, with the real languages (docs/engine.md#updates,
//! §16): random edit sequences on copies of both playgrounds go through a workspace's background
//! updates (the path the watcher feeds, and the store's row diffs), and the graph the workspace
//! committed must equal a fresh full compile of the same bytes, record for record.
//!
//! The edits are the ones that move cross-file facts: body edits, adding and removing functions,
//! renaming an export (its callers elsewhere change), a new subclass that overrides a method in a
//! new file, deleting and recreating files (imports elsewhere go `not_found` and back), breaking
//! and fixing syntax, and case-only renames. Steps either settle before the next one or land while
//! the previous update is queued or running, so changes merge and updates restart.
//!
//! `PROPTEST_CASES` sets the number of sequences per playground (default 6, for pull requests).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use unfold_core::RepoPath;
use unfold_engine::{Engine, MemoryFactsCache, UpdateRequest};
use unfold_workspace::{
    EventSink, OpenContext, Workspace, WorkspaceEvent, WorkspaceOptions, default_languages,
};

/// The longest a step may take to settle; it ends as soon as the workspace is idle.
const PATIENCE: Duration = Duration::from_secs(60);

/// Sequences per playground when `PROPTEST_CASES` isn't set.
const DEFAULT_CASES: u32 = 6;

struct Quiet;

impl EventSink for Quiet {
    fn emit(&self, _event: &WorkspaceEvent) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lang {
    TypeScript,
    Python,
}

impl Lang {
    fn extension(self) -> &'static str {
        match self {
            Self::TypeScript => ".ts",
            Self::Python => ".py",
        }
    }

    /// A statement for the start of a body, indented like `indent`.
    fn statement(self, indent: &str, n: u32) -> String {
        match self {
            Self::TypeScript => format!("{indent}void {n};\n"),
            Self::Python => format!("{indent}_probe = {n}\n"),
        }
    }

    /// A new top-level function that calls an existing one when it can.
    fn function(self, n: u32, callee: Option<&str>) -> String {
        match (self, callee) {
            (Self::TypeScript, Some(callee)) => {
                format!("\nexport function added{n}(): unknown {{\n  return {callee};\n}}\n")
            }
            (Self::TypeScript, None) => {
                format!("\nexport function added{n}(): number {{\n  return {n};\n}}\n")
            }
            (Self::Python, Some(callee)) => format!("\n\ndef added_{n}():\n    return {callee}\n"),
            (Self::Python, None) => format!("\n\ndef added_{n}():\n    return {n}\n"),
        }
    }

    /// Text that makes a file stop parsing.
    fn breakage(self) -> &'static str {
        match self {
            Self::TypeScript => "\nfunction broken( {\n",
            Self::Python => "\ndef broken(:\n",
        }
    }

    /// The first exported (TypeScript) or top-level (Python) function's name.
    fn first_export(self, text: &str) -> Option<String> {
        let prefix = match self {
            Self::TypeScript => "export function ",
            Self::Python => "def ",
        };
        text.lines().find_map(|line| {
            let rest = line.strip_prefix(prefix)?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
    }

    /// A subclass of the playground's `OrderService` that overrides `place`, as a new file.
    fn subclass(self, n: u32) -> (String, String) {
        match self {
            Self::TypeScript => (
                format!("src/services/priority-orders-{n}.ts"),
                format!(
                    "import {{ OrderService }} from \"./order-service.ts\";\n\nexport class PriorityOrders{n} extends OrderService {{\n  place(input: unknown) {{\n    return super.place(input);\n  }}\n}}\n"
                ),
            ),
            Self::Python => (
                format!("src/orders_playground/services/priority_{n}.py"),
                format!(
                    "from .order_service import OrderService\n\n\nclass PriorityOrders{n}(OrderService):\n    def place(self, value: object):\n        return super().place(value)\n"
                ),
            ),
        }
    }
}

/// One edit; indices pick a source file modulo the playground's file count.
#[derive(Clone, Debug)]
enum Edit {
    Body(usize),
    AddFunction(usize),
    RemoveFunction(usize),
    RenameExport(usize),
    AddSubclass,
    DeleteOrRestore(usize),
    BreakOrFix(usize),
    #[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
    CaseRename(usize),
}

/// Source file picks, taken modulo the playground's file count.
const INDEX: std::ops::Range<usize> = 0..10_000;

fn edit() -> impl Strategy<Value = Edit> {
    let common = prop_oneof![
        4 => INDEX.prop_map(Edit::Body),
        2 => INDEX.prop_map(Edit::AddFunction),
        1 => INDEX.prop_map(Edit::RemoveFunction),
        2 => INDEX.prop_map(Edit::RenameExport),
        1 => Just(Edit::AddSubclass),
        2 => INDEX.prop_map(Edit::DeleteOrRestore),
        2 => INDEX.prop_map(Edit::BreakOrFix),
    ];
    prop_oneof![9 => common, 1 => INDEX.prop_map(Edit::CaseRename)]
}

/// A batch of edits, then either wait for the update or go on at once.
#[derive(Clone, Debug)]
struct Step {
    edits: Vec<Edit>,
    settle: bool,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(
        (prop::collection::vec(edit(), 1..4), any::<bool>())
            .prop_map(|(edits, settle)| Step { edits, settle }),
        1..8,
    )
}

/// One source file of the playground as the sequence left it.
#[derive(Debug)]
struct Source {
    /// Folder-relative path as it was copied.
    original: String,
    /// Its current path (differs after a case-only rename).
    path: String,
    /// Its text before it was deleted, while it is deleted.
    deleted: Option<String>,
    /// Functions this sequence appended, in order.
    added: Vec<String>,
    /// The export renamed (`name`, `name_renamed`), while it is renamed.
    renamed: Option<String>,
    broken: bool,
}

/// A playground copy and the state of its edits.
struct Folder {
    _dir: tempfile::TempDir,
    root: PathBuf,
    lang: Lang,
    sources: Vec<Source>,
    counter: u32,
}

impl Folder {
    fn new(name: &str, lang: Lang) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical temp dir");
        let root = base.join(name);
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/playgrounds")
            .join(name);
        copy_dir(&source, &root);
        let _ = std::fs::remove_dir_all(root.join(".unfold/local"));
        let mut paths = Vec::new();
        collect(&root, &root.join("src"), lang.extension(), &mut paths);
        paths.sort();
        let sources = paths
            .into_iter()
            .map(|path| Source {
                original: path.clone(),
                path,
                deleted: None,
                added: Vec::new(),
                renamed: None,
                broken: false,
            })
            .collect();
        Self {
            _dir: dir,
            root,
            lang,
            sources,
            counter: 0,
        }
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).expect("read source")
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(full, text).expect("write source");
    }

    fn next(&mut self) -> u32 {
        self.counter += 1;
        self.counter
    }

    /// Applies `edit` and returns the paths a watcher would report.
    fn apply(&mut self, edit: &Edit) -> Vec<String> {
        let count = self.sources.len();
        let lang = self.lang;
        match *edit {
            Edit::AddSubclass => {
                let n = self.next();
                let (path, text) = lang.subclass(n);
                self.write(&path, &text);
                vec![path]
            }
            Edit::Body(index) => {
                let n = self.next();
                let source = &self.sources[index % count];
                if source.deleted.is_some() {
                    return Vec::new();
                }
                let path = source.path.clone();
                let text = self.read(&path);
                let Some(at) = first_indented_return(&text) else {
                    return Vec::new();
                };
                let indent: String = text[at..].chars().take_while(|c| *c == ' ').collect();
                let edited = format!("{}{}{}", &text[..at], lang.statement(&indent, n), &text[at..]);
                self.write(&path, &edited);
                vec![path]
            }
            Edit::AddFunction(index) => {
                let n = self.next();
                let i = index % count;
                if self.sources[i].deleted.is_some() {
                    return Vec::new();
                }
                let path = self.sources[i].path.clone();
                let text = self.read(&path);
                // A reference to the file's first export: a passed reference the graph records.
                let callee = lang.first_export(&text).filter(|name| name != "broken");
                let function = lang.function(n, callee.as_deref());
                let edited = insert_before_breakage(&text, &function, self.sources[i].broken, lang);
                self.sources[i].added.push(function);
                self.write(&path, &edited);
                vec![path]
            }
            Edit::RemoveFunction(index) => {
                let i = index % count;
                let source = &self.sources[i];
                if source.deleted.is_some() {
                    return Vec::new();
                }
                let Some(function) = source.added.last().cloned() else {
                    return Vec::new();
                };
                let path = source.path.clone();
                let text = self.read(&path);
                let Some(at) = text.rfind(&function) else {
                    return Vec::new();
                };
                let edited = format!("{}{}", &text[..at], &text[at + function.len()..]);
                self.sources[i].added.pop();
                self.write(&path, &edited);
                vec![path]
            }
            Edit::RenameExport(index) => {
                let i = index % count;
                // A broken file's first function may be the breakage itself; renaming it would
                // keep the fix from finding it.
                if self.sources[i].deleted.is_some() || self.sources[i].broken {
                    return Vec::new();
                }
                let path = self.sources[i].path.clone();
                let text = self.read(&path);
                let prefix = match lang {
                    Lang::TypeScript => "export function ",
                    Lang::Python => "def ",
                };
                let edited = match self.sources[i].renamed.take() {
                    Some(name) => {
                        text.replacen(&format!("{prefix}{name}_renamed"), &format!("{prefix}{name}"), 1)
                    }
                    None => match lang.first_export(&text) {
                        Some(name) => {
                            let edited = text.replacen(
                                &format!("\n{prefix}{name}"),
                                &format!("\n{prefix}{name}_renamed"),
                                1,
                            );
                            self.sources[i].renamed = Some(name);
                            edited
                        }
                        None => return Vec::new(),
                    },
                };
                self.write(&path, &edited);
                vec![path]
            }
            Edit::DeleteOrRestore(index) => {
                let i = index % count;
                let path = self.sources[i].path.clone();
                match self.sources[i].deleted.take() {
                    Some(text) => self.write(&path, &text),
                    None => {
                        self.sources[i].deleted = Some(self.read(&path));
                        std::fs::remove_file(self.root.join(&path)).expect("delete source");
                    }
                }
                vec![path]
            }
            Edit::BreakOrFix(index) => {
                let i = index % count;
                if self.sources[i].deleted.is_some() {
                    return Vec::new();
                }
                let path = self.sources[i].path.clone();
                let text = self.read(&path);
                let edited = if self.sources[i].broken {
                    text.replacen(lang.breakage(), "", 1)
                } else {
                    format!("{text}{}", lang.breakage())
                };
                self.sources[i].broken = !self.sources[i].broken;
                self.write(&path, &edited);
                vec![path]
            }
            Edit::CaseRename(index) => {
                let i = index % count;
                if self.sources[i].deleted.is_some() {
                    return Vec::new();
                }
                let from = self.sources[i].path.clone();
                let to = if from == self.sources[i].original {
                    case_flipped(&from)
                } else {
                    self.sources[i].original.clone()
                };
                std::fs::rename(self.root.join(&from), self.root.join(&to)).expect("rename");
                self.sources[i].path = to.clone();
                vec![from, to]
            }
        }
    }

    /// Fixes every file this sequence broke (last-good facts can't be compared with a fresh
    /// compile, which never saw the version that parsed), returning the paths it touched.
    fn fix_all(&mut self) -> Vec<String> {
        let mut paths = Vec::new();
        for index in 0..self.sources.len() {
            if self.sources[index].broken && self.sources[index].deleted.is_none() {
                paths.extend(self.apply(&Edit::BreakOrFix(index)));
            } else if self.sources[index].broken {
                // A broken file that was deleted: restore it fixed.
                let text = self.sources[index].deleted.take().unwrap_or_default();
                let path = self.sources[index].path.clone();
                self.write(&path, &text.replacen(self.lang.breakage(), "", 1));
                self.sources[index].broken = false;
                paths.push(path);
            }
        }
        paths
    }

    fn context(&self) -> OpenContext {
        let mut options = WorkspaceOptions {
            debounce: Duration::from_millis(40),
            watch: false,
            ..WorkspaceOptions::default()
        };
        options.engine.discover.global_ignore = false;
        options.engine.compile_threads = 2;
        OpenContext {
            project: serde_json::from_value(serde_json::json!("p-incrementl")).expect("a project id"),
            options,
            sink: Arc::new(Quiet),
            languages: default_languages(),
        }
    }

    /// The canonical export of a fresh full compile of the folder as it is now, with no cache.
    fn fresh_export(&self) -> String {
        let options = self.context().options.engine;
        let mut engine = Engine::new(
            &self.root,
            default_languages(),
            Arc::new(MemoryFactsCache::new()),
            options,
        )
        .expect("fresh engine");
        let outcome = engine.update(UpdateRequest::compile()).expect("fresh compile");
        unfold_core::canonical::to_string(&outcome.graph)
    }
}

/// The byte offset of the first `return` line that is indented (inside a body).
fn first_indented_return(text: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        if trimmed.len() < line.len() && (trimmed.starts_with("return ") || trimmed.starts_with("return;")) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Appends `function`, keeping a breakage (if any) last so fixing it removes only that.
fn insert_before_breakage(text: &str, function: &str, broken: bool, lang: Lang) -> String {
    if broken && let Some(at) = text.rfind(lang.breakage()) {
        return format!("{}{function}{}", &text[..at], &text[at..]);
    }
    format!("{text}{function}")
}

/// The path with its file name's first letter's case flipped (`lib/logger.ts` → `lib/Logger.ts`).
fn case_flipped(path: &str) -> String {
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    let mut chars = name.chars();
    let flipped = match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        }
        Some(first) => format!("{}{}", first.to_ascii_lowercase(), chars.as_str()),
        None => String::new(),
    };
    if folder.is_empty() {
        flipped
    } else {
        format!("{folder}/{flipped}")
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn collect(root: &Path, dir: &Path, extension: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect(root, &path, extension, out);
        } else if path.to_string_lossy().ends_with(extension) {
            let relative = path.strip_prefix(root).expect("inside root");
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn notify(workspace: &Workspace, paths: &[String]) {
    workspace.notify_changes(
        paths
            .iter()
            .map(|path| RepoPath::new(path).expect("folder-relative path")),
    );
}

/// The first line where two exports differ, for a readable failure.
fn first_difference(left: &str, right: &str) -> String {
    for (index, (a, b)) in left.lines().zip(right.lines()).enumerate() {
        if a != b {
            return format!("line {}:\n  incremental: {a}\n  fresh:       {b}", index + 1);
        }
    }
    format!(
        "one export is a prefix of the other ({} vs {} lines)",
        left.lines().count(),
        right.lines().count()
    )
}

/// Runs `steps` as background updates on a fresh copy of `playground`, then compares.
fn check(playground: &str, lang: Lang, steps: &[Step]) -> Result<(), TestCaseError> {
    let mut folder = Folder::new(playground, lang);
    let workspace = Workspace::open(&folder.root, folder.context()).expect("open the workspace");
    workspace.compile(None).wait().expect("first compile");
    for step in steps {
        let mut paths = Vec::new();
        for edit in &step.edits {
            paths.extend(folder.apply(edit));
        }
        notify(&workspace, &paths);
        if step.settle {
            prop_assert!(workspace.wait_idle(PATIENCE), "the workspace never went idle");
        }
    }
    let fixed = folder.fix_all();
    notify(&workspace, &fixed);
    prop_assert!(workspace.wait_idle(PATIENCE), "the workspace never went idle");
    let committed = workspace
        .readers()
        .expect("readers")
        .get()
        .expect("reader")
        .graph()
        .expect("committed graph");
    workspace.close();
    let incremental = unfold_core::canonical::to_string(&committed);
    let fresh = folder.fresh_export();
    prop_assert!(
        incremental == fresh,
        "the committed graph differs from a fresh compile after {steps:?}\n{}",
        first_difference(&incremental, &fresh)
    );
    Ok(())
}

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_CASES)
}

fn run(playground: &'static str, lang: Lang) {
    let config = Config {
        cases: cases(),
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner = TestRunner::new(config);
    runner
        .run(&steps(), |steps| check(playground, lang, &steps))
        .expect("incremental equals full");
}

#[test]
fn typescript_playground_updates_equal_a_full_compile() {
    run("ts-orders", Lang::TypeScript);
}

#[test]
fn python_playground_updates_equal_a_full_compile() {
    run("py-orders", Lang::Python);
}

/// Every kind of edit at least once, in a fixed order, on both playgrounds.
#[test]
fn each_kind_of_edit_matches_a_fresh_compile() {
    let steps = vec![
        Step {
            edits: vec![Edit::Body(3), Edit::AddFunction(5)],
            settle: true,
        },
        Step {
            edits: vec![Edit::RenameExport(7), Edit::AddSubclass],
            settle: false,
        },
        Step {
            edits: vec![Edit::BreakOrFix(2)],
            settle: true,
        },
        Step {
            edits: vec![Edit::DeleteOrRestore(9), Edit::RemoveFunction(5)],
            settle: false,
        },
        Step {
            edits: vec![Edit::BreakOrFix(2), Edit::RenameExport(7)],
            settle: true,
        },
        Step {
            edits: vec![Edit::DeleteOrRestore(9), Edit::Body(11)],
            settle: true,
        },
    ];
    check("ts-orders", Lang::TypeScript, &steps).expect("ts-orders");
    check("py-orders", Lang::Python, &steps).expect("py-orders");
}

/// A case-only rename through a background update. On case-insensitive file systems the old
/// path still finds the file (`symlink_metadata` matches `Logger.ts` for `logger.ts`), so the
/// scan compares names as spelled on disk.
#[test]
fn case_only_rename_matches_a_fresh_compile() {
    let steps = vec![
        Step {
            edits: vec![Edit::CaseRename(4)],
            settle: true,
        },
        Step {
            edits: vec![Edit::Body(4)],
            settle: true,
        },
        Step {
            edits: vec![Edit::CaseRename(4)],
            settle: true,
        },
    ];
    check("ts-orders", Lang::TypeScript, &steps).expect("ts-orders");
    check("py-orders", Lang::Python, &steps).expect("py-orders");
}
