//! Compiles TypeScript sources with TypeScript linking and validates the graph against the
//! sources and the TypeScript census, so every test checks that every call in the parse tree is
//! one call node, independently of extraction.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use unfold_core::validate::Violation;
use unfold_core::vocab::{FileState, InputRole, Language as LanguageId, LinkKind};
use unfold_core::{ByteRange, File, FileGraph, Graph, Link, LinkTarget, RepoPath, Snapshot};
use unfold_engine::links::program_links;
use unfold_engine::settings::Settings;
use unfold_engine::validate::validate_graph;
use unfold_engine::{FactsSet, FileFacts, Language, ProgramFile, ProjectInput, ProjectInputs, Resolver};
use unfold_lang_typescript::TypeScript;

/// A compiled set of files.
pub struct Compiled {
    pub graph: Graph,
    pub violations: Vec<Violation>,
    /// Files that failed, with their messages.
    pub failures: Vec<(String, String)>,
}

impl Compiled {
    pub fn file(&self, path: &str) -> &FileGraph {
        self.graph.file(path).unwrap_or_else(|| panic!("no file {path}"))
    }

    /// Asserts nothing failed and the graph is valid.
    #[track_caller]
    pub fn valid(self) -> Self {
        assert!(self.failures.is_empty(), "failures: {:?}", self.failures);
        let violations: Vec<String> = self.violations.iter().map(ToString::to_string).collect();
        assert!(violations.is_empty(), "violations:\n{}", violations.join("\n"));
        self
    }
}

/// Compiles `files` (path, bytes) as one program.
pub fn compile(language: &TypeScript, files: &[(String, Vec<u8>)]) -> Compiled {
    compile_with(language, files, &ProjectInputs::default())
}

/// Compiles `files` (path, bytes) as one program with `inputs`; each file is extracted with the
/// options its configuration gives it.
pub fn compile_with(language: &TypeScript, files: &[(String, Vec<u8>)], inputs: &ProjectInputs) -> Compiled {
    let settings = Settings::default();
    let mut snapshots = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut program = Vec::new();
    let mut failures = Vec::new();
    for (path, bytes) in files {
        let path = RepoPath::new(path).expect("valid path");
        let snapshot = Snapshot::new(path.clone(), bytes.clone()).expect("valid UTF-8");
        let options = language
            .extraction_options(&[&path], &settings, inputs)
            .pop()
            .expect("options for the file");
        let (facts, record) = match language.extract_with(&snapshot, &settings, &options) {
            Ok(facts) => {
                // Facts must survive the cache encoding unchanged.
                let encoded = facts.to_bytes().expect("encode facts");
                let decoded = FileFacts::from_bytes(&encoded).expect("decode facts");
                assert_eq!(decoded, facts);
                let record = File {
                    path: path.clone(),
                    language: LanguageId::TypeScript,
                    state: FileState::Current,
                    hash: Some(snapshot.hash()),
                    failure: None,
                };
                (Some(Arc::new(decoded)), record)
            }
            Err(failure) => {
                failures.push((path.as_str().to_owned(), failure.to_string()));
                let record = File {
                    path: path.clone(),
                    language: LanguageId::TypeScript,
                    state: FileState::Failed,
                    hash: None,
                    failure: Some(failure.to_string().into()),
                };
                (None, record)
            }
        };
        program.push(ProgramFile {
            path: path.clone(),
            language: LanguageId::TypeScript,
            facts,
        });
        records.insert(path.clone(), record);
        snapshots.insert(path, snapshot);
    }
    let program = FactsSet::new(program).expect("distinct paths");
    let linked = language.link(&program, inputs);
    let resolver = Resolver::new(&program, linked.as_ref());
    let mut links: BTreeMap<_, _> = program_links(&resolver).expect("links").into_iter().collect();
    let files: Vec<FileGraph> = program
        .iter()
        .map(|(id, entry)| {
            let record = records[&entry.path].clone();
            match &entry.facts {
                Some(facts) => facts
                    .graph
                    .file_graph(record, links.remove(&id).unwrap_or_default()),
                None => FileGraph {
                    file: record,
                    nodes: Vec::new(),
                    links: Vec::new(),
                    types: unfold_core::FileTypes::default(),
                },
            }
        })
        .collect();
    let graph = Graph {
        files: files.into_iter().map(Arc::new).collect(),
        inputs: Vec::new(),
        apps: Vec::new(),
    };
    let violations = validate_graph(&graph, Some(&snapshots), &[linked.as_ref()]);
    Compiled {
        graph,
        violations,
        failures,
    }
}

/// Compiles one source file named `path`.
pub fn compile_source(path: &str, source: &str) -> Compiled {
    compile(
        &TypeScript::new(),
        &[(path.to_owned(), source.as_bytes().to_vec())],
    )
}

/// The project inputs of a folder as discovery finds them: its `tsconfig*.json` and
/// `package.json` files, the source-like files that aren't TypeScript sources (unindexed), and
/// its declaration files.
pub fn folder_inputs(root: &Path) -> ProjectInputs {
    let mut inputs = ProjectInputs {
        root: Some(root.to_path_buf()),
        ..ProjectInputs::default()
    };
    for (relative, bytes) in walk(root) {
        let path = RepoPath::new(&relative).expect("valid path");
        let name = path.file_name();
        let role = if name == "package.json" {
            Some(InputRole::Package)
        } else if name.starts_with("tsconfig") && name.ends_with(".json") {
            Some(InputRole::Tsconfig)
        } else {
            None
        };
        if let Some(role) = role {
            let snapshot = Snapshot::new(path.clone(), bytes).expect("valid UTF-8");
            inputs.inputs.insert(path, ProjectInput { role, snapshot });
        } else if [".d.ts", ".d.mts", ".d.cts"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            inputs.context.push(path);
        } else if [".js", ".mjs", ".cjs", ".jsx"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            inputs.unindexed.push(path);
        }
    }
    inputs.context.sort();
    inputs.unindexed.sort();
    inputs
}

/// Copies `from` into `to`, without `.unfold/local/`.
pub fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create folder");
    for entry in std::fs::read_dir(from).expect("read folder") {
        let entry = entry.expect("folder entry");
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            if source.ends_with(".unfold/local") {
                continue;
            }
            copy_folder(&source, &target);
        } else {
            std::fs::copy(&source, &target).expect("copy file");
        }
    }
}

/// The TypeScript sources under `root`, as folder-relative paths with their bytes, sorted.
pub fn sources(root: &Path) -> Vec<(String, Vec<u8>)> {
    walk(root)
        .into_iter()
        .filter(|(relative, _)| unfold_lang_typescript::is_typescript_source(relative))
        .collect()
}

/// Every file under `root` outside `node_modules`, `dist`, and `.unfold`, sorted.
fn walk(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).expect("read directory");
        for entry in entries {
            let path = entry.expect("directory entry").path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if !matches!(name, "node_modules" | "dist" | ".unfold") && !name.starts_with(".codecodegraph")
                {
                    pending.push(path);
                }
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            found.push((relative, std::fs::read(&path).expect("read file")));
        }
    }
    found.sort();
    found
}

/// The text of `range` in `source`, with newlines shown as `⏎`.
fn shown(source: &str, range: ByteRange) -> String {
    source[range.start as usize..range.end as usize].replace('\n', "⏎")
}

/// What a link reaches, as one word and its target.
pub fn outcome(target: &LinkTarget) -> String {
    match target {
        LinkTarget::Project(key) => format!("project {key}"),
        LinkTarget::Library(label) => format!("library {}", label.as_str()),
        LinkTarget::Builtin(label) => format!("builtin {}", label.as_str()),
        LinkTarget::Unknown(reason) => format!("unknown {reason}"),
    }
}

/// One link as `kind `text` → outcome`.
pub fn link_line(link: &Link) -> String {
    let implements = if link.implements { " (implements)" } else { "" };
    format!(
        "{} `{}` → {}{implements}",
        link.kind,
        link.text,
        outcome(&link.to)
    )
}

/// The nodes of one file as an indented tree, one line per node: its key step (the key after
/// its parent's), its form and flags, its role and what it is `for`, what it binds, its
/// `break`/`continue` target, its head, and its doc; and each of its links on its own line below
/// it (`↳ kind `text` → outcome`). The module's own flags and doc come first, and its links
/// last.
pub fn tree(file: &FileGraph, source: &str) -> Vec<String> {
    let nodes = &file.nodes;
    let key_of = |id: u32| nodes[id as usize].key.as_str();
    let file_key = key_of(0);
    let mut depth = vec![0usize; nodes.len()];
    let mut lines = Vec::new();
    let module = &nodes[0];
    if module.doc.is_some() || !module.flags.is_empty() {
        let mut line = "(module)".to_owned();
        for flag in module.flags.iter() {
            line.push_str(&format!(" {flag}"));
        }
        if let Some(doc) = module.doc {
            line.push_str(&format!(" doc=`{}`", shown(source, doc)));
        }
        lines.push(line);
    }
    for (id, node) in nodes.iter().enumerate().skip(1) {
        let parent = node.parent.expect("a node other than the module has a parent");
        depth[id] = depth[parent as usize] + 1;
        let indent = "  ".repeat(depth[id] - 1);
        let step = node
            .key
            .as_str()
            .strip_prefix(key_of(parent))
            .unwrap_or(node.key.as_str());
        let mut line = format!("{indent}{step} {}", node.form);
        for flag in node.flags.iter() {
            line.push_str(&format!(" {flag}"));
        }
        if let Some(role) = node.role {
            line.push_str(&format!(" role={role}"));
        }
        if let Some(definition) = node.r#for {
            let key = key_of(definition);
            line.push_str(&format!(" for={}", key.strip_prefix(file_key).unwrap_or(key)));
        }
        if let Some(binds) = &node.binds {
            line.push_str(&format!(" binds=`{binds}`"));
        }
        if let Some(target) = node.target {
            let key = key_of(target);
            line.push_str(&format!(" target={}", key.strip_prefix(file_key).unwrap_or(key)));
        }
        if let Some(head) = node.head {
            line.push_str(&format!(" head=`{}`", shown(source, head)));
        }
        if let Some(doc) = node.doc {
            line.push_str(&format!(" doc=`{}`", shown(source, doc)));
        }
        lines.push(line);
        for link in file.links.iter().filter(|link| link.from as usize == id) {
            lines.push(format!("{indent}  ↳ {}", link_line(link)));
        }
    }
    for link in file.links.iter().filter(|link| link.from == 0) {
        lines.push(format!("↳ {}", link_line(link)));
    }
    lines
}

/// Every field of every node of one file, as text: its key, form, and flags, then each range
/// it records (`range`, `head`, `body`, `doc`, `stmt`, `value`) with its text, its name, what it
/// binds, its role and `for`, and its target.
pub fn outline(file: &FileGraph, source: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for node in &file.nodes {
        let mut line = format!("{} {}", node.key, node.form);
        for flag in node.flags.iter() {
            line.push_str(&format!(" {flag}"));
        }
        lines.push(line);
        let ranges = [
            ("range", Some(node.range)),
            ("head", node.head),
            ("body", node.body),
            ("doc", node.doc),
            ("stmt", node.stmt),
            ("value", node.value),
        ];
        for (field, range) in ranges {
            if let Some(range) = range.filter(|_| node.parent.is_some() || field != "range") {
                lines.push(format!("  {field} `{}`", shown(source, range)));
            }
        }
        if let Some(name) = &node.name {
            lines.push(format!("  name `{name}`"));
        }
        if let Some(binds) = &node.binds {
            lines.push(format!("  binds `{binds}`"));
        }
        if let Some(role) = node.role {
            lines.push(format!("  role {role}"));
        }
        for (field, id) in [("for", node.r#for), ("target", node.target)] {
            if let Some(id) = id {
                lines.push(format!("  {field} {}", file.nodes[id as usize].key));
            }
        }
    }
    lines
}

/// The node tree of one source file named `path`, which must compile into a valid graph, with
/// its links.
pub fn linked_tree_of(path: &str, source: &str) -> Vec<String> {
    let compiled = compile_source(path, source).valid();
    tree(compiled.file(path), source)
}

/// An in-memory folder: TypeScript sources plus the inputs, unindexed files, and declaration
/// files discovery would report.
#[derive(Default, Clone)]
pub struct Project {
    sources: Vec<(String, Vec<u8>)>,
    inputs: ProjectInputs,
    language: TypeScript,
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// A TypeScript source.
    pub fn source(mut self, path: &str, text: &str) -> Self {
        self.sources.push((path.to_owned(), text.as_bytes().to_vec()));
        self
    }

    /// A configuration input (`tsconfig*.json`, any file an `extends` names, or `package.json`).
    pub fn input(mut self, path: &str, text: &str) -> Self {
        let repo = RepoPath::new(path).expect("valid path");
        let role = if repo.file_name() == "package.json" && !path.contains("node_modules/") {
            InputRole::Package
        } else {
            InputRole::Tsconfig
        };
        let snapshot = Snapshot::new(repo.clone(), text.as_bytes().to_vec()).expect("valid UTF-8");
        self.inputs.inputs.insert(repo, ProjectInput { role, snapshot });
        self
    }

    /// An existing file that isn't indexed (excluded, out of scope, or JavaScript).
    pub fn unindexed(mut self, path: &str) -> Self {
        self.inputs
            .unindexed
            .push(RepoPath::new(path).expect("valid path"));
        self.inputs.unindexed.sort();
        self
    }

    /// A declaration file.
    pub fn context(mut self, path: &str) -> Self {
        self.inputs.context.push(RepoPath::new(path).expect("valid path"));
        self.inputs.context.sort();
        self
    }

    /// The project inputs.
    pub fn inputs(&self) -> &ProjectInputs {
        &self.inputs
    }

    /// The language.
    pub fn language(&self) -> &TypeScript {
        &self.language
    }

    /// Compiles the sources with the inputs; the graph must be valid (failed files allowed).
    pub fn compile(&self) -> Compiled {
        let compiled = compile_with(&self.language, &self.sources, &self.inputs);
        let violations: Vec<String> = compiled.violations.iter().map(ToString::to_string).collect();
        assert!(violations.is_empty(), "violations:\n{}", violations.join("\n"));
        compiled
    }

    /// Links the sources and hands the linked program to `check`.
    pub fn link<T>(&self, check: impl FnOnce(&FactsSet, &dyn unfold_engine::Linked) -> T) -> T {
        let settings = Settings::default();
        let files = self.sources.iter().map(|(path, bytes)| {
            let path = RepoPath::new(path).expect("valid path");
            let snapshot = Snapshot::new(path.clone(), bytes.clone()).expect("valid UTF-8");
            let options = self
                .language
                .extraction_options(&[&path], &settings, &self.inputs)
                .pop()
                .expect("options");
            let facts = self
                .language
                .extract_with(&snapshot, &settings, &options)
                .ok()
                .map(Arc::new);
            ProgramFile {
                path,
                language: LanguageId::TypeScript,
                facts,
            }
        });
        let program = FactsSet::new(files).expect("distinct paths");
        let linked = self.language.link(&program, &self.inputs);
        check(&program, linked.as_ref())
    }
}

impl Compiled {
    /// The `imports` link whose specifier text is `text` in `path`.
    #[track_caller]
    pub fn import(&self, path: &str, text: &str) -> &Link {
        let file = self.file(path);
        file.links
            .iter()
            .find(|link| link.kind == LinkKind::Imports && &*link.text == text)
            .unwrap_or_else(|| panic!("no import `{text}` in {path}: {:?}", file.links))
    }

    /// The number of `imports` links of `path`.
    pub fn import_count(&self, path: &str) -> usize {
        self.file(path)
            .links
            .iter()
            .filter(|link| link.kind == LinkKind::Imports)
            .count()
    }
}
