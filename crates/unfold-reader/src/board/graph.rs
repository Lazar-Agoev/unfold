//! The slice of one project's graph a board needs: the files its references live in (with
//! their bytes verified against the facts' hash), the entrances it shows, and the qualified
//! names of the targets its cards call. A page's cards can come from several projects, so a
//! page reads through [`PageGraphs`]: one [`BoardGraph`] per connected project. Board checks,
//! signatures, guards, health, Mermaid export, and the outline all read through them, so they
//! are pure functions of the graphs.
//!
//! Build a [`BoardGraph`] with [`BoardGraph::load`] from a store snapshot (only the files and
//! inputs a page's references touch), with [`BoardGraph::from_graph`] from a whole in-memory
//! graph, or piece by piece.

use std::collections::BTreeMap;
use std::path::Path;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{MemberKind, Resolution, Role};
use unfold_core::{Block, ByteRange, Edge, Entrance, FileGraph, Graph, Key, LineIndex, RepoPath};
use unfold_store::Reader;

use super::model::{ProjectName, References, Signature};
use crate::catalog;
use crate::contracts::{FactSource, Facts, TypeDefinition, type_definition};
use crate::error::Result;
use crate::flow::{FlowContext, FlowModel, build_flow};
use crate::index::{file_owners, file_subtree_ends};
use crate::source::{Verified, verify_file};

/// One loaded file.
#[derive(Debug)]
struct GraphFile {
    graph: FileGraph,
    owners: Vec<u32>,
    ends: Vec<u32>,
    source: Verified,
}

impl GraphFile {
    /// The verified bytes of `range`, when the source is current and the range fits.
    fn bytes(&self, range: ByteRange) -> Option<&[u8]> {
        match &self.source {
            Verified::Current(snapshot) => snapshot.bytes().get(range.as_usize()),
            Verified::Changed | Verified::Missing => None,
        }
    }
}

/// A block found in a [`BoardGraph`].
#[derive(Debug, Clone, Copy)]
pub struct BlockAt<'a> {
    file: &'a GraphFile,
    local: u32,
}

impl<'a> BlockAt<'a> {
    /// The block.
    pub fn block(&self) -> &'a Block {
        &self.file.graph.blocks[self.local as usize]
    }

    /// The block's file graph.
    pub fn file(&self) -> &'a FileGraph {
        &self.file.graph
    }

    /// The block's local ID.
    pub fn local(&self) -> u32 {
        self.local
    }

    /// The key of the block's owner (the function, class, or file whose activation it is in).
    pub fn owner(&self) -> &'a Key {
        let owner = self.file.owners[self.local as usize];
        &self.file.graph.blocks[owner as usize].key
    }

    /// The parent's local ID.
    pub fn parent(&self) -> Option<u32> {
        self.block().parent
    }

    /// Whether this block is `ancestor` or lies inside it (both in the same file).
    pub fn within(&self, ancestor: &BlockAt<'_>) -> bool {
        std::ptr::eq(self.file, ancestor.file)
            && ancestor.local <= self.local
            && self.local < self.file.ends[ancestor.local as usize]
    }

    /// The siblings with the same parent and role, in source order.
    pub fn siblings(&self) -> Vec<u32> {
        let block = self.block();
        let Some(parent) = block.parent else {
            return vec![self.local];
        };
        let end = self.file.ends[parent as usize];
        (parent + 1..end)
            .filter(|&local| {
                let other = &self.file.graph.blocks[local as usize];
                other.parent == Some(parent) && other.role == block.role
            })
            .collect()
    }

    /// The signature of the block's verified source.
    pub fn signature(&self) -> Option<Signature> {
        self.bytes().map(Signature::of)
    }

    /// The block's verified source bytes.
    pub fn bytes(&self) -> Option<&'a [u8]> {
        self.file.bytes(self.block().range)
    }

    /// The occurrence edge of a `call` or `new` block (its edge 0).
    pub fn occurrence(&self) -> Option<&'a Edge> {
        self.file
            .graph
            .edges
            .iter()
            .find(|edge| edge.source == self.local && edge.seq == 0)
    }

    /// Whether a type link inside the block (the block's own included) resolves to the
    /// declaration with `key`.
    pub fn uses_type(&self, key: &str) -> bool {
        let end = self.file.ends[self.local as usize];
        self.file.graph.types.type_links.iter().any(|link| {
            (self.local..end).contains(&link.block)
                && link.resolution == Resolution::Resolved
                && link.target.as_ref().is_some_and(|target| target.as_str() == key)
        })
    }
}

/// A line range of an input the graph records, as [`BoardGraph::lines`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lines<'a> {
    /// The lines' verified bytes, from the start of the first through the end of the last
    /// (its terminator included).
    Bytes(&'a [u8]),
    /// The input's bytes on disk differ from the hash the graph recorded, or can't be read.
    Unverified,
    /// The graph records no input at that path.
    NotRecorded,
    /// The input has fewer lines.
    PastEnd,
}

/// The part of the graph a board reads.
#[derive(Debug, Default)]
pub struct BoardGraph {
    files: Vec<GraphFile>,
    by_path: FxHashMap<RepoPath, usize>,
    blocks: FxHashMap<Key, (usize, u32)>,
    entrances: FxHashMap<Key, Entrance>,
    inputs: FxHashMap<RepoPath, Verified>,
    names: FxHashMap<Key, Box<str>>,
    fields: FxHashMap<Key, Vec<Box<str>>>,
}

impl BoardGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file with its verified source. A file added twice keeps the first copy.
    pub fn add_file(&mut self, graph: FileGraph, source: Verified) {
        if self.by_path.contains_key(&graph.file.path) {
            return;
        }
        let index = self.files.len();
        for block in &graph.blocks {
            self.blocks.insert(block.key.clone(), (index, block.local_id));
        }
        self.by_path.insert(graph.file.path.clone(), index);
        self.files.push(GraphFile {
            owners: file_owners(&graph.blocks),
            ends: file_subtree_ends(&graph.blocks),
            graph,
            source,
        });
    }

    /// Adds an entrance.
    pub fn add_entrance(&mut self, entrance: Entrance) {
        self.entrances.insert(entrance.key.clone(), entrance);
    }

    /// Adds an input's verified bytes: a configuration input holding entrance evidence, or an
    /// evidence file anchors cite.
    pub fn add_input(&mut self, path: RepoPath, source: Verified) {
        self.inputs.insert(path, source);
    }

    /// Records the field names of the type declaration or class `key` (its own, then the ones
    /// it inherits), which a table backed by it may mark.
    pub fn add_fields(&mut self, key: Key, fields: Vec<Box<str>>) {
        self.fields.insert(key, fields);
    }

    /// Records the fields of type `key` from its definition in `facts`, when it is a type
    /// declaration or class.
    fn define(&mut self, facts: &mut Facts<'_>, key: &str) -> Result<()> {
        if let Some(definition) = type_definition(facts, key)? {
            self.add_fields(definition.key.clone(), fields_of(&definition));
        }
        Ok(())
    }

    /// Records the qualified name of a definition whose file is not loaded.
    pub fn add_name(&mut self, key: Key, name: &str) {
        self.names.insert(key, name.into());
    }

    /// Every file, entrance, and input of `graph`, with sources verified under `root`, and the
    /// fields of every type declaration and class.
    pub fn from_graph(graph: &Graph, root: &Path) -> Self {
        let mut loaded = Self::new();
        for file in &graph.files {
            let source = verify_file(root, &file.file.path, file.file.hash);
            loaded.add_file(file.clone(), source);
        }
        for input in &graph.inputs {
            loaded.add_input(
                input.path.clone(),
                verify_file(root, &input.path, Some(input.hash)),
            );
        }
        for entrance in &graph.entrances {
            loaded.add_entrance(entrance.clone());
        }
        let types: Vec<Key> = graph
            .files
            .iter()
            .flat_map(|file| &file.blocks)
            .filter(|block| block.kind.is_type_declaration())
            .map(|block| block.key.clone())
            .collect();
        let mut facts = Facts::new(graph as &dyn FactSource, root);
        for key in types {
            // An in-memory graph answers every question: reading it never fails.
            let _ = loaded.define(&mut facts, key.as_str());
        }
        loaded
    }

    /// What `references` (of one project, as [`super::Board::references`] or
    /// [`super::Page::references`] lists them) need from that project's store snapshot: the
    /// files of the blocks and of the entrances' evidence, the entrances, the inputs holding
    /// configuration evidence and the evidence files anchors cite, the fields of the tables'
    /// types, and the qualified names of targets outside those files. Sources are verified under
    /// `root`, the project's folder.
    pub fn load(reader: &Reader, root: &Path, references: &References<'_>) -> Result<Self> {
        let mut loaded = Self::new();
        let mut evidence: Vec<RepoPath> = Vec::new();
        for &key in &references.keys {
            if key.starts_with("entrance:") {
                if let Some(entrance) = reader.entrance(key)? {
                    evidence.push(entrance.evidence.path.clone());
                    loaded.add_entrance(entrance);
                }
            } else if !loaded.blocks.contains_key(key)
                && let Some(located) = reader.block(key)?
            {
                loaded.load_file(reader, root, &located.path)?;
            }
        }
        let inputs = reader.inputs()?;
        // Several entrances (manual ones in `unfold.toml`, say) share one evidence file: read and
        // verify it once.
        evidence.sort();
        evidence.dedup();
        for path in evidence {
            match inputs.iter().find(|input| input.path == path) {
                Some(input) => loaded.add_input(path.clone(), verify_file(root, &path, Some(input.hash))),
                None => loaded.load_file(reader, root, &path)?,
            }
        }
        // An anchor's file is read only when the graph records it as an input.
        for &path in &references.files {
            if let Some(input) = inputs.iter().find(|input| &input.path == path) {
                loaded.add_input(path.clone(), verify_file(root, path, Some(input.hash)));
            }
        }
        let mut facts = Facts::new(reader as &dyn FactSource, root);
        for &table in &references.tables {
            loaded.define(&mut facts, table)?;
        }
        let mut targets: Vec<Key> = Vec::new();
        for file in &loaded.files {
            targets.extend(
                file.graph
                    .edges
                    .iter()
                    .filter(|edge| edge.resolution == Resolution::Resolved)
                    .filter_map(|edge| edge.target.clone()),
            );
        }
        for entrance in loaded.entrances.values() {
            targets.extend(entrance.targets.iter().filter_map(|target| target.target.clone()));
        }
        targets.sort();
        targets.dedup();
        for key in targets {
            if !loaded.blocks.contains_key(&key)
                && let Some(definition) = reader.definition(key.as_str())?
            {
                loaded.add_name(key, &definition.qualified_name);
            }
        }
        Ok(loaded)
    }

    fn load_file(&mut self, reader: &Reader, root: &Path, path: &RepoPath) -> Result<()> {
        if self.by_path.contains_key(path) {
            return Ok(());
        }
        if let Some(file) = reader.file_graph(path.as_str())? {
            let source = verify_file(root, &file.file.path, file.file.hash);
            self.add_file(file, source);
        }
        Ok(())
    }

    /// The block with `key`.
    pub fn block(&self, key: &str) -> Option<BlockAt<'_>> {
        self.blocks.get(key).map(|&(file, local)| BlockAt {
            file: &self.files[file],
            local,
        })
    }

    /// The entrance with `key`.
    pub fn entrance(&self, key: &str) -> Option<&Entrance> {
        self.entrances.get(key)
    }

    /// Whether a block or entrance with `key` exists.
    pub fn exists(&self, key: &str) -> bool {
        self.blocks.contains_key(key) || self.entrances.contains_key(key)
    }

    /// The kind a guard records: the block kind, or `entrance:<kind>`.
    pub fn guard_kind(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => Some(block.block().kind.as_str().to_owned()),
            None => self
                .entrance(key)
                .map(|entrance| format!("entrance:{}", entrance.kind.as_str())),
        }
    }

    /// The owner a guard records: the owner's key for a block, empty for an entrance.
    pub fn guard_owner(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => Some(block.owner().to_string()),
            None => self.entrance(key).map(|_| String::new()),
        }
    }

    /// The verified bytes of `range` in the file or input at `path`.
    fn bytes(&self, path: &RepoPath, range: ByteRange) -> Option<&[u8]> {
        if let Some(&file) = self.by_path.get(path) {
            return self.files[file].bytes(range);
        }
        match self.inputs.get(path)? {
            Verified::Current(snapshot) => snapshot.bytes().get(range.as_usize()),
            Verified::Changed | Verified::Missing => None,
        }
    }

    /// A reference's exact verified source: a block's range, or an entrance's evidence. `None`
    /// when the reference is missing or its source does not verify.
    pub fn key_bytes(&self, key: &str) -> Option<&[u8]> {
        match self.block(key) {
            Some(block) => block.bytes(),
            None => {
                let entrance = self.entrance(key)?;
                self.bytes(&entrance.evidence.path, entrance.evidence.range)
            }
        }
    }

    /// The signature of a reference's exact verified source ([`BoardGraph::key_bytes`]).
    pub fn signature(&self, key: &str) -> Option<Signature> {
        self.key_bytes(key).map(Signature::of)
    }

    /// Lines `first` through `last` (one-based, inclusive) of the input at `path`.
    pub fn lines(&self, path: &RepoPath, [first, last]: [u32; 2]) -> Lines<'_> {
        let snapshot = match self.inputs.get(path) {
            None => return Lines::NotRecorded,
            Some(Verified::Changed | Verified::Missing) => return Lines::Unverified,
            Some(Verified::Current(snapshot)) => snapshot,
        };
        let index = LineIndex::new(snapshot.text());
        let (Some(start), true) = (index.line_start(first), last <= index.line_count()) else {
            return Lines::PastEnd;
        };
        let end = index.line_start(last + 1).unwrap_or(index.len());
        snapshot
            .bytes()
            .get(start as usize..end as usize)
            .map_or(Lines::PastEnd, Lines::Bytes)
    }

    /// The field names of type declaration or class `key`, its own then the ones it inherits,
    /// when they were loaded.
    pub fn fields(&self, key: &str) -> Option<&[Box<str>]> {
        self.fields.get(key).map(Vec::as_slice)
    }

    /// The signature of the source from the start of `first` to the end of `last` (a group's
    /// span), when both are blocks of one verified file.
    pub fn span_signature(&self, first: &str, last: &str) -> Option<Signature> {
        let (first, last) = (self.block(first)?, self.block(last)?);
        if !std::ptr::eq(first.file, last.file) {
            return None;
        }
        let range = ByteRange {
            start: first.block().range.start,
            end: last.block().range.end,
        };
        (range.start <= range.end)
            .then(|| first.file.bytes(range))
            .flatten()
            .map(Signature::of)
    }

    /// The qualified name of the definition `key`.
    pub fn qualified_name(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => catalog::qualified_name(block.file(), block.local()),
            None => self.names.get(key).map(|name| name.to_string()),
        }
    }

    /// The Flow of block `key`, labeled through `context`.
    pub fn flow(&self, key: &str, context: &dyn FlowContext) -> Option<FlowModel> {
        let block = self.block(key)?;
        let (text, state) = match &block.file.source {
            Verified::Current(snapshot) => (Some(snapshot.text()), block.file.source.state()),
            other => (None, other.state()),
        };
        build_flow(block.file(), block.local(), text, state, context).ok()
    }

    /// The edges owned by block `key`, in source order, as (occurrence key, edge).
    pub fn owned_edges(&self, key: &str) -> Vec<(&Key, &Edge)> {
        let Some(block) = self.block(key) else {
            return Vec::new();
        };
        let file = block.file;
        file.graph
            .edges
            .iter()
            .filter(|edge| file.owners.get(edge.source as usize) == Some(&block.local))
            .map(|edge| (&file.graph.blocks[edge.source as usize].key, edge))
            .collect()
    }

    /// Whether a group's member role is one a group may have.
    pub(crate) fn is_group_role(role: Option<Role>) -> bool {
        matches!(
            role,
            Some(Role::Body | Role::Then | Role::Else | Role::Catch | Role::Finally)
        )
    }
}

/// The names of a definition's fields (properties), its own then the ones it inherits, each
/// once.
fn fields_of(definition: &TypeDefinition) -> Vec<Box<str>> {
    let mut fields: Vec<Box<str>> = Vec::new();
    let members = definition
        .members
        .iter()
        .chain(definition.inherited.iter().flat_map(|group| &group.members));
    for member in members.filter(|member| member.kind == MemberKind::Property) {
        if let Some(name) = member.name.as_deref()
            && !fields.iter().any(|known| &**known == name)
        {
            fields.push(name.into());
        }
    }
    fields
}

/// The graphs a page reads: the board's own project's, and one per other connected project by
/// shared name. A project a card names that has no graph here is **not connected**: its cards
/// are placeholders, and its labels, groups, and guards are kept unchecked.
#[derive(Debug, Clone, Copy)]
pub struct PageGraphs<'a> {
    own: &'a BoardGraph,
    others: &'a BTreeMap<ProjectName, BoardGraph>,
}

/// No other project.
static NONE: BTreeMap<ProjectName, BoardGraph> = BTreeMap::new();

impl<'a> PageGraphs<'a> {
    /// The board's own project's graph and the graphs of the other connected projects (those
    /// absent from `others` are not connected).
    pub fn new(own: &'a BoardGraph, others: &'a BTreeMap<ProjectName, BoardGraph>) -> Self {
        Self { own, others }
    }

    /// Only the board's own project.
    pub fn own_only(own: &'a BoardGraph) -> Self {
        Self::new(own, &NONE)
    }

    /// The graph of `project` (`None`: the board's own), when it is connected.
    pub fn get(&self, project: Option<&ProjectName>) -> Option<&'a BoardGraph> {
        match project {
            None => Some(self.own),
            Some(name) => self.others.get(name),
        }
    }
}
