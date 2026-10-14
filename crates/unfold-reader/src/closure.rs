//! Closures: the definitions reached at least from a set of seeds, following what each
//! definition reaches ([`crate::index`]). It is a conservative static set, shown as "reached at
//! least from", never a claim about what runs. The closure of a seed set is the union of its
//! seeds' closures, so callers cache per seed.

use crate::index::{GraphIndex, NodeId};

/// A fixed-size set of node numbers.
struct NodeSet(Vec<u64>);

impl NodeSet {
    fn new(len: usize) -> Self {
        Self(vec![0; len.div_ceil(64)])
    }

    /// Inserts `id`; returns whether it was absent.
    fn insert(&mut self, id: NodeId) -> bool {
        let (word, bit) = ((id / 64) as usize, id % 64);
        let Some(slot) = self.0.get_mut(word) else {
            return false;
        };
        let absent = *slot & (1 << bit) == 0;
        *slot |= 1 << bit;
        absent
    }
}

/// The closure of `seeds`, sorted by node number. Seeds outside the index are ignored; a seed
/// that is not a definition stands for the definition it belongs to.
pub(crate) fn closure(index: &GraphIndex, seeds: &[NodeId]) -> Vec<NodeId> {
    let mut found = NodeSet::new(index.len());
    let mut reached = Vec::new();
    let mut queue: Vec<NodeId> = seeds
        .iter()
        .filter(|&&id| (id as usize) < index.len())
        .filter_map(|&id| index.definition(id))
        .collect();
    while let Some(definition) = queue.pop() {
        if !found.insert(definition) {
            continue;
        }
        reached.push(definition);
        queue.extend_from_slice(index.reaches(definition));
    }
    reached.sort_unstable();
    reached
}
