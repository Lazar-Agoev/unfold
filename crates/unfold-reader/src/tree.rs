//! One file's node tree with the structure every view walks ([`FileTree`]), a set of them that
//! finds blocks by key across files ([`Trees`]), and a block found that way ([`Block`]).
//!
//! A [`FileTree`] owns one [`FileGraph`] and adds what its records only imply: each node's
//! children, the end of its subtree (a subtree is a contiguous run in preorder), the definition
//! it belongs to (through [`unfold_core::owner`], the one ownership rule), where a definition's
//! source starts (its first decorator), a key index, and the `types` and `extends` links in
//! range order (a written type's names are found by range).
//! Typed records and links are looked up by node. Nothing here reads key text: kinds, parents,
//! and files come from the records.

use std::ops::Range;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{CallRole, Form, Language, LinkKind, NodeKind};
use unfold_core::{
    Argument, ByteRange, File, FileGraph, Key, Link, Member, Node, Parameter, RepoPath, Signature, owner,
};

use crate::catalog;

/// One file's nodes, links, and typed records, with their structure indexed.
#[derive(Debug, Clone)]
pub struct FileTree {
    graph: FileGraph,
    by_key: FxHashMap<Key, u32>,
    /// `children[child_starts[id]..child_starts[id + 1]]` are node `id`'s children, in preorder.
    child_starts: Vec<u32>,
    children: Vec<u32>,
    /// The exclusive end of each node's subtree.
    ends: Vec<u32>,
    owners: Vec<Option<u32>>,
    /// Where each node's source starts: a definition's first decorator, else its range.
    starts: Vec<u32>,
    /// Indexes into the file's links of its `types` and `extends` links, by range start.
    type_links: Vec<u32>,
}

/// The records of `items` (sorted by `key`) whose key is `value`.
fn run<T>(items: &[T], key: impl Fn(&T) -> u32, value: u32) -> &[T] {
    let start = items.partition_point(|item| key(item) < value);
    let end = start + items[start..].partition_point(|item| key(item) == value);
    &items[start..end]
}

pub(crate) fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Whether `form` is a control block written as an expression.
pub(crate) fn is_expression(form: Form) -> bool {
    matches!(form, Form::Ternary | Form::Comprehension)
}

impl FileTree {
    /// Indexes `graph`, whose nodes are in preorder with every parent before its children (as
    /// validation guarantees).
    pub fn new(graph: FileGraph) -> Self {
        let count = graph.nodes.len();
        let mut child_starts = vec![0u32; count + 1];
        for node in &graph.nodes {
            if let Some(parent) = node.parent {
                child_starts[parent as usize + 1] += 1;
            }
        }
        for index in 0..count {
            child_starts[index + 1] += child_starts[index];
        }
        let mut fill = child_starts.clone();
        let mut children = vec![0u32; child_starts[count] as usize];
        for (id, node) in graph.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                let at = &mut fill[parent as usize];
                children[*at as usize] = to_u32(id);
                *at += 1;
            }
        }
        let mut ends: Vec<u32> = (1..=to_u32(count)).collect();
        for (id, node) in graph.nodes.iter().enumerate().rev() {
            if let Some(parent) = node.parent.filter(|&parent| (parent as usize) < id) {
                ends[parent as usize] = ends[parent as usize].max(ends[id]);
            }
        }
        let owners = (0..to_u32(count)).map(|id| owner(&graph.nodes, id)).collect();
        let mut starts: Vec<u32> = graph.nodes.iter().map(|node| node.range.start).collect();
        for decorator in &graph.nodes {
            let Some(definition) = decorator
                .r#for
                .filter(|_| decorator.role == Some(CallRole::Decorator))
            else {
                continue;
            };
            let decorates = graph.nodes.get(definition as usize).is_some_and(|node| {
                node.kind().is_definition() && node.parent.is_some() && node.parent == decorator.parent
            });
            if decorates {
                let start = &mut starts[definition as usize];
                *start = (*start).min(decorator.range.start);
            }
        }
        let by_key = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(id, node)| (node.key.clone(), to_u32(id)))
            .collect();
        let mut type_links: Vec<u32> = graph
            .links
            .iter()
            .enumerate()
            .filter(|(_, link)| matches!(link.kind, LinkKind::Types | LinkKind::Extends))
            .map(|(index, _)| to_u32(index))
            .collect();
        type_links.sort_by_key(|&index| graph.links[index as usize].range);
        Self {
            graph,
            by_key,
            child_starts,
            children,
            ends,
            owners,
            starts,
            type_links,
        }
    }

    /// The file's records.
    pub fn graph(&self) -> &FileGraph {
        &self.graph
    }

    /// The file's record: path, language, state, hash.
    pub fn file(&self) -> &File {
        &self.graph.file
    }

    /// The file's path.
    pub fn path(&self) -> &RepoPath {
        &self.graph.file.path
    }

    /// The file's language.
    pub fn language(&self) -> Language {
        self.graph.file.language
    }

    /// Every node, in preorder; the module is node 0. Empty for a failed file.
    pub fn nodes(&self) -> &[Node] {
        &self.graph.nodes
    }

    /// Node `id`. Panics when `id` is not a node of the file; IDs from this tree always are.
    pub fn node(&self, id: u32) -> &Node {
        &self.graph.nodes[id as usize]
    }

    /// The node with `key`.
    pub fn id(&self, key: &str) -> Option<u32> {
        self.by_key.get(key).copied()
    }

    /// Node `id`'s key.
    pub fn key(&self, id: u32) -> &Key {
        &self.node(id).key
    }

    /// Node `id`'s kind.
    pub fn kind(&self, id: u32) -> NodeKind {
        self.node(id).kind()
    }

    /// Node `id`'s parent; `None` for the module.
    pub fn parent(&self, id: u32) -> Option<u32> {
        self.node(id).parent
    }

    /// Node `id`'s children, in preorder (written order).
    pub fn children(&self, id: u32) -> &[u32] {
        let start = self.child_starts[id as usize] as usize;
        let end = self.child_starts[id as usize + 1] as usize;
        &self.children[start..end]
    }

    /// Node `id` and every node inside it, as a range of IDs (a subtree is contiguous in
    /// preorder).
    pub fn subtree(&self, id: u32) -> Range<u32> {
        id..self.ends[id as usize]
    }

    /// Whether `id` is `ancestor` or lies inside it.
    pub fn contains(&self, ancestor: u32, id: u32) -> bool {
        self.subtree(ancestor).contains(&id)
    }

    /// The definition node `id` belongs to: the nearest definition above it
    /// ([`unfold_core::owner`]). The module belongs to nothing.
    pub fn owner(&self, id: u32) -> Option<u32> {
        self.owners[id as usize]
    }

    /// The nodes that belong to definition `definition`, in preorder: its body's blocks and
    /// everything inside them down to (and including) nested definitions, but nothing inside a
    /// nested definition.
    pub fn owned(&self, definition: u32) -> impl Iterator<Item = u32> + '_ {
        self.subtree(definition)
            .skip(1)
            .filter(move |&id| self.owner(id) == Some(definition))
    }

    /// The links that start at node `id`, by `seq`.
    pub fn links(&self, id: u32) -> &[Link] {
        run(&self.graph.links, |link| link.from, id)
    }

    /// The `calls` link of call node `id`: its first link. `None` for any other node.
    pub fn calls_link(&self, id: u32) -> Option<&Link> {
        self.links(id).first().filter(|link| link.kind == LinkKind::Calls)
    }

    /// The signature of function node `id`.
    pub fn signature(&self, id: u32) -> Option<&Signature> {
        run(&self.graph.types.signatures, |item| item.node, id).first()
    }

    /// The parameters of function node `id`, by index.
    pub fn parameters(&self, id: u32) -> &[Parameter] {
        run(&self.graph.types.parameters, |item| item.node, id)
    }

    /// The written members of type or class node `id`, by index.
    pub fn members(&self, id: u32) -> &[Member] {
        run(&self.graph.types.members, |item| item.node, id)
    }

    /// The arguments of call node `id`, by index.
    pub fn arguments(&self, id: u32) -> &[Argument] {
        run(&self.graph.types.arguments, |item| item.call, id)
    }

    /// The `types` and `extends` links whose range lies inside `range`, in range order: the type
    /// names a written type names.
    pub fn type_links_in(&self, range: ByteRange) -> impl Iterator<Item = &Link> {
        let links = &self.graph.links;
        let start = self
            .type_links
            .partition_point(|&index| links[index as usize].range.start < range.start);
        self.type_links[start..]
            .iter()
            .map(|&index| &links[index as usize])
            .take_while(move |link| link.range.start < range.end)
            .filter(move |link| range.contains(link.range))
    }

    /// The source block `id` spans: a definition from its first decorator to its end, any
    /// other block its range. What a card shows and a board signs for a definition.
    pub fn extent(&self, id: u32) -> ByteRange {
        ByteRange {
            start: self.starts[id as usize],
            end: self.node(id).range.end,
        }
    }

    /// The statement call `id` is written in: its own `stmt`, or that of the nearest block above
    /// it that has one. A call in a test or a header is in that test or header, even in a control
    /// block that carries its whole statement (as TypeScript's do); a call that runs when a
    /// definition is created (a decorator, a base, a default) is its own statement. A part of a
    /// ternary or comprehension is inside an expression, so the climb goes on to the statement
    /// holding it.
    pub fn statement(&self, id: u32) -> ByteRange {
        let call = self.node(id).range;
        let parent = |id: u32| self.parent(id).map(|parent| self.node(parent));
        let mut at = Some(id);
        while let Some(current) = at {
            let node = self.node(current);
            // A control block or part written as a statement, not inside an expression.
            let control = match node.kind() {
                NodeKind::Decision | NodeKind::Loop => !is_expression(node.form),
                NodeKind::Try | NodeKind::With => true,
                NodeKind::Part => !parent(current).is_some_and(|parent| is_expression(parent.form)),
                NodeKind::Module
                | NodeKind::Class
                | NodeKind::Function
                | NodeKind::Type
                | NodeKind::Call
                | NodeKind::Exit => false,
            };
            if let Some(head) = node.head.filter(|head| control && head.contains(call)) {
                return head;
            }
            if let Some(stmt) = node.stmt {
                return stmt;
            }
            match node.kind() {
                NodeKind::Call | NodeKind::Exit => {}
                NodeKind::Function
                    if node.form == Form::Lambda
                        || parent(current).is_some_and(|parent| parent.kind() == NodeKind::Call) => {}
                NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Type => return call,
                NodeKind::Decision | NodeKind::Loop | NodeKind::Try | NodeKind::With | NodeKind::Part => {
                    if control {
                        return call;
                    }
                }
            }
            at = node.parent;
        }
        call
    }

    /// Node `id`'s catalog label ([`catalog::label`]).
    pub fn label(&self, id: u32) -> String {
        catalog::label(self.nodes(), self.path(), id)
    }

    /// Node `id`'s qualified name ([`catalog::qualified_name`]).
    pub fn qualified_name(&self, id: u32) -> String {
        catalog::qualified_name(self.nodes(), self.path(), id)
    }
}

/// A block found by key: its file's tree and its ID there.
#[derive(Debug, Clone, Copy)]
pub struct Block<'a> {
    /// The block's file.
    pub tree: &'a FileTree,
    /// The block's ID in its file.
    pub id: u32,
}

impl<'a> Block<'a> {
    /// The block's node.
    pub fn node(&self) -> &'a Node {
        self.tree.node(self.id)
    }

    /// The block's key.
    pub fn key(&self) -> &'a Key {
        self.tree.key(self.id)
    }

    /// The block's kind.
    pub fn kind(&self) -> NodeKind {
        self.tree.kind(self.id)
    }

    /// The definition the block belongs to.
    pub fn owner(&self) -> Option<Self> {
        self.tree.owner(self.id).map(|id| self.at(id))
    }

    /// Another block of the same file.
    pub fn at(&self, id: u32) -> Self {
        Self { tree: self.tree, id }
    }

    /// Whether `other` is this block or lies inside it.
    pub fn contains(&self, other: &Block<'_>) -> bool {
        std::ptr::eq(self.tree, other.tree) && self.tree.contains(self.id, other.id)
    }
}

/// Finds blocks by key.
pub trait Blocks {
    /// The block with `key`, if it is loaded.
    fn block(&self, key: &str) -> Option<Block<'_>>;
}

impl Blocks for FileTree {
    fn block(&self, key: &str) -> Option<Block<'_>> {
        self.id(key).map(|id| Block { tree: self, id })
    }
}

/// Several files' trees, with blocks found by key across them.
#[derive(Debug, Default)]
pub struct Trees {
    files: Vec<FileTree>,
    by_path: FxHashMap<RepoPath, usize>,
    by_key: FxHashMap<Key, (usize, u32)>,
}

impl Trees {
    /// No files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file's tree. A file added twice keeps the first copy.
    pub fn insert(&mut self, tree: FileTree) {
        if self.by_path.contains_key(tree.path()) {
            return;
        }
        let index = self.files.len();
        for (id, node) in tree.nodes().iter().enumerate() {
            self.by_key.insert(node.key.clone(), (index, to_u32(id)));
        }
        self.by_path.insert(tree.path().clone(), index);
        self.files.push(tree);
    }

    /// The tree of the file at `path`.
    pub fn get(&self, path: &str) -> Option<&FileTree> {
        self.by_path.get(path).map(|&index| &self.files[index])
    }

    /// Every file's tree, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &FileTree> {
        self.files.iter()
    }
}

impl Blocks for Trees {
    fn block(&self, key: &str) -> Option<Block<'_>> {
        self.by_key.get(key).map(|&(file, id)| Block {
            tree: &self.files[file],
            id,
        })
    }
}

#[cfg(test)]
mod tests {
    use unfold_core::testkit::sample;
    use unfold_core::vocab::Form;

    use super::*;

    fn service() -> FileTree {
        let graph = sample::shop().build();
        FileTree::new(graph.file("shop/service.py").unwrap().clone())
    }

    #[test]
    fn children_subtrees_and_owners_follow_the_records() {
        let tree = service();
        let place = tree.id("shop/service.py#place_order").unwrap();
        let try_block = tree.id("shop/service.py#place_order/try[0]").unwrap();
        let handler = tree.id("shop/service.py#place_order/try[0]/except[0]").unwrap();
        let mark = tree
            .id("shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed")
            .unwrap();
        let parts: Vec<Form> = tree
            .children(try_block)
            .iter()
            .map(|&id| tree.node(id).form)
            .collect();
        assert_eq!(parts, [Form::Body, Form::Except]);
        assert!(tree.contains(place, mark));
        assert!(tree.contains(handler, mark));
        assert!(!tree.contains(handler, try_block));
        assert_eq!(tree.owner(mark), Some(place));
        assert_eq!(tree.owner(place), Some(0));
        assert_eq!(tree.owner(0), None);
        assert!(tree.owned(place).all(|id| tree.contains(place, id)));
        assert!(tree.owned(place).any(|id| id == mark));
        assert_eq!(tree.parent(handler), Some(try_block));
    }

    #[test]
    fn links_and_type_names_are_found_by_node_and_range() {
        let tree = service();
        let charge = tree
            .id("shop/service.py#place_order/try[0]/body/call:charge_card")
            .unwrap();
        let link = tree.calls_link(charge).unwrap();
        assert_eq!(&*link.text, "charge_card");
        let handler = tree.id("shop/service.py#place_order/try[0]/except[0]").unwrap();
        assert!(tree.calls_link(handler).is_none());
        let clause = tree.node(handler).head.unwrap();
        let names: Vec<&str> = tree.type_links_in(clause).map(|link| &*link.text).collect();
        assert_eq!(names, ["PaymentDeclined"]);
    }

    #[test]
    fn trees_find_blocks_across_files() {
        let graph = sample::shop().build();
        let mut trees = Trees::new();
        for file in &graph.files {
            trees.insert(FileTree::new(FileGraph::clone(file)));
        }
        let block = trees.block("shop/payments.py#charge_card").unwrap();
        assert_eq!(block.tree.path().as_str(), "shop/payments.py");
        assert_eq!(block.kind(), NodeKind::Function);
        assert_eq!(block.owner().unwrap().key().as_str(), "shop/payments.py#");
        assert!(trees.block("shop/payments.py#absent").is_none());
        assert!(trees.get("shop/service.py").is_some());
    }
}
