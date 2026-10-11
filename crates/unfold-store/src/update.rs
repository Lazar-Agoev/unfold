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
//! | [`Update::files`] | One [`FileChange`] per file whose row, structure, edges, or definitions differ | The edited files, plus files whose edges or definitions changed through linking |
//! | [`Update::removed_files`] | Paths no longer indexed (deleted or excluded) | Usually empty |
//! | [`Update::inputs`] | The complete new input set, when any input changed | Usually `None` |
//! | [`Update::entrances`] | Entrances added or changed (by key), and keys of entrances that disappeared | Usually empty |
//! | [`Update::applications`], [`Update::memberships`] | The complete new sets, when they changed | Usually `None` |
//! | [`Update::meta`] | The graph metadata (producers, settings, time, completeness) | Every commit |
//!
//! A first compile is an update in which every file is new: [`FileChange::full`] for each file
//! and every entrance in `upserts`. [`FileChange::between`] and [`EntranceChanges::between`]
//! compute the parts of a change from the previous and new records, so the engine never has to
//! track which part changed itself.
//!
//! # What the store does with each part
//!
//! - A [`FileChange`] always rewrites the file's row (record and disk stamp). Each optional
//!   part replaces that part of the file wholesale: `structure` its blocks and slots, `edges`
//!   its edges, `types` its typed records (signatures, parameters, members, arguments, results,
//!   and type links), and `definitions` its reader definitions and their search rows. A part left
//!   `None` is not touched, so an edit that changes only edges in a file costs one row update
//!   and that file's edges.
//! - Local IDs renumber when a file's structure changes. If any definition's `local_id` moved,
//!   the change must also carry `definitions`; [`FileChange::between`] cannot know that, so the
//!   engine compares its definition catalog per file.
//! - Removing a file deletes its row, blocks, slots, edges, typed records, definitions, search
//!   rows, and cached facts.
//!   Entrances declared in it are removed only through [`Update::entrances`], because entrances
//!   are cross-file facts the engine recomputes.
//! - Entrances are diffed by key rather than replaced as a set. An entrance's row ID is its
//!   key's ID, so IDs, targets, and memberships stay put for unchanged entrances, and a body edit
//!   that shifts one route's evidence rewrites that route only. Applications and memberships
//!   are tiny derived sets and are replaced whole when they change.
//! - Keys are interned as they are written and are never deleted, so a key keeps its ID for
//!   the life of the database.
//!
//! [`crate::Writer::replace`] treats an update as the whole graph instead: stored files and
//! entrances it does not name are removed too.
//!
//! The whole update is applied in one transaction: readers see the previous graph or the new
//! one, never a mixture. The store trusts the update: the engine validates the graph before it
//! commits.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{BlockKind, Language};
use unfold_core::{
    Block, ContentHash, Edge, Entrance, FileGraph, FileRecord, FileTypes, InputRecord, Key, RepoPath, Slot,
};

/// One commit's worth of changes. See the [module documentation](self).
#[derive(Debug, Clone, Default)]
pub struct Update<'a> {
    /// Files whose row or contents changed, including new files.
    pub files: Vec<FileChange<'a>>,
    /// Files no longer indexed. Applied before [`Update::files`].
    pub removed_files: Vec<RepoPath>,
    /// The complete new set of inputs, or `None` when the inputs did not change.
    pub inputs: Option<&'a [InputRecord]>,
    /// Entrances added, changed, or removed.
    pub entrances: EntranceChanges<'a>,
    /// The complete new set of applications, or `None` when unchanged.
    pub applications: Option<&'a [Application]>,
    /// The complete new set of memberships, or `None` when unchanged.
    pub memberships: Option<&'a [Membership]>,
    /// New graph metadata, or `None` to keep what is stored.
    pub meta: Option<GraphMeta>,
}

impl Update<'_> {
    /// Whether the update writes nothing.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.removed_files.is_empty()
            && self.inputs.is_none()
            && self.entrances.is_empty()
            && self.applications.is_none()
            && self.memberships.is_none()
            && self.meta.is_none()
    }
}

/// What the file looked like on disk at the last check: the cheap change test on reopen
/// compares size and modification time before hashing (architecture §7.8).
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

/// A file's blocks and slots, written together because slots refer to blocks by local ID.
#[derive(Debug, Clone, Copy)]
pub struct Structure<'a> {
    /// Blocks in preorder.
    pub blocks: &'a [Block],
    /// Slots ordered by block, then start.
    pub slots: &'a [Slot],
}

/// The changes to one file. The row is always rewritten; each `Some` part replaces that part.
#[derive(Debug, Clone)]
pub struct FileChange<'a> {
    /// The file's record. Its `hash` is the hash of the bytes its facts came from.
    pub record: &'a FileRecord,
    /// The file's state on disk.
    pub disk: DiskStamp,
    /// New blocks and slots, or `None` to keep the stored ones.
    pub structure: Option<Structure<'a>>,
    /// New edges, or `None` to keep the stored ones.
    pub edges: Option<&'a [Edge]>,
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
            record: &file.file,
            disk,
            structure: Some(Structure {
                blocks: &file.blocks,
                slots: &file.slots,
            }),
            edges: Some(&file.edges),
            types: Some(&file.types),
            definitions: None,
        }
    }

    /// The parts of `current` that differ from `previous`. Blocks and slots are compared
    /// together, edges and typed records separately; definitions are left to the caller. The row is always part
    /// of the change, so a file whose only difference is its disk stamp yields a change with
    /// no parts.
    pub fn between(previous: &FileGraph, current: &'a FileGraph, disk: DiskStamp) -> Self {
        let structure_changed = previous.blocks != current.blocks || previous.slots != current.slots;
        Self {
            record: &current.file,
            disk,
            structure: structure_changed.then_some(Structure {
                blocks: &current.blocks,
                slots: &current.slots,
            }),
            edges: (previous.edges != current.edges).then_some(&current.edges[..]),
            types: (previous.types != current.types).then_some(&current.types),
            definitions: None,
        }
    }

    /// Whether the change touches anything besides the file's row.
    pub fn has_parts(&self) -> bool {
        self.structure.is_some() || self.edges.is_some() || self.types.is_some() || self.definitions.is_some()
    }
}

/// Entrances to write and entrances to delete, by key.
#[derive(Debug, Clone, Default)]
pub struct EntranceChanges<'a> {
    /// Entrances that are new or whose fields or targets changed. Each replaces the stored
    /// entrance with the same key.
    pub upserts: Vec<&'a Entrance>,
    /// Keys of entrances that no longer exist. Their targets and memberships go with them.
    pub removals: Vec<Key>,
}

impl<'a> EntranceChanges<'a> {
    /// Every entrance in `entrances`, as for a first compile.
    pub fn all(entrances: &'a [Entrance]) -> Self {
        Self {
            upserts: entrances.iter().collect(),
            removals: Vec::new(),
        }
    }

    /// The changes that turn `previous` into `current`, matched by key. Removals and upserts
    /// keep the order of their input lists.
    pub fn between(previous: &[Entrance], current: &'a [Entrance]) -> Self {
        let before: FxHashMap<&str, &Entrance> = previous
            .iter()
            .map(|entrance| (entrance.key.as_str(), entrance))
            .collect();
        let after: FxHashSet<&str> = current.iter().map(|entrance| entrance.key.as_str()).collect();
        Self {
            upserts: current
                .iter()
                .filter(|entrance| before.get(entrance.key.as_str()) != Some(entrance))
                .collect(),
            removals: previous
                .iter()
                .filter(|entrance| !after.contains(entrance.key.as_str()))
                .map(|entrance| entrance.key.clone())
                .collect(),
        }
    }

    /// Whether nothing changes.
    pub fn is_empty(&self) -> bool {
        self.upserts.is_empty() && self.removals.is_empty()
    }
}

/// A reader definition (architecture §10.1): a block in the definition catalog, with its
/// qualified name and the label search shows. The reader computes these; the store keeps
/// them and indexes them for search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The block's structural key.
    pub key: Key,
    /// The block's local ID in its file.
    pub local_id: u32,
    /// The block's kind.
    pub kind: BlockKind,
    /// The name with its enclosing classes and callables, such as `OrderService.place`.
    pub qualified_name: Box<str>,
    /// The display label: the block's name, or for an unnamed callback a label taken from its
    /// callee.
    pub label: Box<str>,
}

/// An application (architecture §10.1): a distinct resolved startup among process entrances.
/// It is identified by its startup key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    /// The startup target's key: a file, or a Python console script's callable.
    pub startup: Key,
    /// The display name.
    pub name: Box<str>,
    /// The startup's language.
    pub language: Language,
}

/// A non-process entrance that belongs to an application.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Membership {
    /// The entrance's key.
    pub entrance: Key,
    /// The application's startup key.
    pub application: Key,
}

/// Graph-wide metadata the engine records with each commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphMeta {
    /// The front ends and detectors that produced the graph, as JSON written by the engine.
    pub producers: Box<str>,
    /// The fingerprint of the effective settings.
    pub settings_hash: ContentHash,
    /// When the graph was compiled, in milliseconds since the Unix epoch.
    pub compiled_at_ms: i64,
    /// Whether every discovered file is indexed (no failed files, read errors, or invalid
    /// settings).
    pub complete: bool,
    /// What the graph was linked from and what linking found, as text written by the engine, so
    /// a reopened folder can keep the graph without linking it again.
    pub link_basis: Box<str>,
}
