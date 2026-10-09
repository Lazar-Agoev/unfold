//! Link-time conditional import blocks.
//!
//! Some `import` blocks exist only because of a fact about other files. Python's named child
//! import (`from pkg import child`) is an occurrence of its own only when `child` is proven absent
//! from `pkg`'s namespace, so the import machinery must load the submodule. Per-file extraction
//! cannot know that, so it selects every candidate tentatively and marks it conditional
//! ([`GraphFacts::with_conditional`]). At every link, the owning language says which candidates
//! the program keeps ([`Linked::keeps_import`](crate::Linked::keeps_import)); the build stage
//! drops the others with [`GraphFacts::drop_conditional`], which renumbers the file's blocks,
//! parents, slots, keys, and every fact and edge that names a local ID.
//!
//! The facts cache keeps the facts as extracted, so the decision is recomputed whenever the
//! program changes. Resolution runs over the facts as extracted; dropping a leaf `import` block
//! changes only the keys of later `import` siblings under the same parent, and an `import` block
//! is never an edge or entrance target, so every target key resolution produced is still valid.
//! Files without conditional blocks are untouched and cost nothing.

use unfold_core::vocab::BlockKind;
use unfold_core::{Block, Edge, FileGraph, FileRecord, KeyBuilder, RepoPath, Slot};

use super::{FactsError, GraphFacts};

/// One file's facts after dropping conditional blocks, with the local-ID mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Renumbered {
    /// The renumbered facts. Their kept conditional blocks are ordinary `import` blocks.
    pub graph: GraphFacts,
    /// The new local ID of each old local ID; `None` for a dropped block.
    map: Vec<Option<u32>>,
}

impl Renumbered {
    /// The new local ID of a block, or `None` when it was dropped.
    pub(crate) fn local(&self, old: u32) -> Option<u32> {
        self.map.get(old as usize).copied().flatten()
    }

    /// Edges resolved over the original facts, renumbered: edges whose source was dropped are
    /// removed, the others keep their order with their sources renumbered.
    pub(crate) fn edges(&self, edges: Vec<Edge>) -> Vec<Edge> {
        edges
            .into_iter()
            .filter_map(|mut edge| {
                edge.source = self.local(edge.source)?;
                Some(edge)
            })
            .collect()
    }
}

impl GraphFacts {
    /// Checks the conditional list: in range, sorted and distinct, each a leaf `import` block
    /// that is not the root. In preorder a block's first child directly follows it, so a block
    /// is a leaf exactly when the next block has another parent.
    pub(super) fn check_conditional(&self) -> Result<(), FactsError> {
        let mut previous = None;
        for &block in &self.conditional {
            let bad = || FactsError::Conditional { block };
            if previous.is_some_and(|previous| previous >= block) {
                return Err(bad());
            }
            previous = Some(block);
            let found = self.blocks.get(block as usize).ok_or_else(bad)?;
            if found.kind != BlockKind::Import || found.parent.is_none() {
                return Err(bad());
            }
            let next = self.blocks.get(block as usize + 1);
            if next.is_some_and(|next| next.parent == Some(block)) {
                return Err(bad());
            }
        }
        Ok(())
    }

    /// Drops the conditional blocks `keep` rejects and renumbers what is left. `keep` is asked
    /// once per conditional block, in block order. Returns `None` when every block is kept, so
    /// the facts are already the file's graph (their conditional blocks are then ordinary
    /// imports). `path` is the file's path, which keys start with.
    pub(crate) fn drop_conditional(
        &self,
        path: &RepoPath,
        mut keep: impl FnMut(u32) -> bool,
    ) -> Result<Option<Renumbered>, FactsError> {
        let dropped: Vec<u32> = self
            .conditional
            .iter()
            .copied()
            .filter(|&block| !keep(block))
            .collect();
        if dropped.is_empty() {
            return Ok(None);
        }
        let mut map = Vec::with_capacity(self.blocks.len());
        let mut next = 0u32;
        let mut pending = dropped.iter().peekable();
        for block in &self.blocks {
            if pending.next_if_eq(&&block.local_id).is_some() {
                map.push(None);
            } else {
                map.push(Some(next));
                next += 1;
            }
        }
        let renumber = |old: u32| map.get(old as usize).copied().flatten();
        let mut keys = KeyBuilder::new(path);
        let mut blocks = Vec::with_capacity(next as usize);
        for block in &self.blocks {
            let Some(local_id) = renumber(block.local_id) else {
                continue;
            };
            // Dropped blocks are leaves, so a kept block's parent is kept.
            let parent = match block.parent {
                Some(parent) => Some(renumber(parent).ok_or_else(|| {
                    FactsError::Renumber(format!("block {} lost its parent", block.local_id))
                })?),
                None => None,
            };
            let key = keys
                .push(parent, block.kind, block.name.as_deref())
                .map_err(|error| FactsError::Renumber(error.to_string()))?;
            blocks.push(Block {
                local_id,
                parent,
                key,
                ..block.clone()
            });
        }
        let slots = self
            .slots
            .iter()
            .filter_map(|slot| {
                Some(Slot {
                    block: renumber(slot.block)?,
                    ..*slot
                })
            })
            .collect();
        let mut values = self.values.clone();
        for value in &mut values {
            if let Some(block) = value.block {
                value.block = renumber(block);
            }
        }
        let mut calls = self.calls.clone();
        for call in &mut calls {
            call.block = renumber(call.block)
                .ok_or_else(|| FactsError::Renumber(format!("call block {} was dropped", call.block)))?;
        }
        let imports = self
            .imports
            .iter()
            .filter_map(|import| {
                let mut import = import.clone();
                import.block = renumber(import.block)?;
                Some(import)
            })
            .collect();
        let graph = GraphFacts {
            blocks,
            slots,
            values,
            calls,
            imports,
            conditional: Vec::new(),
        };
        graph.check()?;
        Ok(Some(Renumbered { graph, map }))
    }

    /// This file's graph record after linking: [`GraphFacts::file_graph`] with the conditional
    /// blocks `keep` rejects dropped and `edges` (resolved over these facts) renumbered.
    pub fn linked_file_graph(
        &self,
        file: FileRecord,
        edges: Vec<Edge>,
        keep: impl FnMut(u32) -> bool,
    ) -> Result<FileGraph, FactsError> {
        Ok(match self.drop_conditional(&file.path, keep)? {
            None => self.file_graph(file, edges),
            Some(renumbered) => {
                let edges = renumbered.edges(edges);
                let Renumbered { graph, .. } = renumbered;
                FileGraph {
                    file,
                    blocks: graph.blocks,
                    slots: graph.slots,
                    edges,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use unfold_core::text::{ByteRange, LineIndex};
    use unfold_core::vocab::{EdgeKind, Resolution, Role};

    use super::*;
    use crate::facts::{ImportFact, ValueFact, ValueShape};
    use crate::select::{MappedSlot, Mapping, Saved, Tree, walk};

    /// `file > [import > [import, import, import], function, import]`.
    struct Arena(Vec<(BlockKind, Vec<usize>)>);

    impl Mapping for Arena {
        type Node = usize;
        type Slot = ();

        fn range(&self, node: usize) -> ByteRange {
            ByteRange::new(node as u32, node as u32 + 1).unwrap()
        }

        fn kind(&self, node: usize) -> Option<BlockKind> {
            Some(self.0[node].0)
        }

        fn name(&self, _node: usize, kind: BlockKind) -> Option<Box<str>> {
            (kind == BlockKind::Function).then(|| "f".into())
        }

        fn role(&self, _parent: Saved<usize>, _slot: Option<()>, _decorator: bool) -> Option<Role> {
            Some(Role::Body)
        }

        fn slots(&self, node: usize, _kind: BlockKind, out: &mut Vec<MappedSlot>) {
            out.push(MappedSlot {
                slot: unfold_core::vocab::SlotName::Specifier,
                index: None,
                range: self.range(node),
            });
        }
    }

    impl Tree for Arena {
        fn children(&self, node: usize, out: &mut Vec<((), usize)>) {
            out.extend(self.0[node].1.iter().map(|&child| ((), child)));
        }
    }

    fn facts() -> GraphFacts {
        let arena = Arena(vec![
            (BlockKind::File, vec![1, 5, 6]),
            (BlockKind::Import, vec![2, 3, 4]),
            (BlockKind::Import, vec![]),
            (BlockKind::Import, vec![]),
            (BlockKind::Import, vec![]),
            (BlockKind::Function, vec![]),
            (BlockKind::Import, vec![]),
        ]);
        let path = RepoPath::new("a.x").unwrap();
        let selection = walk(&arena, &path, &LineIndex::new("0123456789"), 0, |_, _| {}).unwrap();
        let imports = [1, 2, 3, 4, 6]
            .into_iter()
            .map(|block| ImportFact {
                block,
                specifier: Some(format!("m{block}").into()),
            })
            .collect();
        let mut inline = ValueFact::new(ValueShape::InlineFunction, "(…)");
        inline.block = Some(5);
        GraphFacts::new(selection, vec![inline], Vec::new(), imports).unwrap()
    }

    fn edge(source: u32) -> Edge {
        Edge {
            source,
            seq: 0,
            kind: EdgeKind::Imports,
            target: None,
            text: format!("m{source}").into(),
            resolution: Resolution::Unresolved,
            reason: Some(unfold_core::vocab::Reason::NotFound),
            module: None,
            position: None,
        }
    }

    #[test]
    fn conditional_blocks_must_be_leaf_imports() {
        assert_eq!(
            facts().with_conditional(vec![1]),
            Err(FactsError::Conditional { block: 1 })
        );
        assert_eq!(
            facts().with_conditional(vec![5]),
            Err(FactsError::Conditional { block: 5 })
        );
        assert_eq!(
            facts().with_conditional(vec![0]),
            Err(FactsError::Conditional { block: 0 })
        );
        assert_eq!(
            facts().with_conditional(vec![2, 2]),
            Err(FactsError::Conditional { block: 2 })
        );
        let facts = facts().with_conditional(vec![4, 2, 6]).unwrap();
        assert_eq!(facts.conditional(), [2, 4, 6]);
        let file = crate::facts::FileFacts::new(facts, Vec::new());
        let decoded = crate::facts::FileFacts::from_bytes(&file.to_bytes().unwrap()).unwrap();
        assert_eq!(decoded, file);
    }

    #[test]
    fn dropping_renumbers_blocks_keys_slots_facts_and_edges() {
        let path = RepoPath::new("a.x").unwrap();
        let facts = facts().with_conditional(vec![2, 3, 6]).unwrap();
        assert_eq!(facts.drop_conditional(&path, |_| true).unwrap(), None);
        let mut asked = Vec::new();
        let renumbered = facts
            .drop_conditional(&path, |block| {
                asked.push(block);
                block == 3
            })
            .unwrap()
            .unwrap();
        assert_eq!(asked, [2, 3, 6]);
        let graph = &renumbered.graph;
        let keys: Vec<&str> = graph.blocks().iter().map(|block| block.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "a.x#",
                "a.x#import[0]",
                "a.x#import[0]/import[0]",
                "a.x#import[0]/import[1]",
                "a.x#function:f"
            ]
        );
        assert_eq!(
            graph
                .blocks()
                .iter()
                .map(|block| block.parent)
                .collect::<Vec<_>>(),
            [None, Some(0), Some(1), Some(1), Some(0)]
        );
        // The old `import[0]/import[2]` moved up to `import[0]/import[1]`.
        assert_eq!(graph.blocks()[3].range, facts.blocks()[4].range);
        assert_eq!(
            graph.slots().iter().map(|slot| slot.block).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert_eq!(
            graph
                .imports()
                .iter()
                .map(|import| import.block)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(graph.values()[0].block, Some(4));
        assert!(graph.conditional().is_empty());
        assert_eq!(renumbered.local(2), None);
        assert_eq!(renumbered.local(5), Some(4));
        let edges = renumbered.edges([1, 2, 3, 4, 6].into_iter().map(edge).collect());
        assert_eq!(
            edges
                .iter()
                .map(|edge| (edge.source, &*edge.text))
                .collect::<Vec<_>>(),
            [(1, "m1"), (2, "m3"), (3, "m4")]
        );
    }
}
