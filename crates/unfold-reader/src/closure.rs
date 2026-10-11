//! Activation closures (docs/reader.md#derived-views): the blocks statically reached at least from a set
//! of seeds. It is a conservative static set, shown as "reached at least from", never a claim
//! about what runs.
//!
//! ```text
//! closure(seeds):
//!   visit each unit once, starting from the seeds
//!   a branch seed (`if`, `case`, `default`) considers its owner's occurrences inside the branch;
//!   any other unit considers the occurrences it owns
//!   for each resolved edge among them:
//!     calls, imports, references -> add the target
//!     constructs                 -> add the target and its nearest enclosing class
//!   a file or callable also adds the classes it owns
//! ```
//!
//! External and unresolved edges stop the walk. Every branch of the code contributes. The
//! closure of a seed set is the union of its seeds' closures, so callers may cache per seed.

use unfold_core::vocab::{BlockKind, EdgeKind};

use crate::index::{BlockId, GraphIndex};

/// Whether `kind` is a branch that a manual entrance may target for explicit CLI dispatch.
pub(crate) fn is_branch(kind: BlockKind) -> bool {
    matches!(kind, BlockKind::If | BlockKind::Case | BlockKind::Default)
}

/// A fixed-size set of block numbers.
struct BlockSet(Vec<u64>);

impl BlockSet {
    fn new(len: usize) -> Self {
        Self(vec![0; len.div_ceil(64)])
    }

    /// Inserts `id`; returns whether it was absent.
    fn insert(&mut self, id: BlockId) -> bool {
        let (word, bit) = ((id / 64) as usize, id % 64);
        let Some(slot) = self.0.get_mut(word) else {
            return false;
        };
        let absent = *slot & (1 << bit) == 0;
        *slot |= 1 << bit;
        absent
    }
}

/// The closure of `seeds`, sorted by block number. Seeds outside the index are ignored.
pub(crate) fn closure(index: &GraphIndex, seeds: &[BlockId]) -> Vec<BlockId> {
    let mut found = BlockSet::new(index.len());
    let mut reached = Vec::new();
    let mut queue: Vec<BlockId> = seeds
        .iter()
        .copied()
        .filter(|&id| (id as usize) < index.len())
        .collect();
    while let Some(unit) = queue.pop() {
        if !found.insert(unit) {
            continue;
        }
        reached.push(unit);
        let kind = index.kind(unit);
        let branch = is_branch(kind);
        let scope = if branch { index.owner(unit) } else { unit };
        for edge in index.owned_edges(scope) {
            let Some(target) = edge.resolved_target() else {
                continue;
            };
            if branch && !index.within(edge.source, unit) {
                continue;
            }
            queue.push(target);
            if edge.kind == EdgeKind::Constructs
                && let Some(class) = index.enclosing_class(target)
            {
                queue.push(class);
            }
        }
        if kind == BlockKind::File || kind.is_function_like() {
            queue.extend_from_slice(index.owned_classes(unit));
        }
    }
    reached.sort_unstable();
    reached
}

/// The union of sorted, deduplicated block lists, sorted and deduplicated.
pub(crate) fn union(lists: &[&[BlockId]]) -> Vec<BlockId> {
    let mut all: Vec<BlockId> = lists.iter().flat_map(|list| list.iter().copied()).collect();
    all.sort_unstable();
    all.dedup();
    all
}
