//! Where contracts read stored facts: a [`FactSource`] (the store's `Reader`, or an in-memory
//! `Graph`) and a per-request [`Facts`] cache that reads each file once and verifies its source
//! at most once.

use std::path::Path;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use unfold_core::vocab::LinkKind;
use unfold_core::{FileGraph, Graph, LineBreaks, LinkTarget, RepoPath};
use unfold_store::Reader;

use crate::error::Result;
use crate::source::{Source, verify_file};
use crate::tree::FileTree;

/// The stored facts contracts read.
pub trait FactSource {
    /// The file and local ID of the node with `key`.
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>>;
    /// One file's nodes, links, and typed records.
    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>>;
    /// How many `types` and `extends` links reach the class or type with `key`.
    fn type_use_count(&self, key: &str) -> Result<u64>;
}

impl FactSource for Reader {
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>> {
        Ok(self.node(key)?.map(|located| (located.path, located.id)))
    }

    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>> {
        Ok(Reader::file_graph(self, path)?)
    }

    fn type_use_count(&self, key: &str) -> Result<u64> {
        Ok(Reader::type_use_count(self, key)?)
    }
}

impl FactSource for Graph {
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>> {
        Ok(self
            .files
            .iter()
            .find_map(|file| file.local_id(key).map(|local| (file.file.path.clone(), local))))
    }

    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>> {
        Ok(self.file(path).cloned())
    }

    fn type_use_count(&self, key: &str) -> Result<u64> {
        let uses = self
            .files
            .iter()
            .flat_map(|file| &file.links)
            .filter(|link| matches!(link.kind, LinkKind::Types | LinkKind::Extends))
            .filter(|link| matches!(&link.to, LinkTarget::Project(target) if target.as_str() == key))
            .count();
        Ok(u64::try_from(uses).unwrap_or(u64::MAX))
    }
}

/// The facts one request reads, each file read once and its source verified at most once.
pub struct Facts<'a> {
    source: &'a dyn FactSource,
    root: &'a Path,
    files: FxHashMap<RepoPath, Option<Rc<FileTree>>>,
    texts: FxHashMap<RepoPath, Option<Rc<Source>>>,
}

impl<'a> Facts<'a> {
    /// Facts from `source`, with source files read and verified under `root`.
    pub fn new(source: &'a dyn FactSource, root: &'a Path) -> Self {
        Self {
            source,
            root,
            files: FxHashMap::default(),
            texts: FxHashMap::default(),
        }
    }

    /// The stored facts.
    pub(crate) fn source(&self) -> &'a dyn FactSource {
        self.source
    }

    /// The tree of the file at `path`.
    pub(crate) fn file(&mut self, path: &RepoPath) -> Result<Option<Rc<FileTree>>> {
        if let Some(found) = self.files.get(path) {
            return Ok(found.clone());
        }
        let found = self
            .source
            .file_graph(path.as_str())?
            .map(|graph| Rc::new(FileTree::new(graph)));
        self.files.insert(path.clone(), found.clone());
        Ok(found)
    }

    /// The tree of the file holding the node with `key`, and the node's ID. `None` when no such
    /// node exists (or the store and its file disagree, which reads as missing).
    pub(crate) fn locate(&mut self, key: &str) -> Result<Option<(Rc<FileTree>, u32)>> {
        let Some((path, id)) = self.source.locate(key)? else {
            return Ok(None);
        };
        let Some(file) = self.file(&path)? else {
            return Ok(None);
        };
        let matches = file.id(key) == Some(id);
        Ok(matches.then_some((file, id)))
    }

    /// The verified source of `file`, or `None` when its bytes changed or are missing.
    pub(crate) fn text(&mut self, file: &FileTree) -> Option<Rc<Source>> {
        let path = file.path();
        if let Some(found) = self.texts.get(path) {
            return found.clone();
        }
        let found = verify_file(self.root, path, file.file().hash, LineBreaks::of(file.language()))
            .into_source()
            .map(Rc::new);
        self.texts.insert(path.clone(), found.clone());
        found
    }
}
