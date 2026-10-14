//! The change summary that `graph_updated` carries: which files and structural keys differ
//! between the previous graph and the new one, so the window refreshes only the cards that
//! depend on them.
//!
//! This is not the store's delta: the store builds its update from the same two graphs. The
//! summary only names what changed.

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::vocab::InputRole;
use unfold_core::{FileGraph, Graph, Input, Key, Link, Node, RepoPath};

/// What differs between two graphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    /// Files added, or whose record, nodes, links, or typed records changed, in path order.
    pub changed_files: Vec<RepoPath>,
    /// Files no longer indexed, in path order.
    pub removed_files: Vec<RepoPath>,
    /// Node keys added, removed, or whose node or outgoing links changed, sorted.
    pub changed_keys: Vec<Key>,
    /// Whether the set of configuration inputs and evidence files, or any one's fingerprint,
    /// changed. The settings file's record doesn't count: what its compile settings change
    /// shows in the files, and its apps in `apps_changed`.
    pub inputs_changed: bool,
    /// Whether the apps, their entrances, or the states of their code changed.
    pub apps_changed: bool,
}

impl ChangeSummary {
    /// The differences from `previous` to `current`. Both must be normalized.
    ///
    /// Files are compared in parallel (on the current rayon pool); the result doesn't depend on
    /// the thread count. A file graph both graphs share is the same at once.
    pub fn between(previous: &Graph, current: &Graph) -> Self {
        let mut summary = Self {
            inputs_changed: !reported(&previous.inputs).eq(reported(&current.inputs)),
            apps_changed: previous.apps != current.apps,
            ..Self::default()
        };
        // Pair the files by path (both lists are in path order), then compare each pair.
        let mut pairs = Vec::new();
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
                    (true, file.nodes.iter().map(|node| node.key.clone()).collect())
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
        summary
    }

    /// Whether the files or the inputs changed: what `graph_updated` reports. Apps that changed
    /// alone aren't reported there: windows refresh entrances on `entrances_changed`.
    pub fn graph_changed(&self) -> bool {
        !self.changed_files.is_empty() || !self.removed_files.is_empty() || self.inputs_changed
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        !self.graph_changed() && !self.apps_changed
    }
}

/// The inputs a summary reports: all but the settings file.
fn reported(inputs: &[Input]) -> impl Iterator<Item = &Input> {
    inputs
        .iter()
        .filter(|input| input.role != InputRole::UnfoldConfig)
}

/// A node as cards see it: everything but its file-local numbering, whose references become
/// keys.
struct NodeView<'g> {
    node: &'g Node,
    parent: Option<&'g Key>,
    r#for: Option<&'g Key>,
    target: Option<&'g Key>,
    links: &'g [Link],
}

impl NodeView<'_> {
    fn same_as(&self, other: &Self) -> bool {
        let (a, b) = (self.node, other.node);
        let same_node = a.form == b.form
            && a.name == b.name
            && a.range == b.range
            && a.lines == b.lines
            && a.head == b.head
            && a.body == b.body
            && a.doc == b.doc
            && a.stmt == b.stmt
            && a.value == b.value
            && a.binds == b.binds
            && a.role == b.role
            && a.flags == b.flags;
        let same_link = |a: &Link, b: &Link| {
            a.seq == b.seq
                && a.kind == b.kind
                && a.range == b.range
                && a.text == b.text
                && a.to == b.to
                && a.implements == b.implements
                && a.bound == b.bound
        };
        same_node
            && self.parent == other.parent
            && self.r#for == other.r#for
            && self.target == other.target
            && self.links.len() == other.links.len()
            && self.links.iter().zip(other.links).all(|(a, b)| same_link(a, b))
    }
}

/// Each node of `file` by key, with its outgoing links.
fn views(file: &FileGraph) -> FxHashMap<&Key, NodeView<'_>> {
    let key = |id: Option<u32>| {
        id.and_then(|id| file.nodes.get(id as usize))
            .map(|node| &node.key)
    };
    let mut views = FxHashMap::default();
    let mut link = 0usize;
    for (id, node) in file.nodes.iter().enumerate() {
        let start = link;
        while file
            .links
            .get(link)
            .is_some_and(|found| found.from as usize == id)
        {
            link += 1;
        }
        views.insert(
            &node.key,
            NodeView {
                node,
                parent: key(node.parent),
                r#for: key(node.r#for),
                target: key(node.target),
                links: &file.links[start..link],
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use unfold_core::testkit::sample;

    use super::*;

    #[test]
    fn identical_graphs_have_no_changes() {
        let graph = sample::shop().build();
        assert!(ChangeSummary::between(&graph, &graph).is_empty());
    }

    #[test]
    fn names_changed_files_and_keys() {
        let before = sample::shop().build();
        let mut after = before.clone();
        let file = Arc::make_mut(&mut after.files[0]);
        let path = file.file.path.clone();
        let renamed = file.nodes.len() - 1;
        file.nodes[renamed].binds = Some("renamed".into());
        let key = file.nodes[renamed].key.clone();
        let removed = after.files.pop().map(|file| file.file.path.clone());
        let summary = ChangeSummary::between(&before, &after);
        assert_eq!(summary.changed_files, [path]);
        assert!(summary.changed_keys.contains(&key));
        assert_eq!(summary.removed_files.first().cloned(), removed);
        assert!(!summary.inputs_changed);
    }

    #[test]
    fn the_settings_file_and_the_apps_alone_change_no_file_or_input() {
        let before = sample::shop().build();
        let mut after = before.clone();
        after.inputs.push(Input {
            path: RepoPath::new("unfold.toml").unwrap(),
            role: InputRole::UnfoldConfig,
            hash: unfold_core::ContentHash::of(b"[[app]]\n"),
        });
        after.normalize();
        after.apps[0].name = "Shop!".into();
        let summary = ChangeSummary::between(&before, &after);
        assert!(summary.apps_changed && !summary.graph_changed(), "{summary:?}");

        after.inputs.push(Input {
            path: RepoPath::new("schema.sql").unwrap(),
            role: InputRole::Evidence,
            hash: unfold_core::ContentHash::of(b"create table orders ();\n"),
        });
        after.normalize();
        assert!(ChangeSummary::between(&before, &after).graph_changed());
    }
}
