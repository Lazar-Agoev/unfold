//! A page's outline ("On this page"): its cards in the order you followed them, callers before
//! callees, indented by call depth, and the Flow nodes that still show names from the code
//! instead of authored labels (not counting the steps a collapsed step group hides).
//!
//! A *card link* is an opened call between two cards of one project: an entrance card to the
//! cards of its resolved targets (in registration order), or a block card to the cards of the
//! resolved targets of the occurrences its block owns (in source order). The outline follows
//! card links and then the page's drawn links between two cards (in id order), which join cards
//! across projects; links to resources and frames aren't followed.
//! Roots are the cards nothing on the page links to, entrance cards first, then left to right
//! and top to bottom; each is followed depth first. Cards only reachable through a cycle come
//! last, in the same order.

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use unfold_core::Key;
use unfold_core::vocab::Resolution;

use super::graph::PageGraphs;
use super::model::{Card, CardRef, Page, ProjectName};
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

/// A page's outline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PageOutline {
    /// The page.
    pub page: String,
    /// Every card once, in follow order.
    pub rows: Vec<OutlineRow>,
    /// The Flow nodes without authored labels, in outline order ("N Flow steps unlabeled"); Start,
    /// End, and the steps inside a collapsed step group of their card don't count.
    pub unlabeled: Vec<Unlabeled>,
}

/// One call between two cards of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLink {
    /// The calling card.
    pub source: String,
    /// The card called.
    pub target: String,
    /// Its order among the source card's links.
    pub order: u32,
}

/// The calls between the page's cards of each connected project, by source card in page order.
pub fn card_links(page: &Page, graphs: &PageGraphs<'_>) -> Vec<CardLink> {
    // The first card showing each block of each project.
    let mut by_block: FxHashMap<(Option<&ProjectName>, &str), &Card> = FxHashMap::default();
    for card in &page.cards {
        if let Some(key) = card.reference.block_key() {
            by_block
                .entry((card.reference.project(), key.as_str()))
                .or_insert(card);
        }
    }
    let mut links = Vec::new();
    for card in &page.cards {
        let project = card.reference.project();
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        let targets: Vec<&Key> = match &card.reference {
            CardRef::Entrance { entrance, .. } => graph
                .entrance(entrance.as_str())
                .map(|entrance| {
                    entrance
                        .targets
                        .iter()
                        .filter(|target| target.resolution == Resolution::Resolved)
                        .filter_map(|target| target.target.as_ref())
                        .collect()
                })
                .unwrap_or_default(),
            CardRef::Block { block, .. } => graph
                .owned_edges(block.as_str())
                .into_iter()
                .filter(|(_, edge)| edge.resolution == Resolution::Resolved)
                .filter_map(|(_, edge)| edge.target.as_ref())
                .collect(),
        };
        let mut order = 0;
        for target in targets {
            let Some(called) = by_block.get(&(project, target.as_str())) else {
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

/// Labels of one project's blocks on a page, for building Flows.
struct Labels<'a> {
    page: &'a Page,
    project: Option<&'a ProjectName>,
}

impl FlowContext for Labels<'_> {
    fn label(&self, key: &str) -> Option<&str> {
        self.page.label(self.project, key)
    }
}

/// Whether `key` is `member` or lies under it (`<member>/…`).
fn within(key: &str, member: &str) -> bool {
    key.strip_prefix(member)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The outline of `page`.
pub fn outline(page: &Page, graphs: &PageGraphs<'_>) -> PageOutline {
    let mut links = card_links(page, graphs);
    let mut drawn: Vec<_> = page
        .links
        .iter()
        .filter(|link| page.card(&link.from).is_some() && page.card(&link.to).is_some())
        .collect();
    drawn.sort_by(|a, b| a.id.cmp(&b.id));
    for link in drawn {
        let order = links
            .iter()
            .filter(|known| known.source == link.from)
            .map(|known| known.order + 1)
            .max()
            .unwrap_or(0);
        links.push(CardLink {
            source: link.from.clone(),
            target: link.to.clone(),
            order,
        });
    }
    let mut children: FxHashMap<&str, Vec<&CardLink>> = FxHashMap::default();
    let mut called: FxHashSet<&str> = FxHashSet::default();
    for link in &links {
        children.entry(link.source.as_str()).or_default().push(link);
        called.insert(link.target.as_str());
    }
    for list in children.values_mut() {
        list.sort_by_key(|link| link.order);
    }
    let mut by_place: Vec<&Card> = page.cards.iter().collect();
    by_place.sort_by(|a, b| {
        let block = |card: &Card| matches!(card.reference, CardRef::Block { .. });
        block(a)
            .cmp(&block(b))
            .then(a.at[0].total_cmp(&b.at[0]))
            .then(a.at[1].total_cmp(&b.at[1]))
    });
    let mut rows = Vec::with_capacity(page.cards.len());
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
    let mut by_id: FxHashMap<&str, &Card> = FxHashMap::default();
    for card in &page.cards {
        by_id.entry(card.id.as_str()).or_insert(card);
    }
    // The member keys of each card's collapsed step groups: their steps aren't drawn.
    let mut collapsed: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
    for group in page.groups.iter().filter(|group| group.collapsed) {
        collapsed
            .entry(group.card.as_str())
            .or_default()
            .extend(group.members.iter().map(Key::as_str));
    }
    let mut unlabeled = Vec::new();
    for row in &rows {
        let Some(card) = by_id.get(row.card.as_str()) else {
            continue;
        };
        let (Some(key), project) = (card.reference.block_key(), card.reference.project()) else {
            continue;
        };
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        let labels = Labels { page, project };
        let Some(flow) = graph.flow(key.as_str(), &labels) else {
            continue;
        };
        if flow.source != SourceState::Current {
            continue;
        }
        let hidden = collapsed.get(row.card.as_str()).map_or(&[][..], Vec::as_slice);
        for node in &flow.nodes {
            if let Some(key) = &node.key
                && !matches!(node.kind, FlowNodeKind::Start | FlowNodeKind::End)
                && !hidden.iter().any(|member| within(key.as_str(), member))
                && page.label(project, key.as_str()).is_none()
            {
                unlabeled.push(Unlabeled {
                    card: row.card.clone(),
                    key: key.clone(),
                });
            }
        }
    }
    PageOutline {
        page: page.id.clone(),
        rows,
        unlabeled,
    }
}
