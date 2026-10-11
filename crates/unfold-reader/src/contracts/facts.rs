//! Where typed lines read stored facts: a [`FactSource`] (the store's `Reader`, or an in-memory
//! `Graph`) and a per-request [`Facts`] cache that reads each file once and verifies its source
//! at most once.

use std::path::Path;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{Heritage, Resolution, SlotName};
use unfold_core::{
    Argument, Block, ByteRange, CallResult, Edge, FileGraph, Graph, Member, Parameter, RepoPath, Signature,
    Slot, Snapshot, TypeLink, WrittenType,
};
use unfold_store::Reader;

use super::TypeText;
use super::text::type_text;
use crate::catalog;
use crate::error::Result;
use crate::index::{file_owners, file_subtree_ends, to_u32};
use crate::source::{Verified, verify_file};

/// A type link resolved to a declaration, seen from the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseSite {
    /// The link's file.
    pub path: RepoPath,
    /// The local ID of the block the link is sourced at.
    pub block: u32,
    /// The written name's byte range.
    pub range: ByteRange,
    /// When the name is a base its block extends or implements.
    pub heritage: Option<Heritage>,
}

/// The stored facts typed lines read.
pub trait FactSource {
    /// The file and local ID of the block with `key`.
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>>;
    /// One file's blocks, slots, edges, and typed records, each in canonical order.
    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>>;
    /// How many type links resolve to the declaration with `key`.
    fn type_use_count(&self, key: &str) -> Result<u64>;
    /// The type links that resolve to the declaration with `key`, by path, then block, then
    /// range, skipping `offset` and returning at most `limit`.
    fn type_uses(&self, key: &str, offset: usize, limit: usize) -> Result<Vec<UseSite>>;
}

impl FactSource for Reader {
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>> {
        Ok(self
            .block(key)?
            .map(|located| (located.path, located.block.local_id)))
    }

    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>> {
        Ok(Reader::file_graph(self, path)?)
    }

    fn type_use_count(&self, key: &str) -> Result<u64> {
        Ok(Reader::type_use_count(self, key)?)
    }

    fn type_uses(&self, key: &str, offset: usize, limit: usize) -> Result<Vec<UseSite>> {
        Ok(Reader::type_uses(self, key, offset, limit)?
            .into_iter()
            .map(|found| UseSite {
                path: found.path,
                block: found.block.local_id,
                range: found.range,
                heritage: found.heritage,
            })
            .collect())
    }
}

/// The type links of every file of `graph` that resolve to `key`, in graph order.
fn links_to<'a>(graph: &'a Graph, key: &'a str) -> impl Iterator<Item = (&'a FileGraph, &'a TypeLink)> + 'a {
    graph.files.iter().flat_map(move |file| {
        file.types
            .type_links
            .iter()
            .filter(move |link| {
                link.resolution == Resolution::Resolved
                    && link.target.as_ref().is_some_and(|target| target.as_str() == key)
            })
            .map(move |link| (file, link))
    })
}

impl FactSource for Graph {
    fn locate(&self, key: &str) -> Result<Option<(RepoPath, u32)>> {
        Ok(self.files.iter().find_map(|file| {
            file.blocks
                .iter()
                .find(|block| block.key.as_str() == key)
                .map(|block| (file.file.path.clone(), block.local_id))
        }))
    }

    fn file_graph(&self, path: &str) -> Result<Option<FileGraph>> {
        Ok(self.file(path).cloned())
    }

    fn type_use_count(&self, key: &str) -> Result<u64> {
        Ok(u64::try_from(links_to(self, key).count()).unwrap_or(u64::MAX))
    }

    fn type_uses(&self, key: &str, offset: usize, limit: usize) -> Result<Vec<UseSite>> {
        Ok(links_to(self, key)
            .skip(offset)
            .take(limit)
            .map(|(file, link)| UseSite {
                path: file.file.path.clone(),
                block: link.block,
                range: link.range,
                heritage: link.heritage,
            })
            .collect())
    }
}

/// The records of `items` (sorted by `key`) whose key is `value`.
fn run<T>(items: &[T], key: impl Fn(&T) -> u32, value: u32) -> &[T] {
    let start = items.partition_point(|item| key(item) < value);
    let end = start + items[start..].partition_point(|item| key(item) == value);
    &items[start..end]
}

/// One file's facts with the indexes typed lines walk.
#[derive(Debug)]
pub(crate) struct FileFacts {
    graph: FileGraph,
    owners: Vec<u32>,
    ends: Vec<u32>,
    /// Type link indices ordered by range start.
    links: Vec<u32>,
}

impl FileFacts {
    fn new(graph: FileGraph) -> Self {
        let owners = file_owners(&graph.blocks);
        let ends = file_subtree_ends(&graph.blocks);
        let mut links: Vec<u32> = (0..to_u32(graph.types.type_links.len())).collect();
        links.sort_by_key(|&index| graph.types.type_links[index as usize].range.start);
        Self {
            graph,
            owners,
            ends,
            links,
        }
    }

    /// The file graph.
    pub(crate) fn graph(&self) -> &FileGraph {
        &self.graph
    }

    /// The file's path.
    pub(crate) fn path(&self) -> &RepoPath {
        &self.graph.file.path
    }

    /// Block `local`.
    pub(crate) fn block(&self, local: u32) -> Option<&Block> {
        self.graph.blocks.get(local as usize)
    }

    /// The parent of block `local`.
    pub(crate) fn parent(&self, local: u32) -> Option<u32> {
        self.block(local)?.parent.filter(|&parent| parent < local)
    }

    /// The function, class, or file whose activation block `local` belongs to.
    pub(crate) fn owner(&self, local: u32) -> u32 {
        self.owners.get(local as usize).copied().unwrap_or(local)
    }

    /// The blocks inside block `local`, in preorder (not `local` itself).
    pub(crate) fn descendants(&self, local: u32) -> std::ops::Range<u32> {
        let end = self.ends.get(local as usize).copied().unwrap_or(local);
        local.saturating_add(1)..end.max(local.saturating_add(1))
    }

    /// The direct children of block `local`, in preorder.
    pub(crate) fn children(&self, local: u32) -> impl Iterator<Item = u32> + '_ {
        self.descendants(local)
            .filter(move |&child| self.parent(child) == Some(local))
    }

    /// The edge of occurrence `source` with sequence `seq`.
    pub(crate) fn edge(&self, source: u32, seq: u32) -> Option<&Edge> {
        run(&self.graph.edges, |edge| edge.source, source)
            .iter()
            .find(|edge| edge.seq == seq)
    }

    /// The `slot` of block `local` with `index` (`None` for a slot that doesn't repeat).
    pub(crate) fn slot(&self, local: u32, slot: SlotName, index: Option<u32>) -> Option<&Slot> {
        self.slots(local)
            .iter()
            .find(|item| item.slot == slot && item.index == index)
    }

    /// Every slot of block `local`.
    pub(crate) fn slots(&self, local: u32) -> &[Slot] {
        run(&self.graph.slots, |slot| slot.block, local)
    }

    /// The signature of function-like block `local`.
    pub(crate) fn signature(&self, local: u32) -> Option<&Signature> {
        run(&self.graph.types.signatures, |item| item.block, local).first()
    }

    /// The parameters of function-like block `local`, by index.
    pub(crate) fn parameters(&self, local: u32) -> &[Parameter] {
        run(&self.graph.types.parameters, |item| item.block, local)
    }

    /// The parameter of block `local` with `index`.
    pub(crate) fn parameter(&self, local: u32, index: u32) -> Option<&Parameter> {
        self.parameters(local)
            .iter()
            .find(|parameter| parameter.index == index)
    }

    /// The members of type declaration or class `local`, by index.
    pub(crate) fn members(&self, local: u32) -> &[Member] {
        run(&self.graph.types.members, |item| item.block, local)
    }

    /// The arguments of call or construction `local`, by index.
    pub(crate) fn arguments(&self, local: u32) -> &[Argument] {
        run(&self.graph.types.arguments, |item| item.call, local)
    }

    /// Where the value of call or construction `local` lands.
    pub(crate) fn result(&self, local: u32) -> Option<&CallResult> {
        run(&self.graph.types.results, |item| item.call, local).first()
    }

    /// The type links of block `local` (in range order) that name a base it extends.
    pub(crate) fn bases(&self, local: u32) -> impl Iterator<Item = &TypeLink> {
        run(&self.graph.types.type_links, |item| item.block, local)
            .iter()
            .filter(|link| link.heritage == Some(Heritage::Extends))
    }

    /// The type links whose range lies inside `range`, in order.
    pub(crate) fn links_in(&self, range: ByteRange) -> impl Iterator<Item = &TypeLink> {
        let links = &self.graph.types.type_links;
        let start = self
            .links
            .partition_point(|&index| links[index as usize].range.start < range.start);
        self.links[start..]
            .iter()
            .map(|&index| &links[index as usize])
            .take_while(move |link| link.range.start < range.end)
            .filter(move |link| range.contains(link.range))
    }

    /// `written` with the type names in it.
    pub(crate) fn type_text(&self, written: &WrittenType) -> TypeText {
        type_text(written, self.links_in(written.range))
    }

    /// The catalog label of block `local` (its name, `callback to …`, a kind, or the path).
    pub(crate) fn label(&self, local: u32) -> String {
        catalog::label(&self.graph, local).unwrap_or_default()
    }
}

/// The facts one request reads, each file read once and its source verified at most once.
pub struct Facts<'a> {
    source: &'a dyn FactSource,
    root: &'a Path,
    files: FxHashMap<RepoPath, Option<Rc<FileFacts>>>,
    texts: FxHashMap<RepoPath, Option<Rc<Snapshot>>>,
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

    /// The file at `path`.
    pub(crate) fn file(&mut self, path: &RepoPath) -> Result<Option<Rc<FileFacts>>> {
        if let Some(found) = self.files.get(path) {
            return Ok(found.clone());
        }
        let found = self
            .source
            .file_graph(path.as_str())?
            .map(|graph| Rc::new(FileFacts::new(graph)));
        self.files.insert(path.clone(), found.clone());
        Ok(found)
    }

    /// The file holding the block with `key`, and the block's local ID. `None` when no such
    /// block exists (or the store and its file disagree, which reads as missing).
    pub(crate) fn locate(&mut self, key: &str) -> Result<Option<(Rc<FileFacts>, u32)>> {
        let Some((path, local)) = self.source.locate(key)? else {
            return Ok(None);
        };
        let Some(file) = self.file(&path)? else {
            return Ok(None);
        };
        let matches = file.block(local).is_some_and(|block| block.key.as_str() == key);
        Ok(matches.then_some((file, local)))
    }

    /// The verified text of `file`, or `None` when its bytes changed or are missing.
    pub(crate) fn text(&mut self, file: &FileFacts) -> Option<Rc<Snapshot>> {
        let path = file.path();
        if let Some(found) = self.texts.get(path) {
            return found.clone();
        }
        let found = match verify_file(self.root, path, file.graph.file.hash) {
            Verified::Current(snapshot) => Some(Rc::new(snapshot)),
            Verified::Changed | Verified::Missing => None,
        };
        self.texts.insert(path.clone(), found.clone());
        found
    }
}
