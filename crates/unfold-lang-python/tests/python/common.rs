//! Compiling Python sources end to end for tests: in memory (extraction with a facts-cache round
//! trip, linking, links, validation against the snapshots and the Python census), or a folder
//! through the engine's `update()`, exactly as the app compiles it; and the readable tree the
//! lowering tests compare.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use unfold_core::validate::Violation;
use unfold_core::vocab::{FileState, Language as LanguageId, LinkKind};
use unfold_core::{File, FileGraph, FileTypes, Graph, LinkTarget, Node, RepoPath, Snapshot};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::links::{self, program_links};
use unfold_engine::settings::Settings;
use unfold_engine::validate::validate_graph;
use unfold_engine::{
    Engine, EngineOptions, FactsSet, FileFacts, Language, MemoryFactsCache, ProgramFile, ProjectInputs,
    Resolver, UpdateError, UpdateRequest,
};
use unfold_lang_python::Python;

/// A compiled set of Python files.
pub struct Compiled {
    pub graph: Graph,
    pub violations: Vec<Violation>,
    /// Failed files with their failure messages.
    pub failures: Vec<(String, String)>,
    /// The compiled sources.
    pub sources: BTreeMap<RepoPath, Snapshot>,
}

impl Compiled {
    /// Asserts validation passed.
    #[track_caller]
    pub fn valid(self) -> Self {
        assert!(
            self.violations.is_empty(),
            "validation failed:\n{}",
            self.violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
        self
    }

    /// The graph of the file at `path`.
    pub fn file(&self, path: &str) -> &FileGraph {
        self.graph.file(path).unwrap_or_else(|| panic!("no file {path}"))
    }

    /// The node with `key`, in any file.
    pub fn node(&self, key: &str) -> &Node {
        self.graph
            .files
            .iter()
            .flat_map(|file| &file.nodes)
            .find(|node| node.key.as_str() == key)
            .unwrap_or_else(|| panic!("no node {key}"))
    }

    /// The source of the file at `path`.
    pub fn text(&self, path: &str) -> &str {
        self.sources
            .iter()
            .find(|(source, _)| source.as_str() == path)
            .map(|(_, snapshot)| snapshot.text())
            .unwrap_or_else(|| panic!("no source {path}"))
    }

    /// The readable tree of the file at `path` ([`tree`]).
    pub fn tree(&self, path: &str) -> String {
        tree(self.file(path), self.text(path))
    }
}

/// The lines of a canonical export whose record type is one of `kinds`.
pub fn canonical_records(export: &str, kinds: &[&str]) -> Vec<String> {
    export
        .lines()
        .filter(|line| {
            kinds
                .iter()
                .any(|kind| line.starts_with(&format!("{{\"t\":\"{kind}\"")))
        })
        .map(str::to_owned)
        .collect()
}

/// Compiles in-memory files with the default settings and asserts validation passed.
#[track_caller]
pub fn compile(files: &[(&str, &str)]) -> Compiled {
    compile_files(files, &Settings::default()).valid()
}

/// Compiles in-memory files with `settings`.
pub fn compile_files(files: &[(&str, &str)], settings: &Settings) -> Compiled {
    compile_files_with(files, &[], settings)
}

/// Compiles in-memory files with `settings`, where `unindexed` are repository files that exist
/// but are not compiled.
pub fn compile_files_with(files: &[(&str, &str)], unindexed: &[&str], settings: &Settings) -> Compiled {
    let snapshots = files
        .iter()
        .map(|(path, text)| {
            let path = RepoPath::new(path).expect("valid path");
            let snapshot = Snapshot::from_text(path.clone(), text).expect("valid text");
            (path, snapshot)
        })
        .collect();
    let mut unindexed: Vec<RepoPath> = unindexed
        .iter()
        .map(|path| RepoPath::new(path).expect("valid path"))
        .collect();
    unindexed.sort();
    let inputs = ProjectInputs {
        settings: settings.clone(),
        unindexed,
        ..ProjectInputs::default()
    };
    compile_with_inputs(snapshots, &inputs)
}

/// Compiles the folder at `root` with the engine, scoped by its `unfold.toml`. The folder
/// is copied to a temporary directory first, so nothing can write into `testdata/`.
pub fn compile_folder(root: &Path) -> Compiled {
    let temporary = tempfile::tempdir().expect("a temporary directory");
    copy_folder(root, temporary.path());
    compile_folder_with_threads(temporary.path(), 1)
}

/// Compiles the folder at `root` in place with the engine, on `threads` threads.
pub fn compile_folder_with_threads(root: &Path, threads: usize) -> Compiled {
    let options = EngineOptions {
        discover: DiscoverOptions {
            threads: 1,
            global_ignore: false,
        },
        compile_threads: threads,
        ..EngineOptions::default()
    };
    let mut engine = Engine::new(
        root,
        vec![Arc::new(Python)],
        Arc::new(MemoryFactsCache::new()),
        options,
    )
    .expect("an engine for the folder");
    let (graph, violations) = match engine.update(UpdateRequest::compile()) {
        Ok(outcome) => ((*outcome.graph).clone(), Vec::new()),
        Err(UpdateError::Validation(violations)) => (Graph::default(), violations),
        Err(error) => panic!("the compile failed: {error}"),
    };
    let failures = graph
        .files
        .iter()
        .filter(|file| file.file.state == FileState::Failed)
        .map(|file| {
            let message = file.file.failure.as_deref().unwrap_or_default();
            (file.file.path.as_str().to_owned(), message.to_owned())
        })
        .collect();
    let sources = graph
        .files
        .iter()
        .filter_map(|file| {
            let bytes = std::fs::read(root.join(file.file.path.as_str())).ok()?;
            Some((
                file.file.path.clone(),
                Snapshot::new(file.file.path.clone(), bytes).ok()?,
            ))
        })
        .collect();
    Compiled {
        graph,
        violations,
        failures,
        sources,
    }
}

/// Copies the files under `from` to `to`, recursively.
pub fn copy_folder(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read a folder") {
        let entry = entry.expect("a folder entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            std::fs::create_dir_all(&target).expect("create a folder");
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

/// Compiles snapshots with every project input given: extract (round-tripping facts through
/// the cache encoding), link, resolve, assemble the graph, and validate it.
pub fn compile_with_inputs(snapshots: BTreeMap<RepoPath, Snapshot>, inputs: &ProjectInputs) -> Compiled {
    let settings = &inputs.settings;
    let python = Python;
    let mut files = Vec::new();
    let mut records = BTreeMap::new();
    let mut failures = Vec::new();
    for (path, snapshot) in &snapshots {
        assert!(python.owns(path, settings), "{path} is not a Python source");
        let (facts, record) = match python.extract(snapshot, settings) {
            Ok(facts) => {
                let bytes = facts.to_bytes().expect("encode facts");
                let decoded = FileFacts::from_bytes(&bytes).expect("decode facts");
                assert_eq!(decoded, facts, "facts survive the cache encoding");
                let record = File {
                    path: path.clone(),
                    language: LanguageId::Python,
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
                    language: LanguageId::Python,
                    state: FileState::Failed,
                    hash: None,
                    failure: Some(failure.to_string().into()),
                };
                (None, record)
            }
        };
        files.push(ProgramFile {
            path: path.clone(),
            language: LanguageId::Python,
            facts,
        });
        records.insert(path.clone(), record);
    }
    let program = FactsSet::new(files).expect("distinct paths");
    let linked = python.link(&program, inputs);
    let resolver = Resolver::new(&program, linked.as_ref());
    let mut resolved: BTreeMap<_, _> = program_links(&resolver).expect("links").into_iter().collect();
    // Resolving one file on this thread agrees with the parallel whole-program pass.
    for (file, file_links) in &resolved {
        assert_eq!(
            &links::file_links(&resolver, *file).expect("file links"),
            file_links
        );
    }
    let graphs = program
        .iter()
        .map(|(id, entry)| {
            let record = records[&entry.path].clone();
            match &entry.facts {
                Some(facts) => facts
                    .graph
                    .file_graph(record, resolved.remove(&id).unwrap_or_default()),
                None => FileGraph {
                    file: record,
                    nodes: Vec::new(),
                    links: Vec::new(),
                    types: FileTypes::default(),
                },
            }
        })
        .collect();
    let graph = Graph {
        files: graphs,
        inputs: Vec::new(),
        apps: Vec::new(),
    };
    let violations = validate_graph(&graph, Some(&snapshots), &[linked.as_ref()]);
    Compiled {
        graph,
        violations,
        failures,
        sources: snapshots,
    }
}

/// The repository's `testdata/` directory.
pub fn testdata() -> std::path::PathBuf {
    unfold_core::testkit::testdata_dir()
}

/// A link's target as the tree writes it: `project <key>`, `library <label>`, `builtin`, or
/// `unknown (<reason>)`.
pub fn target(to: &LinkTarget) -> String {
    match to {
        LinkTarget::Project(key) => format!("project {key}"),
        LinkTarget::Library(label) => format!("library {}", label.as_str()),
        LinkTarget::Builtin(_) => "builtin".to_owned(),
        LinkTarget::Unknown(reason) => format!("unknown ({reason})"),
    }
}

/// A file's nodes as a readable tree, one node per line in preorder, indented by depth: the
/// key's last step (the whole key for the module), the form, and the notable fields: `name`
/// when it differs from the step, `head`, `binds`, `role` and `for`, `stmt` and `value` (when
/// they differ from the node's own text), `target`, flags, and each link (`calls → builtin`,
/// `types → project shop/payments.py#PaymentDeclined`).
pub fn tree(file: &FileGraph, text: &str) -> String {
    let text_of = |range: unfold_core::ByteRange| -> String {
        text.get(range.as_usize()).map_or_else(
            || range.to_string(),
            |written| written.split_whitespace().collect::<Vec<_>>().join(" "),
        )
    };
    let source = |range: unfold_core::ByteRange| -> String { format!("`{}`", text_of(range)) };
    let mut out = String::new();
    let mut depths: Vec<usize> = Vec::with_capacity(file.nodes.len());
    for (index, node) in file.nodes.iter().enumerate() {
        let depth = node.parent.map_or(0, |parent| depths[parent as usize] + 1);
        depths.push(depth);
        let key = node.key.as_str();
        let step = match node.parent {
            None => key.to_owned(),
            Some(parent) => {
                let parent_key = file.nodes[parent as usize].key.as_str();
                key.strip_prefix(parent_key)
                    .map_or(key, |rest| rest.trim_start_matches(['/', '.', '#']))
                    .to_owned()
            }
        };
        let mut line = format!("{}{step} {}", "  ".repeat(depth), node.form);
        if node.parent.is_some()
            && let Some(name) = &node.name
            && !step.contains(&**name)
        {
            let _ = write!(line, " name={name}");
        }
        let range_text = text_of(node.range);
        if let Some(head) = node
            .head
            .filter(|_| node.kind() != unfold_core::vocab::NodeKind::Call)
        {
            let _ = write!(line, " head={}", source(head));
        }
        if let Some(binds) = &node.binds {
            let _ = write!(line, " binds={binds}");
        }
        if let Some(role) = node.role {
            let _ = write!(line, " role={role}");
        }
        if let Some(definition) = node.r#for {
            let _ = write!(
                line,
                " for={}",
                file.nodes[definition as usize].name.as_deref().unwrap_or("?")
            );
        }
        if let Some(stmt) = node.stmt {
            if text_of(stmt) == range_text {
                line.push_str(" stmt");
            } else {
                let _ = write!(line, " stmt={}", source(stmt));
            }
        }
        if let Some(value) = node.value {
            let _ = write!(line, " value={}", source(value));
        }
        if let Some(target) = node.target {
            let _ = write!(line, " target={}", file.nodes[target as usize].key);
        }
        for flag in node.flags.iter() {
            let _ = write!(line, " {flag}");
        }
        if node.doc.is_some() {
            line.push_str(" doc");
        }
        let id = u32::try_from(index).expect("few nodes");
        for link in file.links.iter().filter(|link| link.from == id) {
            let kind = match link.kind {
                LinkKind::Calls => "→".to_owned(),
                other => format!("{other} {} →", link.text),
            };
            let _ = write!(line, " [{kind} {}]", target(&link.to));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
