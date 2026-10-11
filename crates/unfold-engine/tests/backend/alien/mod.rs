//! A deliberately unrelated in-memory language that proves the shared backend works only
//! through the `Language`/`Linked` contract: it parses its own syntax, drives the shared
//! selection, caches its facts (round-tripping them through postcard), and answers evidence
//! questions over IDs after linking.
//!
//! Alien files claim the `typescript` language id, since the graph model's language vocabulary is
//! closed; nothing TypeScript-specific is involved.

pub mod detect;
pub mod extract;
pub mod link;
pub mod parse;
pub mod payload;

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::validate::Violation;
use unfold_core::vocab::{BlockKind, EdgeKind, FileState, InputRole, Language as LanguageId};
use unfold_core::{Block, Edge, Entrance, FileGraph, FileRecord, Graph, InputRecord, RepoPath, Snapshot};
use unfold_engine::edges::{self, program_links};
use unfold_engine::entrances::{self, TargetResolver};
use unfold_engine::settings::{Settings, SettingsHash, SettingsHasher};
use unfold_engine::validate::validate_graph;
use unfold_engine::{
    FactsSet, FileFacts, Language, Linked, Outcome, Producer, ProgramFile, ProjectInputs, Resolver,
    SourceFailure,
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

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        Box::new(link::AlienLinked::new(files, &inputs.unindexed))
    }

    fn highlighter(&self) -> &dyn Highlighter {
        &AlienHighlighter
    }
}

/// Launches resolve only to indexed alien files, by exact path; missing files consult the
/// unindexed list.
struct Targets<'a> {
    program: &'a FactsSet,
    unindexed: &'a [RepoPath],
}

impl TargetResolver for Targets<'_> {
    fn is_unindexed_file(&self, path: &RepoPath) -> bool {
        self.unindexed.contains(path)
    }

    fn launch(&self, path: &str, cwd: &str) -> Outcome {
        let joined = if cwd.is_empty() || cwd == "." {
            path.to_owned()
        } else {
            format!("{cwd}/{path}")
        };
        match self.program.find(&joined) {
            Some(file) if self.program.facts(file).is_some() => {
                Outcome::Resolved(unfold_engine::BlockRef { file, local: 0 })
            }
            _ => Outcome::unresolved(unfold_core::vocab::Reason::NotFound),
        }
    }
}

/// A small project to compile with the alien language.
#[derive(Default, Clone)]
pub struct Project {
    files: Vec<(String, String)>,
    unindexed: Vec<RepoPath>,
    config: Option<String>,
}

/// A compiled project: the graph and what validation said about it.
pub struct Compiled {
    pub graph: Graph,
    pub violations: Vec<Violation>,
}

pub const CONFIG: &str = ".unfold/config.toml";

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

    /// `.unfold/config.toml`, whose `[[entrance]]` tables become manual entrances.
    pub fn config(mut self, text: &str) -> Self {
        self.config = Some(text.to_owned());
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
        let settings = match &self.config {
            Some(text) => Settings::parse(text).expect("valid settings"),
            None => Settings::default(),
        };
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
                    let record = FileRecord {
                        path: path.clone(),
                        language: LanguageId::TypeScript,
                        state: FileState::Current,
                        hash: Some(snapshot.hash()),
                        failure: None,
                    };
                    (Some(Arc::new(decoded)), record)
                }
                Err(failure) => {
                    let record = FileRecord {
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
        let mut inputs = Vec::new();
        if let Some(text) = &self.config {
            let path = RepoPath::new(CONFIG).expect("valid path");
            let snapshot = Snapshot::from_text(path.clone(), text).expect("valid text");
            inputs.push(InputRecord {
                path: path.clone(),
                role: InputRole::UnfoldConfig,
                hash: snapshot.hash(),
            });
            snapshots.insert(path, snapshot);
        }
        let program = FactsSet::new(files).expect("distinct paths");
        let project_inputs = ProjectInputs {
            settings: settings.clone(),
            inputs: BTreeMap::new(),
            unindexed: self.unindexed.clone(),
            ..ProjectInputs::default()
        };
        let linked = alien.link(&program, &project_inputs);
        let resolver = Resolver::new(&program, linked.as_ref());
        let mut links: BTreeMap<_, _> = program_links(&resolver).expect("links").into_iter().collect();
        // Resolving one file on this thread agrees with the parallel whole-program pass.
        for (file, file_links) in &links {
            assert_eq!(
                &edges::file_links(&resolver, *file).expect("file links"),
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
                        .linked_file_graph(record, links.remove(&id).unwrap_or_default(), |local| {
                            linked.keeps_import(unfold_engine::BlockRef { file: id, local })
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
        // Detection order: manual declarations first, then the language's detectors.
        let mut candidates = entrances::manual_candidates(&settings);
        candidates.extend(entrances::detect(&resolver, &settings));
        let targets = Targets {
            program: &program,
            unindexed: &self.unindexed,
        };
        let entrances = entrances::assemble(&program, &[&resolver], &targets, candidates).expect("entrances");
        let graph = Graph {
            files: graphs,
            inputs,
            entrances,
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

    pub fn block(&self, key: &str) -> &Block {
        self.graph
            .files
            .iter()
            .flat_map(|file| &file.blocks)
            .find(|block| block.key.as_str() == key)
            .unwrap_or_else(|| panic!("no block {key}"))
    }

    /// The occurrence edges of `path`, in block order.
    pub fn occurrences(&self, path: &str) -> Vec<&Edge> {
        self.file(path)
            .edges
            .iter()
            .filter(|edge| edge.kind.is_occurrence())
            .collect()
    }

    /// The call or construction edge whose text is `text` (the first, in block order).
    pub fn edge(&self, path: &str, text: &str) -> &Edge {
        self.file(path)
            .edges
            .iter()
            .find(|edge| matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs) && &*edge.text == text)
            .unwrap_or_else(|| panic!("no occurrence `{text}` in {path}"))
    }

    pub fn entrances(&self) -> &[Entrance] {
        &self.graph.entrances
    }

    pub fn canonical(&self) -> String {
        unfold_core::canonical::to_string(&self.graph)
    }

    /// The kind and key of a resolved edge's target.
    pub fn target(&self, edge: &Edge) -> Option<(BlockKind, &str)> {
        let block = self.block(edge.target.as_ref()?.as_str());
        Some((block.kind, block.key.as_str()))
    }
}
