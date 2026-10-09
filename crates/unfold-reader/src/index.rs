//! The compact structure every derived view walks: blocks numbered across files, their parents,
//! kinds, roles, owners, and subtrees, edges grouped by owner and by target, the classes each
//! scope owns, and the entrances with their targets resolved to block numbers.
//!
//! It holds no names, ranges, or slots, so it stays small enough to keep per graph version; the
//! views that need text read it from the [`Graph`] it was built from or from per-file store
//! queries. Building it is one pass over the files plus counting sorts: no per-block allocation
//! beyond a key's reference count.

use rustc_hash::FxHashMap;
use unfold_core::vocab::{BlockKind, DetectorId, EdgeKind, EntranceKind, Language, Resolution, Role};
use unfold_core::{Block, Entrance, FileGraph, Graph, Key, RepoPath};

/// A block's number in an index: its position across all files, in graph (path, then preorder)
/// order.
pub type BlockId = u32;

/// The absent [`BlockId`].
pub(crate) const NONE: BlockId = u32::MAX;

/// Whether a block of `child_role` under a parent of `parent_kind` belongs to that parent's
/// activation (architecture §10.1): every child of a file; a class's children except Python
/// header expressions; a callable's body and parameters.
pub(crate) fn owns(parent_kind: BlockKind, child_role: Option<Role>) -> bool {
    match parent_kind {
        BlockKind::File => true,
        BlockKind::Class => child_role != Some(Role::Header),
        kind if kind.is_function_like() => matches!(child_role, Some(Role::Body | Role::Parameter)),
        _ => false,
    }
}

/// The owner of every block of one file, by local ID: the function, class, or file whose
/// activation the block belongs to. Blocks come in preorder, so one pass suffices: a block's
/// owner is its parent when the parent owns it, and otherwise the parent's owner. A block with
/// no valid parent owns itself.
pub fn file_owners(blocks: &[Block]) -> Vec<u32> {
    let mut owners: Vec<u32> = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let local = to_u32(index);
        let owner = match block.parent.filter(|&parent| parent < local) {
            None => local,
            Some(parent) if owns(blocks[parent as usize].kind, block.role) => parent,
            Some(parent) => owners[parent as usize],
        };
        owners.push(owner);
    }
    owners
}

/// The end (exclusive local ID) of every block's subtree in one file. In preorder a subtree is
/// the contiguous run from the block to its end.
pub(crate) fn file_subtree_ends(blocks: &[Block]) -> Vec<u32> {
    let mut ends: Vec<u32> = (1..=to_u32(blocks.len())).collect();
    for (index, block) in blocks.iter().enumerate().rev() {
        if let Some(parent) = block.parent.filter(|&parent| (parent as usize) < index) {
            let end = ends[index];
            let slot = &mut ends[parent as usize];
            *slot = (*slot).max(end);
        }
    }
    ends
}

pub(crate) fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// The facts of one edge that derived views need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EdgeFact {
    /// The occurrence block.
    pub source: BlockId,
    /// The resolved target, or [`NONE`] (also when a resolved target's key is not indexed).
    pub target: BlockId,
    pub seq: u32,
    pub position: Option<u32>,
    pub kind: EdgeKind,
    pub resolution: Resolution,
}

impl EdgeFact {
    /// The target, when the edge is resolved to an indexed block.
    pub fn resolved_target(&self) -> Option<BlockId> {
        (self.resolution == Resolution::Resolved && self.target != NONE).then_some(self.target)
    }
}

/// The facts of one entrance that derived views need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntranceFact {
    pub key: Key,
    pub kind: EntranceKind,
    pub detector: DetectorId,
    /// The source evidence block, or [`NONE`].
    pub evidence_block: BlockId,
    /// The first target when it is resolved to an indexed block, or [`NONE`].
    pub first: BlockId,
    /// Every target resolved to an indexed block, in position order.
    pub resolved: Box<[BlockId]>,
}

/// Items grouped into numbered buckets, in insertion order within each bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Buckets {
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Buckets {
    /// Groups `(bucket, item)` pairs by a counting sort; buckets are `0..count`.
    fn new(count: usize, pairs: &[(u32, u32)]) -> Self {
        let mut starts = vec![0u32; count + 1];
        for &(bucket, _) in pairs {
            starts[bucket as usize + 1] += 1;
        }
        for index in 0..count {
            starts[index + 1] += starts[index];
        }
        let mut fill = starts.clone();
        let mut items = vec![0u32; pairs.len()];
        for &(bucket, item) in pairs {
            let at = &mut fill[bucket as usize];
            items[*at as usize] = item;
            *at += 1;
        }
        Self { starts, items }
    }

    fn get(&self, bucket: BlockId) -> &[u32] {
        let bucket = bucket as usize;
        match (self.starts.get(bucket), self.starts.get(bucket + 1)) {
            (Some(&start), Some(&end)) => &self.items[start as usize..end as usize],
            _ => &[],
        }
    }
}

/// The compact graph structure. Build it from a [`Graph`] with [`GraphIndex::from_graph`], or
/// file by file (from store reads) with a [`GraphIndexBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphIndex {
    keys: Vec<Key>,
    by_key: FxHashMap<Key, BlockId>,
    file_of: Vec<u32>,
    file_starts: Vec<BlockId>,
    paths: Vec<RepoPath>,
    languages: Vec<Language>,
    parent: Vec<BlockId>,
    kind: Vec<BlockKind>,
    role: Vec<Option<Role>>,
    owner: Vec<BlockId>,
    subtree_end: Vec<BlockId>,
    edges: Vec<EdgeFact>,
    owned_edges: Buckets,
    incoming: Buckets,
    owned_classes: Buckets,
    entrances: Vec<EntranceFact>,
}

impl GraphIndex {
    /// Indexes `graph`. Block numbers follow its file order, so block `i` of file `f` is
    /// [`GraphIndex::file_start`]`(f) + i`.
    pub fn from_graph(graph: &Graph) -> Self {
        let mut builder = GraphIndexBuilder::new();
        for file in &graph.files {
            builder.add_file(file);
        }
        builder.finish(&graph.entrances)
    }

    /// The number of blocks.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the index has no blocks.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The block with `key`.
    pub fn id(&self, key: &str) -> Option<BlockId> {
        self.by_key.get(key).copied()
    }

    /// A block's key.
    pub fn key(&self, id: BlockId) -> &Key {
        &self.keys[id as usize]
    }

    /// A block's kind.
    pub fn kind(&self, id: BlockId) -> BlockKind {
        self.kind[id as usize]
    }

    /// A block's role.
    pub fn role(&self, id: BlockId) -> Option<Role> {
        self.role[id as usize]
    }

    /// A block's parent.
    pub fn parent(&self, id: BlockId) -> Option<BlockId> {
        let parent = self.parent[id as usize];
        (parent != NONE).then_some(parent)
    }

    /// The function, class, or file whose activation the block belongs to.
    pub fn owner(&self, id: BlockId) -> BlockId {
        self.owner[id as usize]
    }

    /// Whether `id` is `ancestor` or lies inside it.
    pub fn within(&self, id: BlockId, ancestor: BlockId) -> bool {
        ancestor <= id && id < self.subtree_end[ancestor as usize]
    }

    /// The position of the block's file in graph order.
    pub fn file_of(&self, id: BlockId) -> usize {
        self.file_of[id as usize] as usize
    }

    /// The number of the first block (the root) of the file at position `file`.
    pub fn file_start(&self, file: usize) -> BlockId {
        self.file_starts[file]
    }

    /// The block's local ID in its file.
    pub fn local_id(&self, id: BlockId) -> u32 {
        id - self.file_starts[self.file_of(id)]
    }

    /// The root of the block's file.
    pub fn file_root(&self, id: BlockId) -> BlockId {
        self.file_starts[self.file_of(id)]
    }

    /// The path of the block's file.
    pub fn path(&self, id: BlockId) -> &RepoPath {
        &self.paths[self.file_of(id)]
    }

    /// The language of the block's file.
    pub fn language(&self, id: BlockId) -> Language {
        self.languages[self.file_of(id)]
    }

    /// The nearest class at or above `id`.
    pub fn enclosing_class(&self, id: BlockId) -> Option<BlockId> {
        let mut current = Some(id);
        while let Some(block) = current {
            if self.kind(block) == BlockKind::Class {
                return Some(block);
            }
            current = self.parent(block);
        }
        None
    }

    /// The edges whose occurrence `owner` owns, in graph order.
    pub(crate) fn owned_edges(&self, owner: BlockId) -> impl Iterator<Item = &EdgeFact> {
        self.owned_edges
            .get(owner)
            .iter()
            .map(|&edge| &self.edges[edge as usize])
    }

    /// The resolved edges that target `target`, in graph order.
    pub(crate) fn incoming(&self, target: BlockId) -> impl Iterator<Item = &EdgeFact> {
        self.incoming
            .get(target)
            .iter()
            .map(|&edge| &self.edges[edge as usize])
    }

    /// The classes whose owner is `owner`, in preorder.
    pub(crate) fn owned_classes(&self, owner: BlockId) -> &[BlockId] {
        self.owned_classes.get(owner)
    }

    /// Every edge, in graph order.
    pub(crate) fn edges(&self) -> &[EdgeFact] {
        &self.edges
    }

    /// The entrances, in canonical order.
    pub(crate) fn entrances(&self) -> &[EntranceFact] {
        &self.entrances
    }
}

/// Builds a [`GraphIndex`] one file at a time, so a caller streaming files from the store never
/// holds the whole graph.
#[derive(Debug, Default)]
pub struct GraphIndexBuilder {
    keys: Vec<Key>,
    file_of: Vec<u32>,
    file_starts: Vec<BlockId>,
    paths: Vec<RepoPath>,
    languages: Vec<Language>,
    parent: Vec<BlockId>,
    kind: Vec<BlockKind>,
    role: Vec<Option<Role>>,
    owner: Vec<BlockId>,
    subtree_end: Vec<BlockId>,
    edges: Vec<EdgeFact>,
    /// The target key of each edge in `edges`, exactly when it is resolved.
    targets: Vec<Option<Key>>,
}

impl GraphIndexBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the next file. Files must come in path order for block numbers to follow graph
    /// order; derived views only need each file's blocks to be in preorder.
    pub fn add_file(&mut self, file: &FileGraph) {
        let base = to_u32(self.keys.len());
        let file_index = to_u32(self.paths.len());
        self.file_starts.push(base);
        self.paths.push(file.file.path.clone());
        self.languages.push(file.file.language);
        let owners = file_owners(&file.blocks);
        let ends = file_subtree_ends(&file.blocks);
        for (index, block) in file.blocks.iter().enumerate() {
            let local = to_u32(index);
            self.keys.push(block.key.clone());
            self.file_of.push(file_index);
            self.parent.push(
                block
                    .parent
                    .filter(|&parent| parent < local)
                    .map_or(NONE, |parent| base + parent),
            );
            self.kind.push(block.kind);
            self.role.push(block.role);
            self.owner.push(base + owners[index]);
            self.subtree_end.push(base + ends[index]);
        }
        let count = to_u32(file.blocks.len());
        for edge in file.edges.iter().filter(|edge| edge.source < count) {
            self.edges.push(EdgeFact {
                source: base + edge.source,
                target: NONE,
                seq: edge.seq,
                position: edge.position,
                kind: edge.kind,
                resolution: edge.resolution,
            });
            self.targets.push(
                edge.target
                    .clone()
                    .filter(|_| edge.resolution == Resolution::Resolved),
            );
        }
    }

    /// Resolves edge and entrance targets to block numbers and groups edges and classes.
    pub fn finish(mut self, entrances: &[Entrance]) -> GraphIndex {
        let by_key: FxHashMap<Key, BlockId> = self
            .keys
            .iter()
            .enumerate()
            .map(|(id, key)| (key.clone(), to_u32(id)))
            .collect();
        let lookup = |key: Option<&Key>| key.and_then(|key| by_key.get(key.as_str()).copied());
        for (edge, target) in self.edges.iter_mut().zip(&self.targets) {
            edge.target = lookup(target.as_ref()).unwrap_or(NONE);
        }
        let count = self.keys.len();
        let owned_pairs: Vec<(u32, u32)> = self
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| (self.owner[edge.source as usize], to_u32(index)))
            .collect();
        let incoming_pairs: Vec<(u32, u32)> = self
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, edge)| edge.resolved_target().map(|target| (target, to_u32(index))))
            .collect();
        let class_pairs: Vec<(u32, u32)> = (0..count)
            .filter(|&id| self.kind[id] == BlockKind::Class && self.owner[id] != to_u32(id))
            .map(|id| (self.owner[id], to_u32(id)))
            .collect();
        let entrances = entrances
            .iter()
            .map(|entrance| {
                let resolved = |target: &unfold_core::EntranceTarget| {
                    if target.resolution == Resolution::Resolved {
                        lookup(target.target.as_ref())
                    } else {
                        None
                    }
                };
                EntranceFact {
                    key: entrance.key.clone(),
                    kind: entrance.kind,
                    detector: entrance.detector,
                    evidence_block: lookup(entrance.evidence.block.as_ref()).unwrap_or(NONE),
                    first: entrance.targets.first().and_then(resolved).unwrap_or(NONE),
                    resolved: entrance.targets.iter().filter_map(resolved).collect(),
                }
            })
            .collect();
        self.file_starts.push(to_u32(count));
        GraphIndex {
            owned_edges: Buckets::new(count, &owned_pairs),
            incoming: Buckets::new(count, &incoming_pairs),
            owned_classes: Buckets::new(count, &class_pairs),
            keys: self.keys,
            by_key,
            file_of: self.file_of,
            file_starts: self.file_starts,
            paths: self.paths,
            languages: self.languages,
            parent: self.parent,
            kind: self.kind,
            role: self.role,
            owner: self.owner,
            subtree_end: self.subtree_end,
            edges: self.edges,
            entrances,
        }
    }
}
