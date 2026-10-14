//! A deliberately unrelated in-memory language that proves the shared backend works only
//! through the `Language`/`Linked` contract: it parses its own syntax, lowers it to nodes keyed
//! with `KeyBuilder`, caches its facts (round-tripping them through postcard), and answers
//! evidence questions over IDs after linking.
//!
//! Alien files claim the `typescript` language id, since the graph model's language vocabulary is
//! closed; nothing TypeScript-specific is involved.

pub mod extract;
pub mod link;
pub mod parse;
pub mod payload;

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::validate::Violation;
use unfold_core::vocab::{FileState, Language as LanguageId, LinkKind};
use unfold_core::{File, FileGraph, Graph, Link, Node, RepoPath, Snapshot};
use unfold_engine::links::{self, program_links};
use unfold_engine::settings::{Settings, SettingsHash, SettingsHasher};
use unfold_engine::validate::validate_graph;
use unfold_engine::{
    FactsSet, FileFacts, Language, Linked, Producer, ProgramFile, ProjectInputs, Resolver, SourceFailure,
};

pub struct Alien;

/// The extensions alien files use: `.alien` in memory, and `.ts` in folders compiled through
/// discovery (which classifies sources by extension; nothing TypeScript-specific is involved).
pub const EXTENSIONS: &[&str] = &["alien", "ts"];

/// Whether `path` is an alien file.
pub fn is_alien(path: &RepoPath) -> bool {
    path.extension()
        .is_some_and(|extension| EXTENSIONS.contains(&extension))
}

pub struct AlienHighlighter;

impl Highlighter for AlienHighlighter {
    fn language(&self) -> LanguageId {
        LanguageId::TypeScript
    }

    fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
        let Ok(syntax) = parse::parse(text) else {
            return Vec::new();
        };
        let mut spans: Vec<HighlightSpan> = syntax
            .nodes
            .iter()
            .skip(1)
            .filter(|node| !node.form)
            .map(|node| HighlightSpan {
                range: node.range,
                class: if node.text.starts_with('"') {
                    TokenClass::String
                } else if node.text.starts_with(':') {
                    TokenClass::Constant
                } else {
                    TokenClass::Variable
                },
            })
            .collect();
        spans.sort();
        spans
    }
}

impl Language for Alien {
    fn id(&self) -> LanguageId {
        LanguageId::TypeScript
    }

    fn producer(&self) -> Producer {
        Producer {
            name: "alien",
            version: "1",
        }
    }

    fn owns(&self, path: &RepoPath, _settings: &Settings) -> bool {
        is_alien(path)
    }

    fn settings_hash(&self, _path: &RepoPath, _settings: &Settings) -> SettingsHash {
        SettingsHasher::new("alien").finish()
    }

    fn extract(&self, file: &Snapshot, _settings: &Settings) -> Result<FileFacts, SourceFailure> {
        extract::extract(file)
    }

    fn reads_payload(&self, payload: &[u8]) -> bool {
        postcard::from_bytes::<payload::Payload>(payload).is_ok()
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        Box::new(link::AlienLinked::new(files, &inputs.unindexed))
    }

    fn highlighter(&self) -> &dyn Highlighter {
        &AlienHighlighter
    }
}

/// A small project to compile with the alien language.
#[derive(Default, Clone)]
pub struct Project {
    files: Vec<(String, String)>,
    unindexed: Vec<RepoPath>,
}

/// A compiled project: the graph and what validation said about it.
pub struct Compiled {
    pub graph: Graph,
    pub violations: Vec<Violation>,
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(mut self, path: &str, text: &str) -> Self {
        self.files.push((path.to_owned(), text.to_owned()));
        self
    }

    /// A repository file that exists but is outside the indexed scope.
    pub fn unindexed(mut self, path: &str) -> Self {
        self.unindexed.push(RepoPath::new(path).expect("valid path"));
        self
    }

    /// The same files in reverse order (the program must not care).
    pub fn reversed(mut self) -> Self {
        self.files.reverse();
        self
    }

    pub fn compile(&self) -> Compiled {
        self.run(None)
    }

    /// Validates `graph` against this project's sources and the alien census.
    pub fn validate(&self, graph: &Graph) -> Vec<Violation> {
        self.run(Some(graph)).violations
    }

    fn run(&self, check: Option<&Graph>) -> Compiled {
        let alien = Alien;
        let settings = Settings::default();
        let mut snapshots = BTreeMap::new();
        let mut records = BTreeMap::new();
        let mut files = Vec::new();
        for (path, text) in &self.files {
            let path = RepoPath::new(path).expect("valid path");
            assert!(alien.owns(&path, &settings));
            let snapshot = Snapshot::from_text(path.clone(), text).expect("valid text");
            let (facts, record) = match alien.extract(&snapshot, &settings) {
                Ok(facts) => {
                    // Facts must survive the cache encoding unchanged.
                    let bytes = facts.to_bytes().expect("encode facts");
                    let decoded = FileFacts::from_bytes(&bytes).expect("decode facts");
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
            files.push(ProgramFile {
                path: path.clone(),
                language: LanguageId::TypeScript,
                facts,
            });
            records.insert(path.clone(), record);
            snapshots.insert(path, snapshot);
        }
        let program = FactsSet::new(files).expect("distinct paths");
        let project_inputs = ProjectInputs {
            settings,
            unindexed: self.unindexed.clone(),
            ..ProjectInputs::default()
        };
        let linked = alien.link(&program, &project_inputs);
        let resolver = Resolver::new(&program, linked.as_ref());
        let mut resolved: BTreeMap<_, _> = program_links(&resolver).expect("links").into_iter().collect();
        // Resolving one file on this thread agrees with the parallel whole-program pass.
        for (file, file_links) in &resolved {
            assert_eq!(
                &links::file_links(&resolver, *file).expect("file links"),
                file_links
            );
        }
        let graphs: Vec<FileGraph> = program
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
                        types: unfold_core::FileTypes::default(),
                    },
                }
            })
            .collect();
        let graph = Graph {
            files: graphs.into_iter().map(Arc::new).collect(),
            inputs: Vec::new(),
            apps: Vec::new(),
        };
        let violations = validate_graph(check.unwrap_or(&graph), Some(&snapshots), &[linked.as_ref()]);
        Compiled { graph, violations }
    }
}

impl Compiled {
    /// Asserts the graph passed validation.
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

    /// The `calls` links of `path`, in node order.
    pub fn calls(&self, path: &str) -> Vec<&Link> {
        self.file(path)
            .links
            .iter()
            .filter(|link| link.kind == LinkKind::Calls)
            .collect()
    }

    /// The first `calls` link whose text is `text`, in node order.
    pub fn call(&self, path: &str, text: &str) -> &Link {
        self.calls(path)
            .into_iter()
            .find(|link| &*link.text == text)
            .unwrap_or_else(|| panic!("no call `{text}` in {path}"))
    }

    /// The node a link starts at, in `path`.
    pub fn from(&self, path: &str, link: &Link) -> &Node {
        &self.file(path).nodes[link.from as usize]
    }

    pub fn canonical(&self) -> String {
        unfold_core::canonical::to_string(&self.graph)
    }
}
