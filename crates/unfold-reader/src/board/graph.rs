//! The slice of one project's graph a board needs: the files its references live in (with
//! their bytes verified against the facts' hash), the apps and entrances of `unfold.toml`, and
//! the qualified names of the targets its cards call. Board checks, signatures, guards, the
//! overlay checks, and health all read through it, so they are pure functions of the graph.
//!
//! | Reference | Source it is signed over | Guard signature |
//! | --- | --- | --- |
//! | A block | its extent: a definition from its first decorator to its end, any other block its range | the same |
//! | A combined step | from its first block's statement to the end of its last block's statement (its drawn span) | — |
//! | An app or entrance | each block of its `code` that has one, in order, each followed by `\n` | its `code` keys, each followed by `\n` |
//!
//! An app's or entrance's guard records its `code`, not its source, so a reference to an app or
//! entrance that was renamed can be offered the one that has the same `code`. A `code` key that
//! names no block (missing, or not analyzed) adds nothing to the source, so a heading-only app,
//! or an entrance none of whose code is found, is signed over nothing; once its code is found,
//! the signature changes and its description goes to review.
//!
//! Build a [`BoardGraph`] with [`BoardGraph::load`] from a store snapshot (only the files and
//! inputs a page's references touch), with [`BoardGraph::from_graph`] from a whole in-memory
//! graph, or piece by piece.

use std::path::Path;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{LinkKind, MemberKind, NodeKind, TargetState};
use unfold_core::{App, ByteRange, FileGraph, Graph, Key, LineIndex, LinkTarget, RepoPath};
use unfold_store::Reader;

use super::model::{References, Signature};
use crate::catalog::{Start, start, starts};
use crate::contracts::{FactSource, Facts, TypeDefinition, type_definition};
use crate::error::Result;
use crate::source::{Verified, verify_file};
use crate::tree::{Block, Blocks, FileTree, Trees};

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
    trees: Trees,
    sources: FxHashMap<RepoPath, Verified>,
    apps: Vec<App>,
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
        if self.trees.get(graph.file.path.as_str()).is_some() {
            return;
        }
        self.sources.insert(graph.file.path.clone(), source);
        self.trees.insert(FileTree::new(graph));
    }

    /// Sets the apps of `unfold.toml`, with their entrances.
    pub fn set_apps(&mut self, apps: Vec<App>) {
        self.apps = apps;
    }

    /// Adds an input's verified bytes: an evidence file anchors cite.
    pub fn add_input(&mut self, path: RepoPath, source: Verified) {
        self.inputs.insert(path, source);
    }

    /// Records the field names of the class or type `key` (its own, then the ones it inherits),
    /// which a table backed by it may mark.
    pub fn add_fields(&mut self, key: Key, fields: Vec<Box<str>>) {
        self.fields.insert(key, fields);
    }

    /// Records the fields of type `key` from its definition in `facts`, when it is a class or
    /// type.
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

    /// Every file, app, and input of `graph`, with sources verified under `root`, and the fields
    /// of every class and type.
    pub fn from_graph(graph: &Graph, root: &Path) -> Self {
        let mut loaded = Self::new();
        for file in &graph.files {
            let source = verify_file(root, &file.file.path, file.file.hash);
            loaded.add_file(FileGraph::clone(file), source);
        }
        for input in &graph.inputs {
            loaded.add_input(
                input.path.clone(),
                verify_file(root, &input.path, Some(input.hash)),
            );
        }
        loaded.set_apps(graph.apps.clone());
        let types: Vec<Key> = graph
            .files
            .iter()
            .flat_map(|file| &file.nodes)
            .filter(|node| matches!(node.kind(), NodeKind::Class | NodeKind::Type))
            .map(|node| node.key.clone())
            .collect();
        let mut facts = Facts::new(graph as &dyn FactSource, root);
        for key in types {
            // An in-memory graph answers every question: reading it never fails.
            let _ = loaded.define(&mut facts, key.as_str());
        }
        loaded
    }

    /// What `references` (as [`super::Board::references`] or [`super::Page::references`] lists
    /// them) need from the project's store snapshot: the files of the blocks (opened calls
    /// included) and of the apps' and entrances' code, the apps, the evidence files anchors
    /// cite, the fields of the tables' types, and the qualified names of targets outside those
    /// files. Sources are verified under `root`, the project's folder.
    pub fn load(reader: &Reader, root: &Path, references: &References<'_>) -> Result<Self> {
        let mut loaded = Self::new();
        loaded.set_apps(reader.apps()?);
        let mut code: Vec<Key> = Vec::new();
        for &key in references.keys.iter().chain(&references.open) {
            match loaded.entrance(key) {
                Some(start) => code.extend(start.targets().iter().map(|target| target.code.clone())),
                None => code.push(Key::from_text(key)),
            }
        }
        for key in code {
            if loaded.trees.block(key.as_str()).is_none()
                && let Some(located) = reader.node(key.as_str())?
            {
                loaded.load_file(reader, root, &located.path)?;
            }
        }
        let inputs = reader.inputs()?;
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
        for tree in loaded.trees.iter() {
            targets.extend(tree.graph().links.iter().filter_map(|link| match &link.to {
                LinkTarget::Project(key) => Some(key.clone()),
                _ => None,
            }));
        }
        targets.sort();
        targets.dedup();
        for key in targets {
            if loaded.trees.block(key.as_str()).is_none()
                && let Some(definition) = reader.definition(key.as_str())?
            {
                loaded.add_name(key, &definition.qualified_name);
            }
        }
        Ok(loaded)
    }

    fn load_file(&mut self, reader: &Reader, root: &Path, path: &RepoPath) -> Result<()> {
        if self.trees.get(path.as_str()).is_some() {
            return Ok(());
        }
        if let Some(file) = reader.file_graph(path.as_str())? {
            let source = verify_file(root, &file.file.path, file.file.hash);
            self.add_file(file, source);
        }
        Ok(())
    }

    /// The block with `key`.
    pub fn block(&self, key: &str) -> Option<Block<'_>> {
        self.trees.block(key)
    }

    /// The app or entrance with `key`.
    pub fn entrance(&self, key: &str) -> Option<Start<'_>> {
        start(&self.apps, key)
    }

    /// Every app and entrance, each app followed by its entrances, in file order.
    pub fn starts(&self) -> impl Iterator<Item = Start<'_>> {
        starts(&self.apps)
    }

    /// Whether a block, app, or entrance with `key` exists.
    pub fn exists(&self, key: &str) -> bool {
        self.block(key).is_some() || self.entrance(key).is_some()
    }

    /// The kind a guard records: the node kind, or `entrance` for an app or entrance.
    pub fn guard_kind(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => Some(block.kind().as_str().to_owned()),
            None => self.entrance(key).map(|_| "entrance".to_owned()),
        }
    }

    /// The owner a guard records: the owner's key for a block, empty for a module, an app, or
    /// an entrance.
    pub fn guard_owner(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => Some(
                block
                    .owner()
                    .map_or_else(String::new, |owner| owner.key().to_string()),
            ),
            None => self.entrance(key).map(|_| String::new()),
        }
    }

    /// The signature a guard records: a block's source signature, or the signature of an app's
    /// or entrance's `code` keys ([`code_signature`]).
    pub fn guard_signature(&self, key: &str) -> Option<Signature> {
        match self.block(key) {
            Some(_) => self.signature(key),
            None => self.entrance(key).map(|start| code_signature(&start)),
        }
    }

    /// The verified bytes of `range` in the file of `block`.
    fn block_bytes(&self, block: &Block<'_>, range: ByteRange) -> Option<&[u8]> {
        self.sources.get(block.tree.path())?.source()?.bytes(range)
    }

    /// A block's verified source bytes, over its extent ([`FileTree::extent`]): a decorated
    /// definition's decorators are part of it.
    pub fn bytes(&self, block: &Block<'_>) -> Option<&[u8]> {
        self.block_bytes(block, block.tree.extent(block.id))
    }

    /// The loaded blocks of an app's or entrance's `code`, in order.
    pub fn code_blocks<'a>(&'a self, start: &Start<'a>) -> impl Iterator<Item = Block<'a>> {
        start
            .targets()
            .iter()
            .filter_map(|target| self.block(target.code.as_str()))
    }

    /// A reference's exact verified source: a block's range, or the range of each block of an
    /// app's or entrance's `code` that has one, in order (none for a heading-only app, or code
    /// that is missing or not analyzed). `None` when the reference is missing, or when any of
    /// its source is not loaded or does not verify.
    pub fn sources(&self, key: &str) -> Option<Vec<&[u8]>> {
        if let Some(block) = self.block(key) {
            return self.bytes(&block).map(|bytes| vec![bytes]);
        }
        self.entrance(key)?
            .targets()
            .iter()
            .filter(|target| !matches!(target.state, TargetState::Missing | TargetState::NotAnalyzed))
            .map(|target| self.bytes(&self.block(target.code.as_str())?))
            .collect()
    }

    /// The signature of a reference's exact verified source ([`BoardGraph::sources`]): a
    /// block's bytes, or an app's or entrance's code blocks' bytes, each followed by `\n`.
    pub fn signature(&self, key: &str) -> Option<Signature> {
        if let Some(block) = self.block(key) {
            return self.bytes(&block).map(Signature::of);
        }
        let mut bytes = Vec::new();
        for source in self.sources(key)? {
            bytes.extend_from_slice(source);
            bytes.push(b'\n');
        }
        Some(Signature::of(&bytes))
    }

    /// Whether a `types` or `extends` link from `block` or from inside it reaches the class or
    /// type with `key`.
    pub fn uses_type(&self, block: &Block<'_>, key: &str) -> bool {
        block.tree.subtree(block.id).any(|id| {
            block.tree.links(id).iter().any(|link| {
                matches!(link.kind, LinkKind::Types | LinkKind::Extends)
                    && matches!(&link.to, LinkTarget::Project(target) if target.as_str() == key)
            })
        })
    }

    /// Lines `first` through `last` (one-based, inclusive) of the input at `path`.
    pub fn lines(&self, path: &RepoPath, [first, last]: [u32; 2]) -> Lines<'_> {
        let source = match self.inputs.get(path) {
            None => return Lines::NotRecorded,
            Some(Verified::Changed | Verified::Missing) => return Lines::Unverified,
            Some(Verified::Current(source)) => source,
        };
        let index = LineIndex::new(source.text());
        let (Some(start), true) = (index.line_start(first), last <= index.line_count()) else {
            return Lines::PastEnd;
        };
        let end = index.line_start(last + 1).unwrap_or(index.len());
        source
            .bytes(ByteRange { start, end })
            .map_or(Lines::PastEnd, Lines::Bytes)
    }

    /// The field names of class or type `key`, its own then the ones it inherits, when they
    /// were loaded.
    pub fn fields(&self, key: &str) -> Option<&[Box<str>]> {
        self.fields.get(key).map(Vec::as_slice)
    }

    /// The signature of a combined step's span over blocks `first` to `last`, when both are
    /// blocks of one verified file: from the start of the first block's statement (its range
    /// when it begins none) to the end of the last block's statement or range, whichever ends
    /// later. The drawing spans the same lines.
    pub fn span_signature(&self, first: &str, last: &str) -> Option<Signature> {
        let (first, last) = (self.block(first)?, self.block(last)?);
        if !std::ptr::eq(first.tree, last.tree) {
            return None;
        }
        let (first_node, last_node) = (first.node(), last.node());
        let start = first_node.stmt.unwrap_or(first_node.range).start;
        let end = last_node
            .stmt
            .map_or(last_node.range.end, |stmt| stmt.end.max(last_node.range.end));
        let range = ByteRange::new(start, end).ok()?;
        self.block_bytes(&first, range).map(Signature::of)
    }

    /// The qualified name of the definition `key`.
    pub fn qualified_name(&self, key: &str) -> Option<String> {
        match self.block(key) {
            Some(block) => Some(block.tree.qualified_name(block.id)),
            None => self.names.get(key).map(|name| name.to_string()),
        }
    }
}

impl Blocks for BoardGraph {
    fn block(&self, key: &str) -> Option<Block<'_>> {
        self.trees.block(key)
    }
}

/// The signature of an app's or entrance's `code` keys, each followed by `\n`: what its guard
/// records, and how a renamed one finds the one with the same `code`.
pub(crate) fn code_signature(start: &Start<'_>) -> Signature {
    let mut text = String::new();
    for target in start.targets() {
        text.push_str(target.code.as_str());
        text.push('\n');
    }
    Signature::of(text.as_bytes())
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
