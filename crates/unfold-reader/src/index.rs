//! The compact whole-graph structure behind owners, closures, and Reached from: nodes numbered
//! across files with their kinds, first lines, and owners; what each definition reaches; and the
//! apps' and entrances' starting definitions.
//!
//! It holds no ranges or texts, so it stays small enough to keep per graph version. Building it
//! is one pass over the files, then resolving link and target keys to node numbers.
//!
//! **What a definition reaches** is what running it can run, by written facts only:
//!
//! - the project targets of the `calls`, `passes`, and `imports` links it holds: the links of
//!   the blocks that belong to it ([`unfold_core::owner`]) and its own links (a function's or
//!   module's `imports`);
//! - the classes it defines (defining a class runs its body) and the inline functions written
//!   in it (callbacks and lambdas, which it hands to what it calls).
//!
//! Library and unknown links stop there.

use rustc_hash::FxHashMap;
use unfold_core::vocab::{Form, LinkKind, NodeKind, TargetState};
use unfold_core::{App, FileGraph, Graph, Key, LinkTarget, owner};

use crate::catalog::{Start, starts};
use crate::tree::to_u32;

/// A node's number in an index: its position across all files, in graph (path, then preorder)
/// order.
pub(crate) type NodeId = u32;

/// The absent [`NodeId`].
const NONE: NodeId = u32::MAX;

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

    fn get(&self, bucket: NodeId) -> &[u32] {
        let bucket = bucket as usize;
        match (self.starts.get(bucket), self.starts.get(bucket + 1)) {
            (Some(&start), Some(&end)) => &self.items[start as usize..end as usize],
            _ => &[],
        }
    }
}

/// An app's start or an entrance, with the definitions it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartFact {
    /// `entrance:<app>` or `entrance:<app>/<entrance>`.
    pub key: Key,
    /// An entrance rather than an app's own start.
    pub entrance: bool,
    /// Its `code` targets that name a definition in the index (`found`, or `stale`: a file's
    /// last good version), in order.
    pub seeds: Box<[NodeId]>,
}

/// The compact graph structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphIndex {
    keys: Vec<Key>,
    by_key: FxHashMap<Key, NodeId>,
    kind: Vec<NodeKind>,
    line: Vec<u32>,
    owner: Vec<NodeId>,
    reaches: Buckets,
    starts: Vec<StartFact>,
}

impl GraphIndex {
    /// Indexes `graph`.
    pub fn from_graph(graph: &Graph) -> Self {
        let mut keys = Vec::new();
        let mut kind = Vec::new();
        let mut line = Vec::new();
        let mut owners = Vec::new();
        // Each reach before its target is resolved: the holding definition and the target key.
        let mut links: Vec<(NodeId, &Key)> = Vec::new();
        let mut defines: Vec<(u32, u32)> = Vec::new();
        for file in &graph.files {
            add_file(
                file,
                &mut keys,
                &mut kind,
                &mut line,
                &mut owners,
                &mut links,
                &mut defines,
            );
        }
        let by_key: FxHashMap<Key, NodeId> = keys
            .iter()
            .enumerate()
            .map(|(id, key)| (key.clone(), to_u32(id)))
            .collect();
        let mut pairs = defines;
        pairs.extend(
            links
                .into_iter()
                .filter_map(|(holder, target)| Some((holder, *by_key.get(target)?))),
        );
        pairs.sort_unstable();
        pairs.dedup();
        let starts = start_facts(&graph.apps, &by_key, &kind);
        Self {
            reaches: Buckets::new(keys.len(), &pairs),
            keys,
            by_key,
            kind,
            line,
            owner: owners,
            starts,
        }
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// The node with `key`.
    pub fn id(&self, key: &str) -> Option<NodeId> {
        self.by_key.get(key).copied()
    }

    /// A node's key.
    pub fn key(&self, id: NodeId) -> &Key {
        &self.keys[id as usize]
    }

    /// A node's first line.
    pub fn line(&self, id: NodeId) -> u32 {
        self.line[id as usize]
    }

    /// The definition a node belongs to; `None` for a module.
    pub fn owner(&self, id: NodeId) -> Option<NodeId> {
        let owner = self.owner[id as usize];
        (owner != NONE).then_some(owner)
    }

    /// The node itself when it is a definition, else the definition it belongs to.
    pub fn definition(&self, id: NodeId) -> Option<NodeId> {
        if self.kind[id as usize].is_definition() {
            Some(id)
        } else {
            self.owner(id)
        }
    }

    /// What definition `id` reaches (see the module documentation), in node order.
    pub fn reaches(&self, id: NodeId) -> &[NodeId] {
        self.reaches.get(id)
    }

    /// The apps' starts and entrances, in file order.
    pub fn starts(&self) -> &[StartFact] {
        &self.starts
    }
}

/// Adds one file's nodes, its reaches by key, and its defined classes and inline functions.
fn add_file<'a>(
    file: &'a FileGraph,
    keys: &mut Vec<Key>,
    kind: &mut Vec<NodeKind>,
    line: &mut Vec<u32>,
    owners: &mut Vec<NodeId>,
    links: &mut Vec<(NodeId, &'a Key)>,
    defines: &mut Vec<(u32, u32)>,
) {
    let base = to_u32(keys.len());
    for (local, node) in file.nodes.iter().enumerate() {
        let local = to_u32(local);
        let owner = owner(&file.nodes, local).map_or(NONE, |owner| base + owner);
        keys.push(node.key.clone());
        kind.push(node.kind());
        line.push(node.lines.start);
        owners.push(owner);
        let defined = node.kind() == NodeKind::Class || node.form == Form::Lambda;
        if defined && owner != NONE {
            defines.push((owner, base + local));
        }
    }
    for link in &file.links {
        let LinkTarget::Project(target) = &link.to else {
            continue;
        };
        if !matches!(link.kind, LinkKind::Calls | LinkKind::Passes | LinkKind::Imports) {
            continue;
        }
        let from = base + link.from;
        let holder = if kind[from as usize].is_definition() {
            from
        } else {
            owners[from as usize]
        };
        if holder != NONE {
            links.push((holder, target));
        }
    }
}

/// The starts of `apps`, with their targets that name an indexed definition.
fn start_facts(apps: &[App], by_key: &FxHashMap<Key, NodeId>, kind: &[NodeKind]) -> Vec<StartFact> {
    starts(apps)
        .map(|start| StartFact {
            key: start.key().clone(),
            entrance: matches!(start, Start::Entrance { .. }),
            seeds: start
                .targets()
                .iter()
                .filter(|target| matches!(target.state, TargetState::Found | TargetState::Stale))
                .filter_map(|target| by_key.get(target.code.as_str()).copied())
                .filter(|&id| kind[id as usize].is_definition())
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use unfold_core::testkit::sample;

    use super::*;

    fn keys(index: &GraphIndex, ids: &[NodeId]) -> Vec<String> {
        ids.iter().map(|&id| index.key(id).to_string()).collect()
    }

    #[test]
    fn definitions_reach_their_project_links_classes_and_inline_functions() {
        let index = GraphIndex::from_graph(&sample::shop().build());
        let reach = |key: &str| keys(&index, index.reaches(index.id(key).unwrap()));
        assert_eq!(
            reach("shop/service.py#place_order"),
            [
                "shop/payments.py#charge_card",
                "shop/store.py#load_order",
                "shop/store.py#save_order",
                "shop/store.py#mark_failed",
                "shop/store.py#reserve_stock",
            ]
        );
        // The module's own `imports` links, and the class it defines and the decorator's target.
        assert_eq!(
            reach("shop/payments.py#"),
            ["shop/payments.py#PaymentDeclined", "shop/payments.py#retry"]
        );
        assert_eq!(reach("shop/service.py#"), ["shop/payments.py#", "shop/store.py#"]);
        // A `passes` link reaches, and so does a callback written in the function.
        assert_eq!(reach("shop/store.py#save_all"), ["shop/store.py#save_order"]);
        assert_eq!(
            reach("shop/store.py#reserve_stock"),
            ["shop/store.py#reserve_stock/for[0]/call:sorted/callback[0]"]
        );
        assert_eq!(
            reach("shop/payments.py#charge_card"),
            ["shop/payments.py#PaymentDeclined"]
        );
    }

    #[test]
    fn starts_keep_their_definitions_in_order() {
        let index = GraphIndex::from_graph(&sample::shop().build());
        let starts: Vec<(&str, bool, Vec<String>)> = index
            .starts()
            .iter()
            .map(|start| (start.key.as_str(), start.entrance, keys(&index, &start.seeds)))
            .collect();
        assert_eq!(
            starts[0],
            ("entrance:shop", false, vec!["shop/service.py#".to_owned()])
        );
        assert_eq!(
            starts[1],
            (
                "entrance:shop/place-an-order",
                true,
                vec!["shop/service.py#place_order".to_owned()]
            )
        );
        // A missing key and a key that is no start add nothing.
        assert!(starts[3].2.is_empty());
        assert_eq!(starts[4].2, ["shop/payments.py#charge_card"]);
    }
}
