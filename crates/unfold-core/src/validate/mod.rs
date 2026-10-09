//! The graph validator.
//!
//! Every compile and update validates the new graph before committing it. The validator checks
//! the graph's own invariants (vocabularies per language, ranges, preorder, slots, occurrence
//! edges, references, targets, reasons and modules, entrances, keys) and, when the caller
//! supplies them, the source snapshots the graph came from and an independent [`Census`] of
//! token boundaries, call sites, and module dependencies from each language front end.
//!
//! It never panics and reports every violation it finds, each naming the record it concerns.

mod census;
mod edges;
mod entrances;
mod files;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::key::Key;
use crate::model::{FileGraph, Graph};
use crate::path::RepoPath;
use crate::text::{ByteRange, Snapshot};
use crate::vocab::{BlockKind, EdgeKind, Language, SlotName};

/// The record a [`Violation`] concerns.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subject {
    /// The graph as a whole.
    Graph,
    /// A file record.
    File(RepoPath),
    /// A block.
    Block(Key),
    /// A slot of a block.
    Slot {
        /// The owning block.
        block: Key,
        /// The slot's name.
        slot: SlotName,
        /// The slot's index, if repeated.
        index: Option<u32>,
    },
    /// An edge.
    Edge {
        /// The source block.
        source: Key,
        /// The edge's sequence number.
        seq: u32,
    },
    /// A configuration input.
    Input(RepoPath),
    /// An entrance.
    Entrance(Key),
    /// An entrance target.
    Target {
        /// The entrance.
        entrance: Key,
        /// The target's position.
        position: u32,
    },
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graph => f.write_str("graph"),
            Self::File(path) => write!(f, "file {path}"),
            Self::Block(key) => write!(f, "block {key}"),
            Self::Slot {
                block,
                slot,
                index: Some(index),
            } => write!(f, "slot {slot}[{index}] of {block}"),
            Self::Slot {
                block,
                slot,
                index: None,
            } => write!(f, "slot {slot} of {block}"),
            Self::Edge { source, seq } => write!(f, "edge {seq} of {source}"),
            Self::Input(path) => write!(f, "input {path}"),
            Self::Entrance(key) => write!(f, "entrance {key}"),
            Self::Target { entrance, position } => write!(f, "target {position} of {entrance}"),
        }
    }
}

/// One broken invariant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Violation {
    /// The record it concerns.
    pub subject: Subject,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.subject, self.message)
    }
}

/// An independent view of one language's syntax, computed by its front end from the same
/// bytes, so validation does not just trust the extractor. Files are validated in parallel,
/// so a census is shared across threads.
pub trait Census: Sync {
    /// The language this census covers.
    fn language(&self) -> Language;

    /// Whether `range` in `path` starts at the first byte of a token and ends immediately
    /// after the last byte of a token.
    fn is_token_range(&self, path: &RepoPath, range: ByteRange) -> bool;

    /// Every call site in `path` (calls, tagged templates, and constructions), or `None` if
    /// this census does not enumerate them.
    fn call_sites(&self, _path: &RepoPath) -> Option<Vec<CallSite>> {
        None
    }

    /// The range of every runtime module dependency in `path`, or `None` if this census does
    /// not enumerate them.
    fn module_dependencies(&self, _path: &RepoPath) -> Option<Vec<ByteRange>> {
        None
    }
}

/// One call site found by a [`Census`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// The site's token-exact range.
    pub range: ByteRange,
    /// The block kind it must have: `call` or `new`.
    pub block_kind: BlockKind,
    /// The occurrence edge kind it must have, when the front end knows it independently (for
    /// example, `constructs` for a Python call with proven class identity).
    pub edge_kind: Option<EdgeKind>,
    /// Argument positions that can never supply a named reference (for example, expanded
    /// `*args` in Python).
    pub ineligible_references: Vec<u32>,
}

/// Looks up the snapshot of a source file or configuration input by path (shared across the
/// threads that validate files).
pub trait Sources: Sync {
    /// The snapshot at `path`, if supplied.
    fn snapshot(&self, path: &RepoPath) -> Option<&Snapshot>;
}

impl<S: BuildHasher + Sync> Sources for HashMap<RepoPath, Snapshot, S> {
    fn snapshot(&self, path: &RepoPath) -> Option<&Snapshot> {
        self.get(path)
    }
}

impl Sources for BTreeMap<RepoPath, Snapshot> {
    fn snapshot(&self, path: &RepoPath) -> Option<&Snapshot> {
        self.get(path)
    }
}

/// Validates a graph, optionally against snapshots and language censuses.
#[derive(Default)]
pub struct Validator<'a> {
    sources: Option<&'a dyn Sources>,
    censuses: Vec<&'a dyn Census>,
}

impl fmt::Debug for Validator<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Validator")
            .field("sources", &self.sources.is_some())
            .field(
                "censuses",
                &self
                    .censuses
                    .iter()
                    .map(|census| census.language())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl<'a> Validator<'a> {
    /// A validator that checks only the graph's own invariants.
    pub fn new() -> Self {
        Self::default()
    }

    /// Also checks every file and input against its snapshot: fingerprints, the root's
    /// length, lines, character boundaries, surrounding trivia, and evidence ranges. Files and
    /// inputs without a snapshot are reported.
    pub fn with_sources(mut self, sources: &'a dyn Sources) -> Self {
        self.sources = Some(sources);
        self
    }

    /// Also checks the files of `census.language()` against that census.
    pub fn with_census(mut self, census: &'a dyn Census) -> Self {
        self.censuses.push(census);
        self
    }

    /// Checks `graph` and returns every violation, in a deterministic order.
    ///
    /// Files are checked in parallel (on the current rayon pool) and their violations
    /// reassembled in file order.
    pub fn validate(&self, graph: &Graph) -> Vec<Violation> {
        let (index, duplicates) = Index::new(graph);
        let mut checker = Checker::new(graph, self.sources, &index);
        checker.violations = duplicates;
        checker.check_graph_order();
        let per_file: Vec<Vec<Violation>> = graph
            .files
            .par_iter()
            .map(|file| {
                let mut checker = Checker::new(graph, self.sources, &index);
                files::check(&mut checker, file);
                edges::check(&mut checker, file);
                if let Some(census) = self
                    .censuses
                    .iter()
                    .find(|census| census.language() == file.file.language)
                {
                    census::check(&mut checker, file, *census);
                }
                checker.violations
            })
            .collect();
        checker.violations.extend(per_file.into_iter().flatten());
        checker.check_inputs();
        entrances::check(&mut checker);
        checker.violations
    }
}

/// Validates `graph` against its own invariants only.
pub fn validate(graph: &Graph) -> Vec<Violation> {
    Validator::new().validate(graph)
}

/// Where a block lives: its file's index in the graph and its local ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockAt {
    file: usize,
    local_id: u32,
}

/// The graph-wide lookups every check reads: built once, shared by the file checks.
struct Index<'g> {
    /// Every block key in the graph.
    blocks: FxHashMap<&'g str, BlockAt>,
    /// File paths to their index in `graph.files`.
    files: FxHashMap<&'g str, usize>,
    /// Input paths to their index in `graph.inputs`.
    inputs: FxHashMap<&'g str, usize>,
    /// Classes that declare a direct constructor.
    constructed_classes: FxHashSet<(usize, u32)>,
}

impl<'g> Index<'g> {
    /// The lookups of `graph`, and a violation for every duplicate structural key.
    fn new(graph: &'g Graph) -> (Self, Vec<Violation>) {
        let blocks: usize = graph.files.iter().map(|file| file.blocks.len()).sum();
        let mut index = Self {
            blocks: FxHashMap::with_capacity_and_hasher(blocks, Default::default()),
            files: FxHashMap::with_capacity_and_hasher(graph.files.len(), Default::default()),
            inputs: graph
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| (input.path.as_str(), index))
                .collect(),
            constructed_classes: FxHashSet::default(),
        };
        let mut duplicates = Vec::new();
        for (file_index, file) in graph.files.iter().enumerate() {
            index.files.insert(file.file.path.as_str(), file_index);
            for (local_id, block) in file.blocks.iter().enumerate() {
                let at = BlockAt {
                    file: file_index,
                    local_id: u32::try_from(local_id).unwrap_or(u32::MAX),
                };
                if index.blocks.insert(block.key.as_str(), at).is_some() {
                    duplicates.push(Violation {
                        subject: Subject::Block(block.key.clone()),
                        message: "duplicate structural key".to_owned(),
                    });
                }
                if block.kind == BlockKind::Constructor
                    && let Some(parent) = block.parent.and_then(|parent| file.blocks.get(parent as usize))
                    && parent.kind == BlockKind::Class
                {
                    index.constructed_classes.insert((file_index, parent.local_id));
                }
            }
        }
        (index, duplicates)
    }
}

/// One check's view of the graph: the shared lookups, and the violations it found.
struct Checker<'g> {
    graph: &'g Graph,
    sources: Option<&'g dyn Sources>,
    /// Every block key in the graph.
    blocks: &'g FxHashMap<&'g str, BlockAt>,
    /// File paths to their index in `graph.files`.
    files: &'g FxHashMap<&'g str, usize>,
    /// Input paths to their index in `graph.inputs`.
    inputs: &'g FxHashMap<&'g str, usize>,
    /// Classes that declare a direct constructor.
    constructed_classes: &'g FxHashSet<(usize, u32)>,
    violations: Vec<Violation>,
}

impl<'g> Checker<'g> {
    fn new(graph: &'g Graph, sources: Option<&'g dyn Sources>, index: &'g Index<'g>) -> Self {
        Self {
            graph,
            sources,
            blocks: &index.blocks,
            files: &index.files,
            inputs: &index.inputs,
            constructed_classes: &index.constructed_classes,
            violations: Vec::new(),
        }
    }

    fn report(&mut self, subject: Subject, message: impl Into<String>) {
        self.violations.push(Violation {
            subject,
            message: message.into(),
        });
    }

    /// The snapshot of `path`, if sources were supplied and include it.
    fn snapshot(&self, path: &RepoPath) -> Option<&'g Snapshot> {
        self.sources.and_then(|sources| sources.snapshot(path))
    }

    /// The block a key names, with its file.
    fn resolve(&self, key: &Key) -> Option<(&'g FileGraph, &'g crate::model::Block)> {
        let at = *self.blocks.get(key.as_str())?;
        let file = &self.graph.files[at.file];
        Some((file, &file.blocks[at.local_id as usize]))
    }

    fn check_graph_order(&mut self) {
        let graph = self.graph;
        for pair in graph.files.windows(2) {
            if pair[0].file.path >= pair[1].file.path {
                self.report(
                    Subject::File(pair[1].file.path.clone()),
                    "files are not in strictly increasing path order",
                );
            }
        }
    }

    fn check_inputs(&mut self) {
        let graph = self.graph;
        for (index, input) in graph.inputs.iter().enumerate() {
            let subject = || Subject::Input(input.path.clone());
            if index > 0 && graph.inputs[index - 1].path >= input.path {
                self.report(subject(), "inputs are not in strictly increasing path order");
            }
            if self.files.contains_key(input.path.as_str()) {
                self.report(subject(), "path is both an indexed file and an input");
            }
            if self.sources.is_some() {
                match self.snapshot(&input.path) {
                    None => self.report(subject(), "no snapshot was supplied"),
                    Some(snapshot) if snapshot.hash() != input.hash => {
                        self.report(
                            subject(),
                            format!(
                                "fingerprint {} does not match the snapshot's {}",
                                input.hash,
                                snapshot.hash()
                            ),
                        );
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
