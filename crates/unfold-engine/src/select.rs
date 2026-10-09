//! Shared, deterministic block selection: which syntax becomes blocks, their parents, roles,
//! names, slots, structural keys, and line spans.
//!
//! A language supplies a [`Mapping`] over its own syntax nodes (which kind a node selects,
//! which subtrees to skip, which nodes are decorators, names, roles, and slot ranges) and drives
//! a [`Selector`] through its own walk: [`Selector::enter`] when it reaches a node,
//! [`Selector::exit`] when it leaves one it entered. Languages whose syntax can list a node's
//! children can implement [`Tree`] and call [`walk`] instead, which drives the selector
//! iteratively, so deeply nested code cannot overflow the stack.
//!
//! The selector applies the rules every language shares:
//!
//! - Local IDs are assigned in preorder, the root (`file`) first; the root spans the whole file.
//! - A block's parent is its nearest selected ancestor.
//! - Its role comes from the mapping, given the parent and the first child slot on the path that
//!   leaves the parent (across unselected wrappers), or `decorator`-ness when that path passes
//!   through a decorator. Selecting a block resets both for its descendants.
//! - Keys come from `unfold_core::KeyBuilder`; lines from the file's `LineIndex`.
//! - Slots are sorted by start within their block.

use unfold_core::key::KeyError;
use unfold_core::text::{ByteRange, LineIndex};
use unfold_core::vocab::{BlockKind, Role, SlotName};
use unfold_core::{Block, KeyBuilder, RepoPath, Slot};

/// The deepest nesting of selected blocks a file may have. Keys grow with depth, so deeper
/// files would take quadratic space; real code stays far below this.
pub const MAX_BLOCK_DEPTH: u32 = 1024;

/// A slot range reported by a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedSlot {
    /// Which written part.
    pub slot: SlotName,
    /// The zero-based index of a repeated part.
    pub index: Option<u32>,
    /// The part's token-exact range.
    pub range: ByteRange,
}

/// The nearest selected ancestor of a node, as the mapping sees it when assigning a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved<N> {
    /// The ancestor block's local ID.
    pub local: u32,
    /// Its kind.
    pub kind: BlockKind,
    /// Its syntax node.
    pub node: N,
}

/// A language's graph mapping over its own syntax (the proof of concept's `GraphMapping`).
///
/// Nodes are cheap handles (`Copy`), such as references into the language's tree or arena
/// indices. Slot labels are the language's own child labels; the selector only carries them
/// back to [`Mapping::role`] and never interprets them.
pub trait Mapping {
    /// A syntax node handle.
    type Node: Copy;
    /// A child-slot label.
    type Slot: Copy + Eq;

    /// The node's token-exact byte range. Ignored for the root, which spans the whole file.
    fn range(&self, node: Self::Node) -> ByteRange;

    /// Whether to drop the node and its whole subtree (for example, type syntax).
    fn skip(&self, _node: Self::Node) -> bool {
        false
    }

    /// Whether the node is a decorator: blocks reached through it get the `decorator` role
    /// (in languages whose [`Mapping::role`] says so).
    fn decorator(&self, _node: Self::Node) -> bool {
        false
    }

    /// The block kind the node selects, if any. Exactly the root selects `file`.
    fn kind(&self, node: Self::Node) -> Option<BlockKind>;

    /// The block's name, from syntax only; `None` or empty for an unnamed block.
    fn name(&self, node: Self::Node, kind: BlockKind) -> Option<Box<str>>;

    /// The role of a block under `parent`, given the first child slot on the path leaving the
    /// parent (`None` when the block is reached without passing a child slot) and whether that
    /// path passes through a decorator. `None` for an unmapped relationship.
    fn role(
        &self,
        parent: Saved<Self::Node>,
        first_slot: Option<Self::Slot>,
        through_decorator: bool,
    ) -> Option<Role>;

    /// The block's written parts, in any order.
    fn slots(&self, node: Self::Node, kind: BlockKind, out: &mut Vec<MappedSlot>);
}

/// A mapping whose nodes can list their children, so [`walk`] can drive the selection.
pub trait Tree: Mapping {
    /// Appends the node's children with their slot labels, in source order.
    fn children(&self, node: Self::Node, out: &mut Vec<(Self::Slot, Self::Node)>);
}

/// What the walk should do after [`Selector::enter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit {
    /// The node is skipped: do not visit its children, and do not call [`Selector::exit`].
    Skip,
    /// The node was entered: visit its children, then call [`Selector::exit`].
    Enter {
        /// The local ID of the block the node selected, if it selected one.
        block: Option<u32>,
    },
}

/// Why a selection failed. Every case is a language bug or a pathological file, reported as a
/// failure instead of a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectError {
    /// The blocks do not form a keyed preorder tree (for example, the root is not a `file`).
    #[error("invalid block tree: {0}")]
    Key(#[from] KeyError),
    /// Selected blocks nest deeper than [`MAX_BLOCK_DEPTH`].
    #[error("blocks nest more than {MAX_BLOCK_DEPTH} deep")]
    TooDeep,
    /// `exit` did not match an `enter`, or the walk ended with nodes still open.
    #[error("enter and exit calls are unbalanced")]
    Unbalanced,
    /// No root block was selected.
    #[error("no file root was selected")]
    MissingRoot,
}

/// One file's selected blocks and slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub(crate) blocks: Vec<Block>,
    pub(crate) slots: Vec<Slot>,
}

impl Selection {
    /// Blocks in preorder; `blocks[i].local_id == i`.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Slots ordered by block, then start.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }
}

/// The state a node passes to its children.
#[derive(Debug, Clone, Copy)]
struct Frame<N, S> {
    saved: Option<Saved<N>>,
    first_slot: Option<S>,
    through_decorator: bool,
    depth: u32,
}

/// Selects one file's blocks while the language walks its syntax tree.
#[derive(Debug)]
pub struct Selector<'l, N, S> {
    keys: KeyBuilder,
    lines: &'l LineIndex,
    blocks: Vec<Block>,
    slots: Vec<Slot>,
    frames: Vec<Frame<N, S>>,
    scratch: Vec<MappedSlot>,
}

impl<'l, N: Copy, S: Copy + Eq> Selector<'l, N, S> {
    /// Starts selecting the file at `path`, whose text `lines` indexes.
    pub fn new(path: &RepoPath, lines: &'l LineIndex) -> Self {
        Self {
            keys: KeyBuilder::new(path),
            lines,
            blocks: Vec::new(),
            slots: Vec::new(),
            frames: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Enters `node`, reached from its syntactic parent through child slot `slot` (`None` for
    /// the root). Selects a block if the mapping says so.
    pub fn enter<M>(&mut self, mapping: &M, slot: Option<S>, node: N) -> Result<Visit, SelectError>
    where
        M: Mapping<Node = N, Slot = S> + ?Sized,
    {
        if mapping.skip(node) {
            return Ok(Visit::Skip);
        }
        let parent = self.frames.last().copied().unwrap_or(Frame {
            saved: None,
            first_slot: None,
            through_decorator: false,
            depth: 0,
        });
        let first_slot = parent.first_slot.or(slot);
        let through_decorator = parent.through_decorator || mapping.decorator(node);
        let Some(kind) = mapping.kind(node) else {
            self.frames.push(Frame {
                first_slot,
                through_decorator,
                ..parent
            });
            return Ok(Visit::Enter { block: None });
        };
        let depth = parent.depth + 1;
        if depth > MAX_BLOCK_DEPTH {
            return Err(SelectError::TooDeep);
        }
        let name = mapping.name(node, kind).filter(|name| !name.is_empty());
        let parent_local = parent.saved.map(|saved| saved.local);
        let key = self.keys.push(parent_local, kind, name.as_deref())?;
        let local = self.blocks.len() as u32;
        let range = match parent.saved {
            None => ByteRange {
                start: 0,
                end: self.lines.len(),
            },
            Some(_) => mapping.range(node),
        };
        let role = parent
            .saved
            .and_then(|saved| mapping.role(saved, first_slot, through_decorator));
        self.blocks.push(Block {
            local_id: local,
            parent: parent_local,
            kind,
            role,
            name,
            range,
            lines: self.lines.span(range),
            key,
        });
        self.scratch.clear();
        mapping.slots(node, kind, &mut self.scratch);
        self.scratch
            .sort_by_key(|slot| (slot.range.start, slot.range.end, slot.slot, slot.index));
        self.slots.extend(self.scratch.iter().map(|slot| Slot {
            block: local,
            slot: slot.slot,
            index: slot.index,
            range: slot.range,
        }));
        self.frames.push(Frame {
            saved: Some(Saved { local, kind, node }),
            first_slot: None,
            through_decorator: false,
            depth,
        });
        Ok(Visit::Enter { block: Some(local) })
    }

    /// Leaves the most recently entered node.
    pub fn exit(&mut self) -> Result<(), SelectError> {
        self.frames.pop().map(|_| ()).ok_or(SelectError::Unbalanced)
    }

    /// The local ID of the nearest selected block enclosing the current position.
    pub fn current_block(&self) -> Option<u32> {
        self.frames.last()?.saved.map(|saved| saved.local)
    }

    /// The blocks selected so far.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Finishes the file. Every entered node must have been exited.
    pub fn finish(self) -> Result<Selection, SelectError> {
        if !self.frames.is_empty() {
            return Err(SelectError::Unbalanced);
        }
        if self.blocks.is_empty() {
            return Err(SelectError::MissingRoot);
        }
        Ok(Selection {
            blocks: self.blocks,
            slots: self.slots,
        })
    }
}

/// Selects the blocks of the tree under `root` by walking it iteratively in preorder, calling
/// `selected(node, local_id)` for every node that selects a block (in local-ID order), so the
/// language can map its nodes to blocks when recording calls and imports.
pub fn walk<T, F>(
    tree: &T,
    path: &RepoPath,
    lines: &LineIndex,
    root: T::Node,
    mut selected: F,
) -> Result<Selection, SelectError>
where
    T: Tree + ?Sized,
    F: FnMut(T::Node, u32),
{
    enum Step<N, S> {
        Enter(Option<S>, N),
        Exit,
    }
    let mut selector = Selector::new(path, lines);
    let mut stack = vec![Step::Enter(None, root)];
    let mut children = Vec::new();
    while let Some(step) = stack.pop() {
        match step {
            Step::Exit => selector.exit()?,
            Step::Enter(slot, node) => {
                let Visit::Enter { block } = selector.enter(tree, slot, node)? else {
                    continue;
                };
                if let Some(local) = block {
                    selected(node, local);
                }
                stack.push(Step::Exit);
                children.clear();
                tree.children(node, &mut children);
                stack.extend(
                    children
                        .drain(..)
                        .rev()
                        .map(|(slot, child)| Step::Enter(Some(slot), child)),
                );
            }
        }
    }
    selector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal arena node: kind, name, range, whether it is a decorator, and children with
    /// string slots.
    type Node = (
        Option<BlockKind>,
        &'static str,
        ByteRange,
        bool,
        Vec<(&'static str, usize)>,
    );

    struct Arena {
        nodes: Vec<Node>,
    }

    impl Mapping for Arena {
        type Node = usize;
        type Slot = &'static str;

        fn range(&self, node: usize) -> ByteRange {
            self.nodes[node].2
        }

        fn decorator(&self, node: usize) -> bool {
            self.nodes[node].3
        }

        fn kind(&self, node: usize) -> Option<BlockKind> {
            self.nodes[node].0
        }

        fn name(&self, node: usize, _kind: BlockKind) -> Option<Box<str>> {
            Some(self.nodes[node].1.into())
        }

        fn role(
            &self,
            _parent: Saved<usize>,
            first_slot: Option<&'static str>,
            through: bool,
        ) -> Option<Role> {
            if through {
                return Some(Role::Decorator);
            }
            match first_slot? {
                "body" => Some(Role::Body),
                "test" => Some(Role::Condition),
                _ => None,
            }
        }

        fn slots(&self, _node: usize, _kind: BlockKind, _out: &mut Vec<MappedSlot>) {}
    }

    impl Tree for Arena {
        fn children(&self, node: usize, out: &mut Vec<(&'static str, usize)>) {
            out.extend(self.nodes[node].4.iter().copied());
        }
    }

    fn range(start: u32, end: u32) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    #[test]
    fn roles_follow_the_first_slot_across_wrappers() {
        // file > wrapper(body) > if(test-slot ignored: first slot is body) ...
        let arena = Arena {
            nodes: vec![
                (Some(BlockKind::File), "", range(0, 0), false, vec![("body", 1)]),
                (None, "", range(0, 9), false, vec![("test", 2), ("odd", 3)]),
                (Some(BlockKind::If), "", range(0, 9), false, vec![("test", 4)]),
                (None, "", range(2, 3), true, vec![("value", 5)]),
                (Some(BlockKind::Call), "", range(3, 5), false, vec![]),
                (Some(BlockKind::Call), "", range(2, 3), false, vec![]),
            ],
        };
        let lines = LineIndex::new("if (a) b;");
        let mut order = Vec::new();
        let selection = walk(
            &arena,
            &RepoPath::new("a.alien").unwrap(),
            &lines,
            0,
            |node, local| {
                order.push((node, local));
            },
        )
        .unwrap();
        assert_eq!(order, [(0, 0), (2, 1), (4, 2), (5, 3)]);
        let summary: Vec<_> = selection
            .blocks()
            .iter()
            .map(|block| (block.kind, block.parent, block.role, block.key.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (BlockKind::File, None, None, "a.alien#"),
                (BlockKind::If, Some(0), Some(Role::Body), "a.alien#if[0]"),
                (
                    BlockKind::Call,
                    Some(1),
                    Some(Role::Condition),
                    "a.alien#if[0]/call[0]"
                ),
                (BlockKind::Call, Some(0), Some(Role::Decorator), "a.alien#call[0]"),
            ]
        );
        assert_eq!(selection.blocks()[0].range, range(0, 9));
    }

    #[test]
    fn deep_nesting_is_reported_not_overflowed() {
        let depth = MAX_BLOCK_DEPTH as usize + 5;
        let mut nodes = vec![(Some(BlockKind::File), "", range(0, 1), false, vec![("body", 1)])];
        for index in 1..=depth {
            let children = if index < depth {
                vec![("body", index + 1)]
            } else {
                vec![]
            };
            nodes.push((Some(BlockKind::If), "", range(0, 1), false, children));
        }
        let arena = Arena { nodes };
        let lines = LineIndex::new("x");
        let result = walk(
            &arena,
            &RepoPath::new("deep.alien").unwrap(),
            &lines,
            0,
            |_, _| {},
        );
        assert_eq!(result, Err(SelectError::TooDeep));
    }

    #[test]
    fn unbalanced_and_rootless_selections_fail() {
        let arena = Arena {
            nodes: vec![(Some(BlockKind::If), "", range(0, 1), false, vec![])],
        };
        let lines = LineIndex::new("x");
        let path = RepoPath::new("a.alien").unwrap();
        assert!(matches!(
            walk(&arena, &path, &lines, 0, |_, _| {}),
            Err(SelectError::Key(_))
        ));
        let mut selector: Selector<'_, usize, &str> = Selector::new(&path, &lines);
        assert_eq!(selector.exit(), Err(SelectError::Unbalanced));
        let selector: Selector<'_, usize, &str> = Selector::new(&path, &lines);
        assert_eq!(selector.finish(), Err(SelectError::MissingRoot));
    }
}
