//! The graph validator.
//!
//! Every compile and update validates the new graph before committing it. Validation checks the
//! graph's own invariants:
//!
//! - each file's nodes: preorder, the containment grammar, the parts of each control block,
//!   forms per language, flags per form, and which fields each kind carries (`nodes`);
//! - ranges contained in their parents (`nodes`);
//! - keys: unique in the graph, and each one re-derived from the tree (`nodes`);
//! - links: order, sources, outcomes, and targets per link kind (`links`);
//! - typed records: which nodes have them, and their indexes (`records`);
//! - apps and entrances: their keys, re-derived from their names and unique (`apps`).
//!
//! When the caller supplies them, it also checks files and inputs against their source
//! snapshots ([`Sources`]: hashes, lines, character boundaries, and surrounding trivia), and each
//! language's files against that language's independent [`Census`].
//!
//! It never checks routes, so legal code never fails a compile, and an entrance target's state is
//! data, not an error: a `missing` target is valid. It never panics and reports every violation
//! it finds, each naming the record it concerns.

mod apps;
mod links;
mod nodes;
mod records;

use std::collections::BTreeMap;
use std::collections::hash_map::Entry;
use std::fmt;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use crate::key::Key;
use crate::model::{FileGraph, Graph, Node};
use crate::path::RepoPath;
use crate::text::{ByteRange, LABEL_MAX_UTF16, Snapshot, utf16_len};
use crate::vocab::{Form, Language};

/// The record a [`Violation`] concerns.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subject {
    /// A file record, or a record of the file that names no node.
    File(RepoPath),
    /// A configuration input.
    Input(RepoPath),
    /// A node.
    Node(Key),
    /// A link.
    Link {
        /// The node it starts at.
        from: Key,
        /// Its sequence number.
        seq: u32,
    },
    /// The signature of a function.
    Signature(Key),
    /// A parameter of a function.
    Parameter {
        /// The function.
        node: Key,
        /// The parameter's index.
        index: u32,
    },
    /// A member of a type or class.
    Member {
        /// The type or class.
        node: Key,
        /// The member's index.
        index: u32,
    },
    /// An argument of a call.
    Argument {
        /// The call.
        call: Key,
        /// The argument's index.
        index: u32,
    },
    /// An app.
    App(Key),
    /// An entrance.
    Entrance(Key),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(f, "file {path}"),
            Self::Input(path) => write!(f, "input {path}"),
            Self::Node(key) => write!(f, "node {key}"),
            Self::Link { from, seq } => write!(f, "link {seq} of {from}"),
            Self::Signature(node) => write!(f, "signature of {node}"),
            Self::Parameter { node, index } => write!(f, "parameter {index} of {node}"),
            Self::Member { node, index } => write!(f, "member {index} of {node}"),
            Self::Argument { call, index } => write!(f, "argument {index} of {call}"),
            Self::App(key) => write!(f, "app {key}"),
            Self::Entrance(key) => write!(f, "entrance {key}"),
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

/// An independent view of one language's syntax, computed by its front end from the same bytes
/// as the facts, so validation does not just trust the lowering. Files are validated in
/// parallel, so a census is shared across threads.
pub trait Census: Sync {
    /// The language this census covers.
    fn language(&self) -> Language;

    /// Every call in `path`'s parse tree (calls, constructions, decorators, and tagged
    /// templates), or `None` if the census doesn't cover `path`. Each must be exactly one call
    /// node, and each call node exactly one of them.
    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]>;
}

/// One call found by a [`Census`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// The call's token-exact range.
    pub range: ByteRange,
    /// The form its node must have, `call` or `new`, when syntax alone decides it (TypeScript's
    /// `new X()`). Absent when only resolution can tell, as for a Python construction.
    pub form: Option<Form>,
}

/// Looks up the snapshot of a source file or configuration input by path (shared across the
/// threads that validate files).
pub trait Sources: Sync {
    /// The snapshot at `path`, if supplied.
    fn snapshot(&self, path: &RepoPath) -> Option<&Snapshot>;
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
        let censuses: Vec<Language> = self.censuses.iter().map(|census| census.language()).collect();
        f.debug_struct("Validator")
            .field("sources", &self.sources.is_some())
            .field("censuses", &censuses)
            .finish()
    }
}

impl<'a> Validator<'a> {
    /// A validator that checks only the graph's own invariants.
    pub fn new() -> Self {
        Self::default()
    }

    /// Also checks every current file and every input against its snapshot: hashes, the
    /// module's length, lines, character boundaries, and surrounding trivia. Current files and
    /// inputs without a snapshot are reported. Last-good facts came from earlier bytes, so
    /// their files are not checked against snapshots.
    pub fn with_sources(mut self, sources: &'a dyn Sources) -> Self {
        self.sources = Some(sources);
        self
    }

    /// Also checks the files of `census.language()` against that census.
    pub fn with_census(mut self, census: &'a dyn Census) -> Self {
        self.censuses.push(census);
        self
    }

    /// Checks `graph` and returns every violation, in a deterministic order: graph-wide order
    /// and duplicate keys first, then each file in path order, then inputs, then apps.
    ///
    /// Files are checked independently, in parallel on the current rayon pool, and their
    /// violations reassembled in file order.
    pub fn validate(&self, graph: &Graph) -> Vec<Violation> {
        let index = Index::new(graph);
        let context = Context {
            graph,
            index: &index,
            sources: self.sources,
        };
        let mut checker = Checker::new(&context);
        checker.check_file_order();
        checker.check_duplicate_keys();
        let per_file: Vec<Vec<Violation>> = graph
            .files
            .par_iter()
            .map(|file| {
                let mut checker = Checker::new(&context);
                let census = self
                    .censuses
                    .iter()
                    .find(|census| census.language() == file.file.language)
                    .copied();
                nodes::check(&mut checker, file, census);
                checker.violations
            })
            .collect();
        checker.violations.extend(per_file.into_iter().flatten());
        checker.check_inputs();
        apps::check(&mut checker);
        checker.violations
    }
}

/// Where a node lives: its file's index in the graph and its local ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeAt {
    file: usize,
    id: u32,
}

/// The graph-wide lookups every check reads, built once and shared by the file checks.
struct Index<'g> {
    /// Every node key in the graph, at its first occurrence.
    nodes: FxHashMap<&'g str, NodeAt>,
    /// Every node key that occurs more than once, at its later occurrences, in graph order.
    duplicates: Vec<&'g Key>,
    /// File paths to their index in `graph.files`.
    files: FxHashMap<&'g str, usize>,
}

impl<'g> Index<'g> {
    fn new(graph: &'g Graph) -> Self {
        let count: usize = graph.files.iter().map(|file| file.nodes.len()).sum();
        let mut nodes = FxHashMap::with_capacity_and_hasher(count, Default::default());
        let mut duplicates = Vec::new();
        let mut files = FxHashMap::with_capacity_and_hasher(graph.files.len(), Default::default());
        for (file_index, file) in graph.files.iter().enumerate() {
            files.insert(file.file.path.as_str(), file_index);
            for (id, node) in file.nodes.iter().enumerate() {
                let at = NodeAt {
                    file: file_index,
                    id: u32::try_from(id).unwrap_or(u32::MAX),
                };
                match nodes.entry(node.key.as_str()) {
                    Entry::Occupied(_) => duplicates.push(&node.key),
                    Entry::Vacant(entry) => {
                        entry.insert(at);
                    }
                }
            }
        }
        Self {
            nodes,
            duplicates,
            files,
        }
    }
}

/// What every check shares: the graph, its lookups, and the snapshots.
struct Context<'g> {
    graph: &'g Graph,
    index: &'g Index<'g>,
    sources: Option<&'g dyn Sources>,
}

/// One check's view of the graph, and the violations it found.
struct Checker<'g> {
    context: &'g Context<'g>,
    violations: Vec<Violation>,
}

impl<'g> Checker<'g> {
    fn new(context: &'g Context<'g>) -> Self {
        Self {
            context,
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
        self.context.sources.and_then(|sources| sources.snapshot(path))
    }

    /// The node a key names, with its file.
    fn resolve(&self, key: &Key) -> Option<(&'g FileGraph, &'g Node, u32)> {
        let at = *self.context.index.nodes.get(key.as_str())?;
        let file = self.context.graph.files.get(at.file)?;
        Some((file, file.nodes.get(at.id as usize)?, at.id))
    }

    /// Reports `text` when it is empty or longer than a label may be.
    fn check_label(&mut self, subject: &dyn Fn() -> Subject, field: &str, text: &str) {
        if text.is_empty() {
            self.report(
                subject(),
                format!("{field} is empty; an absent {field} is omitted"),
            );
        } else if utf16_len(text) > LABEL_MAX_UTF16 {
            self.report(
                subject(),
                format!("{field} is longer than {LABEL_MAX_UTF16} UTF-16 units"),
            );
        }
    }

    fn check_file_order(&mut self) {
        let graph = self.context.graph;
        for pair in graph.files.windows(2) {
            if pair[0].file.path >= pair[1].file.path {
                self.report(
                    Subject::File(pair[1].file.path.clone()),
                    "files are not in strictly increasing path order",
                );
            }
        }
    }

    fn check_duplicate_keys(&mut self) {
        let index = self.context.index;
        for key in &index.duplicates {
            self.report(Subject::Node((*key).clone()), "another node has this key");
        }
    }

    fn check_inputs(&mut self) {
        let graph = self.context.graph;
        for (position, input) in graph.inputs.iter().enumerate() {
            let subject = || Subject::Input(input.path.clone());
            if position > 0 && graph.inputs[position - 1].path >= input.path {
                self.report(subject(), "inputs are not in strictly increasing path order");
            }
            if self.context.index.files.contains_key(input.path.as_str()) {
                self.report(subject(), "the path is both an indexed file and an input");
            }
            if self.context.sources.is_none() {
                continue;
            }
            match self.snapshot(&input.path) {
                None => self.report(subject(), "no snapshot was supplied"),
                Some(snapshot) if snapshot.hash() != input.hash => self.report(
                    subject(),
                    format!(
                        "hash {} does not match the snapshot's {}",
                        input.hash,
                        snapshot.hash()
                    ),
                ),
                Some(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
