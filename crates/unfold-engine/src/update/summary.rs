//! The change summary that `graph_updated` carries: which files, structural keys, and entrances
//! differ between the previous graph and the new one, so each window refreshes only the cards
//! that depend on them.
//!
//! This is not the store's delta: the workspace builds the store's `Update` from the same two
//! graphs with `FileChange::between` and `EntranceChanges::between`. The summary only names
//! what changed.

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::{Block, Edge, Entrance, FileGraph, Graph, Key, RepoPath, Slot};

/// What differs between two graphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    /// Files added, or whose record, blocks, slots, or edges changed, in path order.
    pub changed_files: Vec<RepoPath>,
    /// Files no longer indexed, in path order.
    pub removed_files: Vec<RepoPath>,
    /// Block keys added, removed, or whose block, slots, or outgoing edges changed, sorted.
    pub changed_keys: Vec<Key>,
    /// Entrance keys added, removed, or whose entrance changed, sorted.
    pub changed_entrances: Vec<Key>,
    /// Whether the input set or any input's fingerprint changed.
    pub inputs_changed: bool,
}

impl ChangeSummary {
    /// The differences from `previous` to `current`. Both must be normalized.
    ///
    /// Files are compared in parallel (on the current rayon pool); the result doesn't depend on
    /// the thread count.
    pub fn between(previous: &Graph, current: &Graph) -> Self {
        let mut summary = Self {
            inputs_changed: previous.inputs != current.inputs,
            ..Self::default()
        };
        // Pair the files by path (both lists are in path order), then compare each pair.
        let mut pairs: Vec<(Option<&FileGraph>, Option<&FileGraph>)> = Vec::new();
        let mut old = previous.files.iter().peekable();
        for file in &current.files {
            while let Some(gone) = old.next_if(|old| old.file.path < file.file.path) {
                pairs.push((Some(gone), None));
            }
            pairs.push((old.next_if(|old| old.file.path == file.file.path), Some(file)));
        }
        pairs.extend(old.map(|gone| (Some(gone), None)));
        // For each pair: whether the file changed, and the keys that did.
        let compared: Vec<(bool, Vec<Key>)> = pairs
            .par_iter()
            .map(|&pair| match pair {
                (Some(before), Some(after)) if before == after => (false, Vec::new()),
                (Some(before), Some(after)) => {
                    let mut keys = Vec::new();
                    changed_keys(before, after, &mut keys);
                    (true, keys)
                }
                (Some(file), None) | (None, Some(file)) => {
                    (true, file.blocks.iter().map(|block| block.key.clone()).collect())
                }
                (None, None) => (false, Vec::new()),
            })
            .collect();
        for (&(before, after), (changed, keys)) in pairs.iter().zip(compared) {
            match (before, after) {
                (Some(gone), None) => summary.removed_files.push(gone.file.path.clone()),
                (_, Some(file)) if changed => summary.changed_files.push(file.file.path.clone()),
                _ => {}
            }
            summary.changed_keys.extend(keys);
        }
        summary.changed_keys.sort();
        summary.changed_keys.dedup();
        summary.changed_entrances = changed_entrances(&previous.entrances, &current.entrances);
        summary
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.changed_files.is_empty()
            && self.removed_files.is_empty()
            && self.changed_entrances.is_empty()
            && !self.inputs_changed
    }
}

/// A block as cards see it: everything but its file-local numbering.
struct BlockView<'g> {
    block: &'g Block,
    parent: Option<&'g Key>,
    slots: &'g [Slot],
    edges: &'g [Edge],
}

impl BlockView<'_> {
    fn same_as(&self, other: &Self) -> bool {
        let (a, b) = (self.block, other.block);
        let same_block = a.kind == b.kind
            && a.role == b.role
            && a.name == b.name
            && a.range == b.range
            && a.lines == b.lines;
        let same_slot = |a: &Slot, b: &Slot| a.slot == b.slot && a.index == b.index && a.range == b.range;
        let same_edge = |a: &Edge, b: &Edge| {
            a.seq == b.seq
                && a.kind == b.kind
                && a.target == b.target
                && a.text == b.text
                && a.resolution == b.resolution
                && a.reason == b.reason
                && a.module == b.module
                && a.position == b.position
        };
        same_block
            && self.parent == other.parent
            && self.slots.len() == other.slots.len()
            && self.slots.iter().zip(other.slots).all(|(a, b)| same_slot(a, b))
            && self.edges.len() == other.edges.len()
            && self.edges.iter().zip(other.edges).all(|(a, b)| same_edge(a, b))
    }
}

/// Each block of `file` by key, with its slots and outgoing edges.
fn views(file: &FileGraph) -> FxHashMap<&Key, BlockView<'_>> {
    let mut views = FxHashMap::default();
    let (mut slot, mut edge) = (0usize, 0usize);
    for block in &file.blocks {
        let slots_start = slot;
        while file
            .slots
            .get(slot)
            .is_some_and(|found| found.block == block.local_id)
        {
            slot += 1;
        }
        let edges_start = edge;
        while file
            .edges
            .get(edge)
            .is_some_and(|found| found.source == block.local_id)
        {
            edge += 1;
        }
        let parent = block
            .parent
            .and_then(|parent| file.blocks.get(parent as usize))
            .map(|parent| &parent.key);
        views.insert(
            &block.key,
            BlockView {
                block,
                parent,
                slots: &file.slots[slots_start..slot],
                edges: &file.edges[edges_start..edge],
            },
        );
    }
    views
}

fn changed_keys(before: &FileGraph, after: &FileGraph, out: &mut Vec<Key>) {
    let old = views(before);
    let new = views(after);
    for (key, view) in &new {
        if !old.get(key).is_some_and(|old| old.same_as(view)) {
            out.push((*key).clone());
        }
    }
    out.extend(
        old.keys()
            .filter(|key| !new.contains_key(*key))
            .map(|key| (*key).clone()),
    );
}

fn changed_entrances(previous: &[Entrance], current: &[Entrance]) -> Vec<Key> {
    let old: FxHashMap<&Key, &Entrance> = previous
        .iter()
        .map(|entrance| (&entrance.key, entrance))
        .collect();
    let new: FxHashMap<&Key, &Entrance> = current.iter().map(|entrance| (&entrance.key, entrance)).collect();
    let mut keys: Vec<Key> = new
        .iter()
        .filter(|(key, entrance)| old.get(*key) != Some(*entrance))
        .map(|(key, _)| (*key).clone())
        .chain(
            old.keys()
                .filter(|key| !new.contains_key(*key))
                .map(|key| (*key).clone()),
        )
        .collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use unfold_core::testkit::sample::sample;

    use super::*;

    #[test]
    fn identical_graphs_have_no_changes() {
        let graph = sample().build();
        assert!(ChangeSummary::between(&graph, &graph).is_empty());
    }

    #[test]
    fn names_changed_files_keys_and_entrances() {
        let before = sample().build();
        let mut after = before.clone();
        let file = &mut after.files[0];
        let path = file.file.path.clone();
        let renamed = file.blocks.len() - 1;
        file.blocks[renamed].name = Some("renamed".into());
        let key = file.blocks[renamed].key.clone();
        let removed = after.files.pop().map(|file| file.file.path);
        let entrance = after.entrances.first().map(|entrance| entrance.key.clone());
        after.entrances.clear();
        let summary = ChangeSummary::between(&before, &after);
        assert_eq!(summary.changed_files, [path]);
        assert!(summary.changed_keys.contains(&key));
        assert_eq!(summary.removed_files.first().cloned(), removed);
        if let Some(entrance) = entrance {
            assert!(summary.changed_entrances.contains(&entrance));
        }
        assert!(!summary.inputs_changed);
    }
}
