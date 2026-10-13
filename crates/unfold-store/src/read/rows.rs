//! The small row types reader queries return where no core type fits.

use unfold_core::vocab::{BlockKind, EdgeKind, FileState, Heritage, Language};
use unfold_core::{Block, ByteRange, ContentHash, FileRecord, Key, RepoPath};

use crate::search::CandidateKind;
use crate::update::{DiskStamp, GraphMeta};

/// A file's row: its record and what the last check saw on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    /// The folder-relative path.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// Whether its facts come from the bytes on disk.
    pub state: FileState,
    /// The SHA-256 of the bytes the facts came from.
    pub facts_hash: Option<ContentHash>,
    /// The syntax or read error, for a `last_good` or `failed` file.
    pub failure: Option<Box<str>>,
    /// The file on disk at the last check.
    pub disk: DiskStamp,
}

impl FileRow {
    /// The graph-model record.
    pub fn record(&self) -> FileRecord {
        FileRecord {
            path: self.path.clone(),
            language: self.language,
            state: self.state,
            hash: self.facts_hash,
            failure: self.failure.clone(),
        }
    }
}

/// A block with the file it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedBlock {
    /// The file's path.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The block.
    pub block: Block,
}

/// A type link resolved to some declaration, seen from the declaration: where it is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeUse {
    /// The link's file.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The block the link is sourced at (the innermost block around the written name).
    pub block: Block,
    /// The written name's byte range.
    pub range: ByteRange,
    /// The name as written.
    pub text: Box<str>,
    /// When the name is a base its block extends or implements.
    pub heritage: Option<Heritage>,
}

/// A resolved edge into some target, seen from the target: who calls, constructs, imports,
/// or passes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingEdge {
    /// The source occurrence's file.
    pub source_path: RepoPath,
    /// The source occurrence's key.
    pub source: Key,
    /// The source occurrence's local ID in its file.
    pub source_local_id: u32,
    /// The edge's position among the source's edges.
    pub seq: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// The written callee, argument, or specifier.
    pub text: Box<str>,
    /// The argument position of a reference.
    pub position: Option<u32>,
}

/// A stored definition (docs/reader.md#derived-views).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionRow {
    /// The block's key.
    pub key: Key,
    /// The block's file.
    pub path: RepoPath,
    /// The block's local ID in its file.
    pub local_id: u32,
    /// The block's kind.
    pub kind: BlockKind,
    /// The qualified name.
    pub qualified_name: Box<str>,
    /// The display label.
    pub label: Box<str>,
}

/// One search candidate. Ranking is the caller's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    /// What the candidate is.
    pub kind: CandidateKind,
    /// The definition's or entrance's key, or the file's root key.
    pub key: Key,
    /// The indexed name (see [`crate::search`]).
    pub name: Box<str>,
    /// The indexed qualified name.
    pub qualified_name: Box<str>,
    /// The file or evidence path.
    pub path: Box<str>,
    /// The block kind, entrance kind, or `file`.
    pub item_kind: Box<str>,
}

/// A position in a paginated result. Opaque: pass back the `next` of the previous page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor(pub(crate) [i64; 3]);

/// One page of a paginated result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The items, in a stable order.
    pub items: Vec<T>,
    /// Where the next page starts, or `None` after the last page.
    pub next: Option<Cursor>,
}

/// Row counts and the database size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexStats {
    /// Files whose facts come from the bytes on disk.
    pub files_current: u64,
    /// Files showing their last version that parsed.
    pub files_last_good: u64,
    /// Files that never parsed.
    pub files_failed: u64,
    /// Fingerprinted inputs.
    pub inputs: u64,
    /// Blocks.
    pub blocks: u64,
    /// Slots.
    pub slots: u64,
    /// Resolved edges.
    pub edges_resolved: u64,
    /// External edges.
    pub edges_external: u64,
    /// Unresolved edges.
    pub edges_unresolved: u64,
    /// Entrances.
    pub entrances: u64,
    /// Definitions.
    pub definitions: u64,
    /// Applications.
    pub applications: u64,
    /// Interned keys (never deleted, so at least the live count).
    pub keys: u64,
    /// Cached facts entries.
    pub facts_entries: u64,
    /// The database file's size in bytes (pages in use, excluding the write-ahead log).
    pub database_bytes: u64,
}

/// The import edges of one language's files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    /// Imports that resolve to an indexed file.
    pub resolved: u64,
    /// Every import.
    pub total: u64,
    /// The module labels of external imports with how many imports name each, most named
    /// first (ties by label).
    pub external: Vec<(Box<str>, u64)>,
}

/// The `meta` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMeta {
    /// The schema version the database was created with.
    pub schema_version: String,
    /// The engine version the database belongs to.
    pub engine_version: String,
    /// The last committed graph metadata, or `None` before the first commit that set it.
    pub graph: Option<GraphMeta>,
}
