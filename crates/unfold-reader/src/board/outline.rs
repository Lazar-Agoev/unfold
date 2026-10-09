//! The board's outline (the Boards view): its cards in the order you followed them, callers
//! before callees, indented by call depth, and the Flow nodes that still show names from the
//! code instead of authored labels.
//!
//! A *link* is an opened call between two cards: an entrance card to the cards of its resolved
//! targets (in registration order), or a block card to the cards of the resolved targets of the
//! occurrences its block owns (in source order). Roots are the cards nothing on the board links
//! to, entrance cards first, then left to right and top to bottom; each is followed depth
//! first. Cards only reachable through a cycle come last, in the same order.

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use unfold_core::Key;
use unfold_core::vocab::Resolution;

use super::graph::BoardGraph;
use super::model::{Board, Card, CardRef};
use crate::flow::{FlowContext, FlowNodeKind};
use crate::source::SourceState;

/// One outline row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutlineRow {
    /// The card.
    pub card: String,
    /// Its call depth (0 for a root).
    pub depth: u32,
}

/// A Flow node still showing a fallback name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Unlabeled {
    /// The card whose Flow shows it.
    pub card: String,
    /// The node's block.
    pub key: Key,
}

/// The board's outline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardOutline {
    /// Every card once, in follow order.
    pub rows: Vec<OutlineRow>,
    /// The Flow nodes without authored labels, in outline order ("N Flow nodes still show code
    /// names").
    pub unlabeled: Vec<Unlabeled>,
}

/// One link between cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLink {
    /// The calling card.
    pub source: String,
    /// The card called.
    pub target: String,
    /// Its order among the source card's links.
    pub order: u32,
}

/// The links between the board's cards, by source card in board order.
pub fn card_links(board: &Board, graph: &BoardGraph) -> Vec<CardLink> {
    // The first card showing each block, as `Board::card_for_block` finds it.
    let mut by_block: FxHashMap<&str, &Card> = FxHashMap::default();
    for card in &board.cards {
        if let Some(key) = card.reference.block() {
            by_block.entry(key.as_str()).or_insert(card);
        }
    }
    let mut links = Vec::new();
    for card in &board.cards {
        let targets: Vec<&Key> = match &card.reference {
            CardRef::Entrance(key) => graph
                .entrance(key.as_str())
                .map(|entrance| {
                    entrance
                        .targets
                        .iter()
                        .filter(|target| target.resolution == Resolution::Resolved)
                        .filter_map(|target| target.target.as_ref())
                        .collect()
                })
                .unwrap_or_default(),
            CardRef::Block(key) => graph
                .owned_edges(key.as_str())
                .into_iter()
                .filter(|(_, edge)| edge.resolution == Resolution::Resolved)
                .filter_map(|(_, edge)| edge.target.as_ref())
                .collect(),
        };
        let mut order = 0;
        for target in targets {
            let Some(called) = by_block.get(target.as_str()) else {
                continue;
            };
            if called.id == card.id {
                continue;
            }
            links.push(CardLink {
                source: card.id.clone(),
                target: called.id.clone(),
                order,
            });
            order += 1;
        }
    }
    links
}

/// Labels from the board, for building Flows.
struct Labels<'a>(&'a Board);

impl FlowContext for Labels<'_> {
    fn label(&self, key: &str) -> Option<&str> {
        self.0.label(key)
    }
}

/// The outline of `board`.
pub fn outline(board: &Board, graph: &BoardGraph) -> BoardOutline {
    let links = card_links(board, graph);
    let mut children: FxHashMap<&str, Vec<&CardLink>> = FxHashMap::default();
    let mut called: FxHashSet<&str> = FxHashSet::default();
    for link in &links {
        children.entry(link.source.as_str()).or_default().push(link);
        called.insert(link.target.as_str());
    }
    for list in children.values_mut() {
        list.sort_by_key(|link| link.order);
    }
    let mut by_place: Vec<&Card> = board.cards.iter().collect();
    by_place.sort_by(|a, b| {
        let block = |card: &Card| matches!(card.reference, CardRef::Block(_));
        block(a)
            .cmp(&block(b))
            .then(a.at[0].total_cmp(&b.at[0]))
            .then(a.at[1].total_cmp(&b.at[1]))
    });
    let mut rows = Vec::with_capacity(board.cards.len());
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    let roots = by_place
        .iter()
        .filter(|card| !called.contains(card.id.as_str()))
        .chain(by_place.iter());
    for root in roots {
        // Depth-first with an explicit stack, children in link order.
        let mut stack: Vec<(&str, u32)> = vec![(root.id.as_str(), 0)];
        while let Some((id, depth)) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            rows.push(OutlineRow {
                card: id.to_owned(),
                depth,
            });
            if let Some(list) = children.get(id) {
                for link in list.iter().rev() {
                    stack.push((link.target.as_str(), depth + 1));
                }
            }
        }
    }
    let labels = Labels(board);
    let mut by_id: FxHashMap<&str, &Card> = FxHashMap::default();
    for card in &board.cards {
        by_id.entry(card.id.as_str()).or_insert(card);
    }
    let mut unlabeled = Vec::new();
    for row in &rows {
        let Some(CardRef::Block(key)) = by_id.get(row.card.as_str()).map(|card| &card.reference) else {
            continue;
        };
        let Some(flow) = graph.flow(key.as_str(), &labels) else {
            continue;
        };
        if flow.source != SourceState::Current {
            continue;
        }
        for node in &flow.nodes {
            if let Some(key) = &node.key
                && !matches!(node.kind, FlowNodeKind::Start | FlowNodeKind::End)
                && board.label(key.as_str()).is_none()
            {
                unlabeled.push(Unlabeled {
                    card: row.card.clone(),
                    key: key.clone(),
                });
            }
        }
    }
    BoardOutline { rows, unlabeled }
}
