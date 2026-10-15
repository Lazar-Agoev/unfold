//! The delta one commit applies: [`Update`].
//!
//! # How the engine produces an update
//!
//! After each `update(changes)` the engine holds the previous and the new graph in memory. It
//! compares them and describes only what differs, borrowing records from the new graph so that
//! nothing is copied:
//!
//! | Field | Contents | Typical size |
//! | --- | --- | --- |
//! | [`Update::files`] | One [`FileChange`] per file whose row, nodes, links, typed records, or definitions differ | The edited files, plus files whose links changed through linking |
//! | [`Update::removed_files`] | Paths no longer indexed (deleted or excluded) | Usually empty |
//! | [`Update::inputs`] | The complete new input set, when any input changed | Usually `None` |
//! | [`Update::apps`] | The complete new apps of `unfold.toml`, with their entrances and target states, when any changed | Usually `None` |
//! | [`Update::meta`] | The graph metadata (producers, settings, time, completeness) | Every commit |
//!
//! A first compile is an update in which every file is new: [`FileChange::full`] for each
//! file. [`FileChange::between`] computes the parts of a change from the previous and new file
//! graphs, so the engine never has to track which part changed itself.
//!
//! # What the store does with each part
//!
//! - A [`FileChange`] always rewrites the file's row (record and disk stamp). Each optional
//!   part replaces that part of the file wholesale: `nodes` its nodes, `links` its links,
//!   `types` its typed records (signatures, parameters, members, and arguments), and
//!   `definitions` its catalog entries and their search rows. A part left `None` is not
//!   touched, so an edit that changes only links in a file costs one row update and that
//!   file's links.
//! - Links in other files that reach a replaced file's nodes are untouched: they hold the
//!   target's interned key, not a row of the target's file.
//! - Local IDs renumber when a file's nodes change. If any definition's local ID moved, the
//!   change must also carry `definitions`; [`FileChange::between`] cannot know that, so the
//!   caller compares its definition catalog per file.
//! - Removing a file deletes its row, nodes, links, typed records, definitions, search rows,
//!   and cached facts.
//! - Apps are a small set read from `unfold.toml` and resolved after each compile; they are
//!   replaced whole when anything in them changes, entrances and target states included.
//! - Keys are interned as they are written. A key keeps its ID while some row refers to it;
//!   the commit that deletes its last use deletes it too.
//!
//! [`crate::Writer::replace`] treats an update as the whole graph instead: stored files it
//! does not name are removed, and absent inputs or apps mean empty sets.
//!
//! The whole update is applied in one transaction: readers see the previous graph or the new
//! one, never a mixture. The store trusts the update: the engine validates the graph before it
//! commits.

use unfold_core::vocab::NodeKind;
use unfold_core::{App, ContentHash, File, FileGraph, FileTypes, Input, Key, Link, Node, RepoPath};

/// One commit's worth of changes. See the [module documentation](self).
#[derive(Debug, Clone, Default)]
pub struct Update<'a> {
    /// Files whose row or contents changed, including new files.
    pub files: Vec<FileChange<'a>>,
    /// Files no longer indexed. Applied before [`Update::files`].
    pub removed_files: Vec<RepoPath>,
    /// The complete new set of inputs, or `None` when the inputs did not change.
    pub inputs: Option<&'a [Input]>,
    /// The complete new apps in file order, or `None` when they did not change.
    pub apps: Option<&'a [App]>,
    /// New graph metadata, or `None` to keep what is stored.
    pub meta: Option<GraphMeta>,
}

impl Update<'_> {
    /// Whether the update writes nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.removed_files.is_empty()
            && self.inputs.is_none()
            && self.apps.is_none()
            && self.meta.is_none()
    }
}

/// What the file looked like on disk at the last check: the cheap change test on reopen
/// compares size and modification time before hashing (docs/engine.md#project-lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskStamp {
    /// The SHA-256 of the bytes on disk; differs from the facts' hash for a `last_good` file,
    /// and is absent when the file could not be read.
    pub hash: Option<ContentHash>,
    /// The modification time, in nanoseconds since the Unix epoch.
    pub mtime_ns: Option<i64>,
    /// The size in bytes.
    pub size: Option<u64>,
}

/// The changes to one file. The row is always rewritten; each `Some` part replaces that part.
#[derive(Debug, Clone)]
pub struct FileChange<'a> {
    /// The file's record. Its `hash` is the hash of the bytes its facts came from.
    pub file: &'a File,
    /// The file's state on disk.
    pub disk: DiskStamp,
    /// New nodes in preorder, or `None` to keep the stored ones.
    pub nodes: Option<&'a [Node]>,
    /// New links, or `None` to keep the stored ones.
    pub links: Option<&'a [Link]>,
    /// New typed records, or `None` to keep the stored ones.
    pub types: Option<&'a FileTypes>,
    /// New definitions for this file, or `None` to keep the stored ones.
    pub definitions: Option<&'a [Definition]>,
}

impl<'a> FileChange<'a> {
    /// Every part of `file`: for new files and for a first compile. Definitions are left to the
    /// caller.
    pub fn full(file: &'a FileGraph, disk: DiskStamp) -> Self {
        Self {
            file: &file.file,
            disk,
            nodes: Some(&file.nodes),
            links: Some(&file.links),
            types: Some(&file.types),
            definitions: None,
        }
    }

    /// The parts of `current` that differ from `previous`; definitions are left to the caller.
    /// The row is always part of the change, so a file whose only difference is its disk stamp
    /// yields a change with no parts.
    pub fn between(previous: &FileGraph, current: &'a FileGraph, disk: DiskStamp) -> Self {
        Self {
            file: &current.file,
            disk,
            nodes: (previous.nodes != current.nodes).then_some(&current.nodes[..]),
            links: (previous.links != current.links).then_some(&current.links[..]),
            types: (previous.types != current.types).then_some(&current.types),
            definitions: None,
        }
    }

    /// Whether the change touches anything besides the file's row.
    pub fn has_parts(&self) -> bool {
        self.nodes.is_some() || self.links.is_some() || self.types.is_some() || self.definitions.is_some()
    }
}

/// An entry of the reader's definition catalog: a definition that can be a card, with its
/// qualified name and the label search shows. The reader computes these; the store keeps them
/// and indexes them for search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The node's structural key.
    pub key: Key,
    /// The node's local ID in its file.
    pub node: u32,
    /// The node's kind: `module`, `class`, `function`, or `type`.
    pub kind: NodeKind,
    /// The name with its enclosing definitions, such as `OrderService.place`.
    pub qualified_name: Box<str>,
    /// The display label: the node's name, or for an inline function a label taken from where
    /// it is written, such as "callback to `ordersRouter.post`".
    pub label: Box<str>,
}

/// Graph-wide metadata the engine records with each commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphMeta {
    /// The front ends that produced the graph, as JSON written by the engine.
    pub producers: Box<str>,
    /// The fingerprint of the effective settings.
    pub settings_hash: ContentHash,
    /// When the graph was compiled, in milliseconds since the Unix epoch.
    pub compiled_at_ms: i64,
    /// Whether every discovered file is indexed (no failed files, read errors, or invalid
    /// settings).
    pub complete: bool,
    /// What the graph was linked from and what linking found, as text written by the engine, so
    /// a reopened folder can keep the graph without linking it again; it also keeps what the
    /// engine remembers for the apps' fixes.
    pub link_basis: Box<str>,
}
