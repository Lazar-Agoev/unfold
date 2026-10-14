//! The whole program as the backend sees it: every file's cached facts in path order, and the
//! compact IDs that language evidence and the shared policy use to talk about it.
//!
//! The backend never holds a syntax tree. Everything it asks a language is phrased with these
//! IDs, which stay meaningful across relinks because they index cached facts:
//!
//! | ID | Names | Assigned by |
//! | --- | --- | --- |
//! | [`FileId`] | A file's position in the [`FactsSet`] (path order) | The engine, per link |
//! | [`NodeRef`] | A node: file plus local ID | The front end's lowering (preorder) |
//! | [`CallRef`] | A call node's facts, by its index in the file's call table | The front end; the table is the call nodes in preorder |
//! | [`ValueRef`] | An expression recorded in a file's value table | The front end, during extraction |
//! | [`ImportRef`] | A written module dependency in a file's import table | The front end, during extraction |
//! | [`TypeRef`] | A written type name in a file's type-name table | The front end, during extraction |
//! | [`DeclRef`] | A declaration the language knows about | The language; opaque to the backend |

use std::sync::Arc;

use unfold_core::vocab::Language;
use unfold_core::{Node, RepoPath};

use crate::facts::{CallFact, FileFacts, ImportFact, TypeRefFact, ValueFact};

/// A file's index in its [`FactsSet`]. File IDs follow path order, so they are reassigned
/// whenever files are added or removed; never persist them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

impl FileId {
    /// The ID as an index.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A node of the program: its file and its local (preorder) ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeRef {
    /// The node's file.
    pub file: FileId,
    /// The node's local ID in that file; the module is 0.
    pub local: u32,
}

/// An expression in a file's value table ([`GraphFacts::values`](crate::facts::GraphFacts::values)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueRef {
    /// The value's file.
    pub file: FileId,
    /// Its index in the file's value table.
    pub value: u32,
}

/// A call node's facts ([`GraphFacts::calls`](crate::facts::GraphFacts::calls)): the `n`-th
/// entry of the call table describes the `n`-th call node in preorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallRef {
    /// The call's file.
    pub file: FileId,
    /// Its index in the file's call table.
    pub call: u32,
}

/// A written module dependency in a file's import table
/// ([`GraphFacts::imports`](crate::facts::GraphFacts::imports)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportRef {
    /// The import's file.
    pub file: FileId,
    /// Its index in the file's import table.
    pub import: u32,
}

/// A written type name in a file's type-name table
/// ([`GraphFacts::type_refs`](crate::facts::GraphFacts::type_refs)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeRef {
    /// The name's file.
    pub file: FileId,
    /// Its index in the file's type-name table.
    pub index: u32,
}

/// A declaration, as its language identifies it.
///
/// The backend only compares, hashes, and passes declaration IDs back to the language that
/// produced them; what `index` counts (declarations, symbols, parameters, signatures) is the
/// language's choice. Languages conventionally put the declaring file in `file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclRef {
    /// The declaring file.
    pub file: FileId,
    /// A language-defined index.
    pub index: u32,
}

/// One file of the program.
#[derive(Debug, Clone)]
pub struct ProgramFile {
    /// The folder-relative path.
    pub path: RepoPath,
    /// The language that owns the file.
    pub language: Language,
    /// The facts the graph uses: from the current bytes, or from the last version that parsed.
    /// `None` for a file no version of which has parsed: it has no nodes, and links into it are
    /// `not_indexed`.
    pub facts: Option<Arc<FileFacts>>,
}

/// Why files cannot form a program.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProgramError {
    /// Two files share a path.
    #[error("{0} appears twice in the program")]
    DuplicatePath(RepoPath),
    /// More files than file IDs can number.
    #[error("the program has more than {} files", u32::MAX)]
    TooManyFiles,
}

/// Every file's facts, in path order. This is all the shared backend and each language's linker
/// read; nothing here holds a syntax tree.
#[derive(Debug, Clone, Default)]
pub struct FactsSet {
    files: Vec<ProgramFile>,
}

impl FactsSet {
    /// Builds a program from files in any order. Files are sorted by path, so the program (and
    /// every file ID) is independent of the order they were supplied in.
    pub fn new(files: impl IntoIterator<Item = ProgramFile>) -> Result<Self, ProgramError> {
        let mut files: Vec<ProgramFile> = files.into_iter().collect();
        if u32::try_from(files.len()).is_err() {
            return Err(ProgramError::TooManyFiles);
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(pair) = files.windows(2).find(|pair| pair[0].path == pair[1].path) {
            return Err(ProgramError::DuplicatePath(pair[1].path.clone()));
        }
        Ok(Self { files })
    }

    /// The files in path order; a file's index is its [`FileId`].
    pub fn files(&self) -> &[ProgramFile] {
        &self.files
    }

    /// Every file ID with its file, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (FileId, &ProgramFile)> {
        // `new` guarantees the count fits in u32.
        self.files
            .iter()
            .enumerate()
            .map(|(index, file)| (FileId(index as u32), file))
    }

    /// The file with this ID.
    pub fn file(&self, id: FileId) -> Option<&ProgramFile> {
        self.files.get(id.index())
    }

    /// The facts of a file that has them.
    pub fn facts(&self, id: FileId) -> Option<&FileFacts> {
        self.file(id)?.facts.as_deref()
    }

    /// The ID of the file at `path`.
    pub fn find(&self, path: &str) -> Option<FileId> {
        self.files
            .binary_search_by(|file| file.path.as_str().cmp(path))
            .ok()
            .map(|index| FileId(index as u32))
    }

    /// The node a reference names, if it exists.
    pub fn node(&self, node: NodeRef) -> Option<&Node> {
        self.facts(node.file)?.graph.nodes().get(node.local as usize)
    }

    /// The value a reference names, if it exists.
    pub fn value(&self, value: ValueRef) -> Option<&ValueFact> {
        self.facts(value.file)?.graph.values().get(value.value as usize)
    }

    /// The call a reference names, if it exists.
    pub fn call(&self, call: CallRef) -> Option<&CallFact> {
        self.facts(call.file)?.graph.calls().get(call.call as usize)
    }

    /// The import a reference names, if it exists.
    pub fn import(&self, import: ImportRef) -> Option<&ImportFact> {
        self.facts(import.file)?
            .graph
            .imports()
            .get(import.import as usize)
    }

    /// The type name a reference names, if it exists.
    pub fn type_ref(&self, reference: TypeRef) -> Option<&TypeRefFact> {
        self.facts(reference.file)?
            .graph
            .type_refs()
            .get(reference.index as usize)
    }

    /// The imports of a file, in the order its front end recorded them.
    pub fn imports(&self, file: FileId) -> &[ImportFact] {
        self.facts(file).map_or(&[], |facts| facts.graph.imports())
    }

    /// The language of the file with this ID.
    pub fn language(&self, file: FileId) -> Option<Language> {
        self.file(file).map(|file| file.language)
    }
}
