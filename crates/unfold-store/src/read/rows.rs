//! The small row types reader queries return where no core type fits.

use std::fmt;

use unfold_core::vocab::{FileState, Language, NodeKind};
use unfold_core::{ContentHash, File, Key, Link, Node, RepoPath};

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
    pub fn record(&self) -> File {
        File {
            path: self.path.clone(),
            language: self.language,
            state: self.state,
            hash: self.facts_hash,
            failure: self.failure.clone(),
        }
    }
}

/// A node with the file it belongs to and its local ID there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedNode {
    /// The file's path.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The node's local ID: its position in the file's preorder.
    pub id: u32,
    /// The node.
    pub node: Node,
}

/// A link that reaches some project key, seen from that key: a caller, or a use of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingLink {
    /// The link's file.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The key of the node the link starts at.
    pub from: Key,
    /// The link as stored; its `from` is the local ID of that node in `path`.
    pub link: Link,
}

impl IncomingLink {
    /// Where the links after this one start: its place in path order and then source order.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            path: self.path.as_str().into(),
            at: [i64::from(self.link.from), i64::from(self.link.seq)],
        }
    }
}

/// An entry of the definition catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionRow {
    /// The node's key.
    pub key: Key,
    /// The node's file.
    pub path: RepoPath,
    /// The node's local ID in its file.
    pub node: u32,
    /// The node's kind.
    pub kind: NodeKind,
    /// The qualified name.
    pub qualified_name: Box<str>,
    /// The display label.
    pub label: Box<str>,
}

/// One search candidate, in the index's relevance order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    /// What the candidate is.
    pub kind: CandidateKind,
    /// The definition's, app's, or entrance's key, or the file's module key.
    pub key: Key,
    /// The indexed name (see [`crate::search`]).
    pub name: Box<str>,
    /// The indexed qualified name.
    pub qualified_name: Box<str>,
    /// The file path; empty for apps and entrances.
    pub path: Box<str>,
    /// The node kind, `app`, `entrance`, or `module`.
    pub item_kind: Box<str>,
}

/// A position in a paginated result. Opaque: pass back the `next` of the previous page. Its
/// text (`Display`, read back by [`Cursor::parse`]) carries it across IPC.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor {
    /// The path of the last item returned.
    pub(crate) path: Box<str>,
    /// Its position within that file: a local ID, and a link's sequence number (0 otherwise).
    pub(crate) at: [i64; 2],
}

impl Cursor {
    /// The cursor written as `text`, or `None` when `text` isn't one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, ':');
        let at = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
        Some(Self {
            path: parts.next()?.into(),
            at,
        })
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.at[0], self.at[1], self.path)
    }
}

/// One page of a paginated result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The items, in path order and then source order within a file.
    pub items: Vec<T>,
    /// Where the next page starts, or `None` after the last page.
    pub next: Option<Cursor>,
}

/// Row counts and the database size, for tests ([`crate::Reader::stats`]).
#[cfg(any(test, feature = "testkit"))]
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
    /// Nodes.
    pub nodes: u64,
    /// Links to a project definition or module.
    pub links_project: u64,
    /// Links to a library.
    pub links_library: u64,
    /// Links to the language's builtins.
    pub links_builtin: u64,
    /// Links that reach nothing proven.
    pub links_unknown: u64,
    /// Typed records (signatures, parameters, members, and arguments).
    pub typed_records: u64,
    /// Apps.
    pub apps: u64,
    /// Entrances.
    pub entrances: u64,
    /// Definitions in the catalog.
    pub definitions: u64,
    /// Interned keys: the keys some row refers to.
    pub keys: u64,
    /// Cached facts entries.
    pub facts_entries: u64,
    /// The database file's size in bytes (pages in use, excluding the write-ahead log).
    pub database_bytes: u64,
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
