//! Compiling Python sources end to end for tests: in memory (extraction with a facts-cache round
//! trip, linking, edges, validation against the snapshots and the Python census), or a folder
//! through the engine's `update()`, exactly as the app compiles it.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use unfold_core::validate::Violation;
use unfold_core::vocab::{FileState, Language as LanguageId};
use unfold_core::{FileGraph, FileRecord, Graph, RepoPath, Snapshot};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::edges::program_links;
use unfold_engine::settings::Settings;
use unfold_engine::validate::validate_graph;
use unfold_engine::{
    BlockRef, Engine, EngineOptions, FactsSet, FileFacts, Language, MemoryFactsCache, ProgramFile,
    ProjectInputs, Resolver, UpdateError, UpdateRequest,
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

    /// The text of the source at `path`.
    pub fn text(&self, path: &str) -> &str {
        self.sources
            .iter()
            .find(|(source, _)| source.as_str() == path)
            .map(|(_, snapshot)| snapshot.text())
            .unwrap_or_else(|| panic!("no source {path}"))
    }

    /// The graph of the file at `path`.
    pub fn file(&self, path: &str) -> &FileGraph {
        self.graph.file(path).unwrap_or_else(|| panic!("no file {path}"))
    }

    /// The canonical records of one type (`file`, `block`, `slot`), in export order.
    pub fn records(&self, kinds: &[&str]) -> Vec<String> {
        canonical_records(&unfold_core::canonical::to_string(&self.graph), kinds)
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
    compile_snapshots(snapshots, unindexed, settings)
}

/// Compiles the folder at `root` with the engine, scoped by its `.unfold/config.toml`. The folder
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

/// Compiles snapshots: extract (round-tripping facts through the cache encoding), link, resolve,
/// assemble the graph, and validate it.
pub fn compile_snapshots(
    snapshots: BTreeMap<RepoPath, Snapshot>,
    unindexed: Vec<RepoPath>,
    settings: &Settings,
) -> Compiled {
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
                let record = FileRecord {
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
                let record = FileRecord {
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
    let inputs = ProjectInputs {
        settings: settings.clone(),
        unindexed,
        ..ProjectInputs::default()
    };
    let linked = python.link(&program, &inputs);
    let resolver = Resolver::new(&program, linked.as_ref());
    let mut links: BTreeMap<_, _> = program_links(&resolver).expect("links").into_iter().collect();
    let graphs = program
        .iter()
        .map(|(id, entry)| {
            let record = records[&entry.path].clone();
            match &entry.facts {
                Some(facts) => facts
                    .graph
                    .linked_file_graph(record, links.remove(&id).unwrap_or_default(), |local| {
                        linked.keeps_import(BlockRef { file: id, local })
                    })
                    .expect("renumbered facts"),
                None => FileGraph {
                    file: record,
                    blocks: Vec::new(),
                    slots: Vec::new(),
                    edges: Vec::new(),
                    types: unfold_core::FileTypes::default(),
                },
            }
        })
        .collect();
    let graph = Graph {
        files: graphs,
        inputs: Vec::new(),
        entrances: Vec::new(),
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

/// Renders a graph's entrances in the `expected-entrances.txt` format (`testdata/README.md`):
/// one `entrance` line per entrance, followed by its `target` lines in position order. Evidence
/// lines are read from the files under `root`.
pub fn render_entrances(root: &Path, graph: &Graph) -> String {
    use unfold_core::LineIndex;
    use unfold_core::vocab::Resolution;
    let mut out = String::new();
    for entrance in &graph.entrances {
        let evidence = &entrance.evidence;
        let text = std::fs::read_to_string(root.join(evidence.path.as_str())).unwrap_or_default();
        let line = LineIndex::new(&text).line_of(evidence.range.start);
        let method = entrance
            .method
            .map_or_else(|| "-".to_owned(), |method| method.to_string());
        let trigger = entrance.trigger.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "entrance\t{}\t{}\t{method}\t{trigger}\t{}\t{}:{line}\n",
            entrance.detector, entrance.kind, entrance.trigger_status, evidence.path
        ));
        for target in &entrance.targets {
            let what = match target.resolution {
                Resolution::Resolved => target
                    .target
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                Resolution::External => target
                    .module
                    .as_ref()
                    .map(|module| module.as_str().to_owned())
                    .unwrap_or_default(),
                Resolution::Unresolved => target.reason.map(|reason| reason.to_string()).unwrap_or_default(),
            };
            out.push_str(&format!(
                "\ttarget\t{}\t{}\t{what}\t{}\n",
                target.role, target.resolution, target.text
            ));
        }
    }
    out
}

/// The records of an `expected-entrances.txt` file: every line but comments and blank lines.
pub fn entrance_records(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}
