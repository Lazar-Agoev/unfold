//! Checking a page against the current graph: group validity, guarded references, and health
//! with its repairs (docs/reader.md#boards).
//!
//! **Reconnecting** after a graph update is a matter of
//! looking keys up again: a surviving key reconnects; a missing one stays on the page as a
//! placeholder (health: missing); a guarded reference whose kind or owner changed, or a
//! position-based (ordinal) reference whose code changed, is held for review instead of silently
//! attaching its text to different code. A rename or move leaves the old key missing, and
//! [`Repair::Reconnect`] is the manual repair.
//!
//! A drawn link is checked only for its ends: it is **broken** when an end's card is missing
//! from the graph (a resource or frame end is never missing); its label is never outdated. The
//! claims (resource cards, connections, and frame descriptions) are checked by [`claims`]: what
//! they cite must still be there, and their signatures must still match. A proposed step's anchor
//! is a guarded reference too: missing or held, it keeps its text and waits for an explicit
//! Reconnect, Accept, or removal; it is never reattached by name or shifted position.

use std::fmt;

use serde::Serialize;

use super::BoardError;
use super::claims;
use super::graph::{BoardGraph, Lines};
use super::items::remove_items;
use super::model::{
    Anchor, Board, CardRef, Group, GuardEntry, Link, Page, ProposalPath, Signature, is_ordinal,
};
use crate::flow::{NoContext, RouteKind};
use crate::source::SourceState;

/// What a health item or repair is about, on one page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// A card, by id.
    Card(String),
    /// A group, by id.
    Group(String),
    /// A Flow label, by block key.
    Label(String),
    /// A drawn link or a connection, by id.
    Link(String),
    /// A resource card, by id.
    Resource(String),
    /// A frame, by id.
    Frame(String),
    /// A proposed step, by id.
    Proposal(String),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Card(id) => write!(f, "card `{id}`"),
            Self::Group(id) => write!(f, "group `{id}`"),
            Self::Label(key) => write!(f, "label of `{key}`"),
            Self::Link(id) => write!(f, "link `{id}`"),
            Self::Resource(id) => write!(f, "resource `{id}`"),
            Self::Frame(id) => write!(f, "frame `{id}`"),
            Self::Proposal(id) => write!(f, "proposal `{id}`"),
        }
    }
}

/// The kind of a health problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum HealthKind {
    /// A reference (a card's, a label's, a resource card's anchor or type, or a proposed step's
    /// anchor) is not in the graph (a placeholder until repaired).
    Missing,
    /// A description's, label's, resource card's, or connection's source changed since it was
    /// signed.
    Outdated,
    /// A group no longer meets the group rules.
    BrokenGroup,
    /// A guarded reference changed and is held for review.
    Review,
    /// An end of a link is a missing card, or a connection's step, carried type, or anchor is
    /// gone.
    BrokenLink,
}

/// A repair the UI can offer for a health item; [`apply_repair`] performs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RepairKind {
    /// Point the reference at another key (after a rename or move).
    Reconnect,
    /// Remove the card (and its groups, proposed steps, and links).
    RemoveCard,
    /// Remove the label.
    RemoveLabel,
    /// Remove the group, keeping its statements.
    Ungroup,
    /// Remove the link or connection.
    RemoveLink,
    /// Remove the resource card (and the links to it).
    RemoveResource,
    /// Remove the description (a connection's description and label).
    RemoveDescription,
    /// Keep the text and sign it against the current source.
    Resign,
    /// Accept the reference's current code: save a fresh guard for it.
    Accept,
    /// Remove the proposed step.
    RemoveProposal,
}

/// One health problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct HealthItem {
    /// The page it is on.
    pub page: String,
    /// What kind of problem.
    pub kind: HealthKind,
    /// What it is about.
    pub target: Target,
    /// The reference involved, when it is not the target itself (a group member; a resource
    /// card's or connection's anchor, type, step, or carried type).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<String>,
    /// The position of the anchor involved among its resource card's or connection's anchors.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub anchor: Option<u32>,
    /// The explanation.
    pub message: String,
    /// The repairs to offer.
    pub repairs: Vec<RepairKind>,
}

/// The health of a page, or of a whole board (its pages' items in page order).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Every problem: per page, cards first, then groups, labels, links' ends, resource cards,
    /// connections, frames, and proposed steps.
    pub items: Vec<HealthItem>,
    /// The number of missing references.
    pub missing: u32,
    /// The number of outdated descriptions, labels, resource cards, and connections.
    pub outdated: u32,
    /// The number of broken-group items.
    pub broken_groups: u32,
    /// The number of references held for review.
    pub review: u32,
    /// The number of broken links.
    pub broken_links: u32,
}

impl Health {
    /// Whether there are no problems.
    pub fn is_healthy(&self) -> bool {
        self.items.is_empty()
    }

    fn add(&mut self, item: HealthItem) {
        if self.items.iter().any(|known| {
            known.page == item.page
                && known.kind == item.kind
                && known.target == item.target
                && known.message == item.message
        }) {
            return;
        }
        let counter = match item.kind {
            HealthKind::Missing => &mut self.missing,
            HealthKind::Outdated => &mut self.outdated,
            HealthKind::BrokenGroup => &mut self.broken_groups,
            HealthKind::Review => &mut self.review,
            HealthKind::BrokenLink => &mut self.broken_links,
        };
        *counter += 1;
        self.items.push(item);
    }

    /// Adds another page's items.
    pub fn append(&mut self, other: Self) {
        for item in other.items {
            self.add(item);
        }
    }

    /// The items of page `page` alone.
    pub fn of_page(&self, page: &str) -> Self {
        let mut health = Self::default();
        for item in self.items.iter().filter(|item| item.page == page) {
            health.add(item.clone());
        }
        health
    }

    /// Whether page `page` has a problem of `kind` about `target`.
    pub fn has(&self, page: &str, kind: HealthKind, target: &Target) -> bool {
        self.items
            .iter()
            .any(|item| item.page == page && item.kind == kind && &item.target == target)
    }

    /// The board list's label, only when something is wrong ("1 missing · 2 outdated").
    pub fn label(&self) -> Option<String> {
        let parts: Vec<String> = [
            (self.missing, "missing", "missing"),
            (self.outdated, "outdated", "outdated"),
            (self.broken_groups, "broken group", "broken groups"),
            (self.review, "to review", "to review"),
            (self.broken_links, "broken link", "broken links"),
        ]
        .into_iter()
        .filter(|(count, ..)| *count > 0)
        .map(|(count, one, many)| format!("{count} {}", if count == 1 { one } else { many }))
        .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

const CHANGED: &str = "Code changed since this description was written";
const LABEL_CHANGED: &str = "Code changed since this label was written";

/// Why a guarded reference is held for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// Its kind or owner changed.
    KindOrOwner,
    /// It is position-based and its source changed.
    Ordinal,
}

impl Held {
    /// The health message of a held reference.
    const fn message(self) -> &'static str {
        match self {
            Self::KindOrOwner => "Saved reference kind or owner changed; needs review",
            Self::Ordinal => "Position-based reference changed; needs review before reconnecting",
        }
    }

    /// The health message of a proposed step whose anchor is held: where it goes needs review.
    const fn placement(self) -> &'static str {
        match self {
            Self::KindOrOwner => {
                "The step this proposal goes after changed kind or owner; check where it goes, then accept the current code, reconnect it to another step, or remove the proposal"
            }
            Self::Ordinal => {
                "The step this proposal goes after is position-based and its code changed; check where it goes, then accept the current code, reconnect it to another step, or remove the proposal"
            }
        }
    }
}

/// Why `group` breaks the group rules in `graph`, if it does: a
/// run of consecutive sibling statements with one parent and one body role (`body`, `then`,
/// `else`, `catch`, `finally`), inside its block card, never crossing a body boundary.
pub(crate) fn group_problem(page: &Page, graph: &BoardGraph, group: &Group) -> Option<&'static str> {
    let Some(CardRef::Block { block: card_key, .. }) = page.card(&group.card).map(|card| &card.reference)
    else {
        return Some("Group must belong to a block card");
    };
    let Some(card) = graph.block(card_key.as_str()) else {
        return Some("Group card is missing from the index");
    };
    let mut members = Vec::with_capacity(group.members.len());
    for key in &group.members {
        match graph.block(key.as_str()) {
            Some(member) => members.push(member),
            None => return Some("Group contains missing block references"),
        }
    }
    let first = members.first()?;
    if members
        .iter()
        .any(|member| !member.within(&card) || member.local() == card.local())
    {
        return Some("Group members must be inside their card's body");
    }
    let (parent, role) = (first.parent(), first.block().role);
    if members
        .iter()
        .any(|member| member.parent() != parent || member.block().role != role)
    {
        return Some("Group members must be siblings in the same body");
    }
    if !BoardGraph::is_group_role(role) {
        return Some("Group members must be statements in one body");
    }
    let siblings = first.siblings();
    let start = siblings.iter().position(|&local| local == first.local())?;
    let consecutive = members
        .iter()
        .enumerate()
        .all(|(offset, member)| siblings.get(start + offset) == Some(&member.local()));
    (!consecutive).then_some("Group members must be consecutive siblings in source order")
}

/// Why guarded reference `key` is held for review in `graph`, if it is: its kind or owner
/// changed, or it is position-based and its source changed. Missing references are not held
/// (they are placeholders), and references without a guard are never held.
pub(crate) fn guard_problem(page: &Page, graph: &BoardGraph, key: &str) -> Option<&'static str> {
    held(page, graph, key).map(Held::message)
}

/// Why guarded reference `key` is held for review in `graph`, if it is ([`guard_problem`]).
fn held(page: &Page, graph: &BoardGraph, key: &str) -> Option<Held> {
    let saved = page.guard(key)?;
    let kind = graph.guard_kind(key)?;
    let owner = graph.guard_owner(key)?;
    if kind != saved.kind || owner != saved.owner {
        return Some(Held::KindOrOwner);
    }
    let signature = graph.signature(key);
    if is_ordinal(key) && signature.is_some_and(|signature| signature != saved.signature) {
        return Some(Held::Ordinal);
    }
    None
}

/// Why a proposed step on the block card with key `card` can't go after `after` on `path` in
/// `graph`, if it can't. The rule is the UI's (`placementProblem`): it goes
/// after a step of the card's own Flow (not the card itself, a call inside a statement, or a step
/// of a nested definition) that something runs after (not a return, throw, break, continue, or a
/// region whose routing isn't modeled), on one of that step's paths when it has several (a
/// condition's yes and no, a loop's each and done) and on none when it has one way on.
pub(crate) fn placement_problem(
    graph: &BoardGraph,
    card: &str,
    after: &str,
    path: Option<ProposalPath>,
) -> Option<String> {
    if after == card {
        return Some(format!(
            "Can't place it after its card itself: a proposed step goes after a step inside the card's block. Choose a statement or call under {card}/"
        ));
    }
    if !after.strip_prefix(card).is_some_and(|rest| rest.starts_with('/')) {
        return Some(format!(
            "Can't place it after {after}: that is not inside its card's block. Choose a statement or call under {card}/"
        ));
    }
    if graph.block(after).is_none() {
        return Some(format!(
            "Can't place it after {after}: that step is not in the graph. Choose a statement or call under {card}/ that the code has now"
        ));
    }
    let flow = graph
        .flow(card, &NoContext)
        .filter(|flow| flow.source == SourceState::Current);
    let Some(flow) = flow else {
        return Some(format!(
            "Can't place it after {after}: its card's source changed since the last compile or is missing, so its Flow can't be checked. Compile the project, then try again"
        ));
    };
    let Some(node) = flow
        .nodes
        .iter()
        .find(|node| node.key.as_ref().is_some_and(|key| key.as_str() == after))
    else {
        return Some(format!(
            "Can't place it after {after}: that is not a step of its card's Flow (a call inside a statement, or a step of a nested definition, is not). Choose a step its card's Flow shows"
        ));
    };
    if node.kind.is_terminal() {
        return Some(format!(
            "Can't place it after {after}: nothing runs after it on its path. Choose the step before it, or a step on another path"
        ));
    }
    let mut paths: Vec<ProposalPath> = Vec::new();
    for route in flow.routes.iter().filter(|route| route.from == node.id) {
        if let Some(found) = route_path(route.kind)
            && !paths.contains(&found)
        {
            paths.push(found);
        }
    }
    let choices = || {
        paths
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join(" or ")
    };
    match path {
        None if paths.len() > 1 => Some(format!(
            "Can't place it after {after}: that step has {} paths: choose {}",
            paths.len(),
            choices()
        )),
        Some(path) if paths.len() > 1 && !paths.contains(&path) => Some(format!(
            "Can't place it after {after}: that step has no {} path: choose {}",
            path.as_str(),
            choices()
        )),
        Some(_) if paths.len() <= 1 => Some(format!(
            "Can't place it after {after}: that step has one way on, so a proposed step after it takes no path"
        )),
        _ => None,
    }
}

/// The proposal path a route out of a decision is on (an `else if` is the condition's no, a
/// do-while's again its each), as the UI's `routePath` reads it; none for any other route.
fn route_path(kind: RouteKind) -> Option<ProposalPath> {
    match kind {
        RouteKind::Yes => Some(ProposalPath::Yes),
        RouteKind::No | RouteKind::ElseIf => Some(ProposalPath::No),
        RouteKind::Each | RouteKind::Again => Some(ProposalPath::Each),
        RouteKind::Done => Some(ProposalPath::Done),
        _ => None,
    }
}

/// The guard to save for `key` now, from `graph`: its kind, owner, and current signature.
pub fn current_guard(graph: &BoardGraph, key: &str) -> Option<GuardEntry> {
    Some(GuardEntry {
        key: unfold_core::Key::from_text(key),
        kind: graph.guard_kind(key)?,
        owner: graph.guard_owner(key)?,
        signature: graph.signature(key)?,
    })
}

/// Sets the guard of its key on `page`, replacing any saved one.
pub(crate) fn put_guard(page: &mut Page, guard: GuardEntry) {
    match page.guards.iter_mut().find(|entry| entry.key == guard.key) {
        Some(entry) => *entry = guard,
        None => page.guards.push(guard),
    }
}

fn outdated(saved: Option<Signature>, current: Option<Signature>) -> bool {
    saved.is_some() && current != saved
}

/// Whether link end `end` is missing: gone from the page, or a card whose key is gone from the
/// graph (a resource or frame of the page is never missing).
fn missing_end(page: &Page, graph: &BoardGraph, end: &str) -> bool {
    match page.card(end) {
        None => page.resource(end).is_none() && page.frame(end).is_none(),
        Some(card) => !graph.exists(card.reference.key().as_str()),
    }
}

/// The page's health against `graph`.
pub fn health(page: &Page, graph: &BoardGraph) -> Health {
    let mut health = Health::default();
    let item = |kind, target: Target, key: Option<&str>, message: &str, repairs: &[RepairKind]| HealthItem {
        page: page.id.clone(),
        kind,
        target,
        key: key.map(str::to_owned),
        anchor: None,
        message: message.to_owned(),
        repairs: repairs.to_vec(),
    };
    for card in &page.cards {
        let key = card.reference.key().as_str();
        let target = Target::Card(card.id.clone());
        if !graph.exists(key) {
            health.add(item(
                HealthKind::Missing,
                target,
                None,
                &format!("Code reference is missing: {key}"),
                &[RepairKind::Reconnect, RepairKind::RemoveCard],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(page, graph, key) {
            health.add(item(
                HealthKind::Review,
                target.clone(),
                None,
                problem,
                &[RepairKind::Accept, RepairKind::Reconnect, RepairKind::RemoveCard],
            ));
        }
        if let Some(description) = &card.description
            && outdated(description.signature, graph.signature(key))
        {
            health.add(item(
                HealthKind::Outdated,
                target,
                None,
                CHANGED,
                &[RepairKind::Resign, RepairKind::RemoveDescription],
            ));
        }
    }
    for group in &page.groups {
        let target = Target::Group(group.id.clone());
        if let Some(problem) = group_problem(page, graph, group) {
            health.add(item(
                HealthKind::BrokenGroup,
                target,
                None,
                problem,
                &[RepairKind::Ungroup],
            ));
            continue;
        }
        for key in &group.members {
            if let Some(problem) = guard_problem(page, graph, key.as_str()) {
                health.add(item(
                    HealthKind::Review,
                    target.clone(),
                    Some(key.as_str()),
                    problem,
                    &[RepairKind::Accept, RepairKind::Ungroup],
                ));
            }
        }
        if let Some(description) = &group.description {
            let current = match (group.members.first(), group.members.last()) {
                (Some(first), Some(last)) => graph.span_signature(first.as_str(), last.as_str()),
                _ => None,
            };
            if outdated(description.signature, current) {
                health.add(item(
                    HealthKind::Outdated,
                    target,
                    None,
                    CHANGED,
                    &[RepairKind::Resign, RepairKind::RemoveDescription],
                ));
            }
        }
    }
    for entry in &page.labels {
        let key = entry.key.as_str();
        let target = Target::Label(key.to_owned());
        if graph.block(key).is_none() {
            health.add(item(
                HealthKind::Missing,
                target,
                None,
                &format!("Labeled block is missing: {key}"),
                &[RepairKind::Reconnect, RepairKind::RemoveLabel],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(page, graph, key) {
            health.add(item(
                HealthKind::Review,
                target,
                None,
                problem,
                &[RepairKind::Accept, RepairKind::RemoveLabel],
            ));
            continue;
        }
        if outdated(entry.label.signature, graph.signature(key)) {
            health.add(item(
                HealthKind::Outdated,
                target,
                None,
                LABEL_CHANGED,
                &[RepairKind::Resign, RepairKind::RemoveLabel],
            ));
        }
    }
    // Groups in one card may neither overlap nor nest.
    for (index, left) in page.groups.iter().enumerate() {
        for right in &page.groups[index + 1..] {
            if left.card != right.card || !overlap(graph, left, right) {
                continue;
            }
            for (group, other) in [(left, right), (right, left)] {
                health.add(item(
                    HealthKind::BrokenGroup,
                    Target::Group(group.id.clone()),
                    None,
                    &format!("Groups overlap or nest with {}", other.id),
                    &[RepairKind::Ungroup],
                ));
            }
        }
    }
    for link in &page.links {
        if let Some(end) = [&link.from, &link.to]
            .into_iter()
            .find(|end| missing_end(page, graph, end))
        {
            health.add(item(
                HealthKind::BrokenLink,
                Target::Link(link.id.clone()),
                None,
                &format!("Linked card {end} is missing"),
                &[RepairKind::RemoveLink],
            ));
        }
    }
    for item in claims::health(page, graph) {
        health.add(item);
    }
    for proposal in &page.proposals {
        let target = Target::Proposal(proposal.id.clone());
        let after = proposal.after.as_str();
        if graph.block(after).is_none() {
            health.add(item(
                HealthKind::Missing,
                target,
                Some(after),
                &format!(
                    "The step this proposal goes after is missing: {after}. Reconnect it to the step it goes after now, or remove the proposal"
                ),
                &[RepairKind::Reconnect, RepairKind::RemoveProposal],
            ));
            continue;
        }
        if let Some(held) = held(page, graph, after) {
            health.add(item(
                HealthKind::Review,
                target,
                Some(after),
                held.placement(),
                &[
                    RepairKind::Accept,
                    RepairKind::Reconnect,
                    RepairKind::RemoveProposal,
                ],
            ));
        }
    }
    health
}

/// The health of every page of `board`, in page order.
pub fn board_health(board: &Board, graph: &BoardGraph) -> Health {
    let mut health = Health::default();
    for page in &board.pages {
        health.append(self::health(page, graph));
    }
    health
}

fn overlap(graph: &BoardGraph, left: &Group, right: &Group) -> bool {
    left.members.iter().any(|left_key| {
        let a = graph.block(left_key.as_str());
        right.members.iter().any(|right_key| {
            if left_key == right_key {
                return true;
            }
            match (a, graph.block(right_key.as_str())) {
                (Some(a), Some(b)) => a.within(&b) || b.within(&a),
                _ => false,
            }
        })
    })
}

/// A repair with everything it needs (the page is the caller's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// Replace reference `from` with `to` everywhere on the page: card references,
    /// group members, the label's key, resource cards' types and anchors, connections' carried
    /// types and anchors, and its guard (a fresh one is saved). A connection's step and a proposed
    /// step's anchor under `from` (a card's statement or call) move to the same path under `to`;
    /// a proposed step whose anchor is `from` itself must then go after a step inside its card's
    /// block.
    Reconnect {
        /// The missing key.
        from: String,
        /// The key it should point at now.
        to: String,
    },
    /// Replace anchor `index` of resource card or connection `item` with `to` (a key, or lines
    /// of an evidence file), which must be in the graph.
    ReconnectAnchor {
        /// The resource card or connection.
        item: String,
        /// The anchor's position among its anchors.
        index: usize,
        /// The new anchor.
        to: Anchor,
    },
    /// Remove a card with what would dangle: the groups and proposed steps on it, the links drawn
    /// to or from it, its place in a frame, and the notes attached to it (they stay, unattached).
    RemoveCard(String),
    /// Remove the label of a block key.
    RemoveLabel(String),
    /// Remove a group.
    Ungroup(String),
    /// Remove a drawn link or a connection (the notes attached to it stay, unattached).
    RemoveLink(String),
    /// Remove a resource card with what would dangle: the links to and from it, its place in a
    /// frame, and the notes attached to it (they stay, unattached).
    RemoveResource(String),
    /// Remove a card's, group's, resource card's, or frame's description, a label, a drawn
    /// link's label, or a connection's description and label.
    RemoveDescription(Target),
    /// Sign a description, label, link label, resource card, or connection (with its texts)
    /// against the current source, keeping its text.
    Resign(Target),
    /// Save a fresh guard for a reference key, accepting its current code.
    Accept(String),
    /// Remove a proposed step.
    RemoveProposal(String),
}

fn problem(page: &Page, target: Target, message: &str) -> BoardError {
    BoardError::Problem {
        page: page.id.clone(),
        target,
        message: message.to_owned(),
    }
}

/// Applies `repair` to `page`.
pub fn apply_repair(page: &mut Page, graph: &BoardGraph, repair: &Repair) -> Result<(), BoardError> {
    match repair {
        Repair::Reconnect { from, to } => reconnect(page, graph, from, to)?,
        Repair::ReconnectAnchor { item, index, to } => reconnect_anchor(page, graph, item, *index, to)?,
        Repair::RemoveCard(id) => {
            if page.card(id).is_some() {
                remove_items(page, &[id]);
            }
        }
        Repair::RemoveLink(id) => {
            if page.link(id).is_some() {
                remove_items(page, &[id]);
            }
        }
        Repair::RemoveResource(id) => {
            if page.resource(id).is_some() {
                remove_items(page, &[id]);
            }
        }
        Repair::RemoveProposal(id) => {
            if page.proposal(id).is_some() {
                remove_items(page, &[id]);
            }
        }
        Repair::RemoveLabel(label) => remove_label(page, label),
        Repair::Ungroup(id) => page.groups.retain(|group| &group.id != id),
        Repair::RemoveDescription(target) => remove_description(page, target),
        Repair::Resign(target) => super::save::resign(page, graph, target)?,
        Repair::Accept(key) => match current_guard(graph, key) {
            Some(guard) => put_guard(page, guard),
            None => return Err(BoardError::Unverified { what: key.clone() }),
        },
    }
    Ok(())
}

fn remove_description(page: &mut Page, target: &Target) {
    match target {
        Target::Card(id) => {
            if let Some(card) = page.cards.iter_mut().find(|card| &card.id == id) {
                card.description = None;
            }
        }
        Target::Group(id) => {
            if let Some(group) = page.groups.iter_mut().find(|group| &group.id == id) {
                group.description = None;
            }
        }
        Target::Label(label) => remove_label(page, label),
        Target::Link(id) => {
            if let Some(link) = page.links.iter_mut().find(|link| &link.id == id) {
                link.label = None;
                link.description = None;
            }
        }
        Target::Resource(id) => {
            if let Some(resource) = page.resources.iter_mut().find(|resource| &resource.id == id) {
                resource.description = None;
            }
        }
        Target::Frame(id) => {
            if let Some(frame) = page.frames.iter_mut().find(|frame| &frame.id == id) {
                frame.description = None;
            }
        }
        // A proposed step has no description: its title and reason are the proposal itself.
        Target::Proposal(_) => {}
    }
}

fn remove_label(page: &mut Page, key: &str) {
    page.labels.retain(|entry| entry.key.as_str() != key);
}

/// What a repair about reference `key` is about: the card showing it, else the resource card or
/// connection citing it, else the proposed step going after it, else its label.
fn target_of(page: &Page, key: &str) -> Target {
    let is = |known: Option<&unfold_core::Key>| known.is_some_and(|known| known.as_str() == key);
    let cited = |anchors: &[Anchor]| anchors.iter().any(|anchor| is(anchor.key()));
    if let Some(card) = page.cards.iter().find(|card| is(Some(card.reference.key()))) {
        return Target::Card(card.id.clone());
    }
    if let Some(resource) = page
        .resources
        .iter()
        .find(|resource| is(resource.type_key.as_ref()) || cited(&resource.anchors))
    {
        return Target::Resource(resource.id.clone());
    }
    let cites = |link: &&Link| is(link.step.as_ref()) || is(link.carries.as_ref()) || cited(&link.anchors);
    if let Some(link) = page.links.iter().find(cites) {
        return Target::Link(link.id.clone());
    }
    match page
        .proposals
        .iter()
        .find(|proposal| proposal.after.as_str() == key)
    {
        Some(proposal) => Target::Proposal(proposal.id.clone()),
        None => Target::Label(key.to_owned()),
    }
}

/// `key` with its prefix `from` replaced by `to`, when it is `from` or lies under it.
fn moved(key: &str, from: &str, to: &str) -> Option<unfold_core::Key> {
    let rest = key.strip_prefix(from)?;
    (rest.is_empty() || rest.starts_with('/')).then(|| unfold_core::Key::from_text(&format!("{to}{rest}")))
}

fn reconnect(page: &mut Page, graph: &BoardGraph, from: &str, to: &str) -> Result<(), BoardError> {
    let fits = if from.starts_with("entrance:") {
        graph.entrance(to).is_some()
    } else {
        graph.block(to).is_some()
    };
    if !fits {
        return Err(problem(
            page,
            target_of(page, from),
            "the new reference is not in the graph or is a different kind of reference",
        ));
    }
    // Proposed steps move with their card, and with their anchor: the same step under the new
    // key. One reconnected by its own anchor must still go after a step inside its card's block;
    // checked before anything changes.
    let mut anchors = Vec::new();
    for (index, proposal) in page.proposals.iter().enumerate() {
        let Some(after) = moved(proposal.after.as_str(), from, to) else {
            continue;
        };
        let card = page
            .card(&proposal.card)
            .and_then(|card| card.reference.block_key());
        if proposal.after.as_str() == from
            && let Some(card) = card
        {
            let card = if card.as_str() == from { to } else { card.as_str() };
            if let Some(message) = placement_problem(graph, card, after.as_str(), proposal.path) {
                return Err(problem(page, Target::Proposal(proposal.id.clone()), &message));
            }
        }
        anchors.push((index, after));
    }
    let new_key = unfold_core::Key::from_text(to);
    for card in &mut page.cards {
        if card.reference.key().as_str() == from {
            card.reference = card.reference.with_key(new_key.clone());
        }
    }
    for group in &mut page.groups {
        // A group already holding the new key just loses the old one: a repeated member would
        // make the saved file unreadable.
        if group.members.iter().any(|member| member.as_str() == to) {
            group.members.retain(|member| member.as_str() != from);
        } else {
            for member in &mut group.members {
                if member.as_str() == from {
                    *member = new_key.clone();
                }
            }
        }
    }
    if page.label_entry(from).is_some() {
        // The moved label replaces one the new key had: labels are unique per key.
        remove_label(page, to);
        if let Some(entry) = page.labels.iter_mut().find(|entry| entry.key.as_str() == from) {
            entry.key = new_key.clone();
        }
    }
    let reconnect_anchors = |anchors: &mut Vec<Anchor>| {
        for anchor in anchors.iter_mut() {
            if let Anchor::Key { key } = anchor
                && key.as_str() == from
            {
                *key = new_key.clone();
            }
        }
        // An anchor already citing the new key isn't repeated.
        let mut seen = Vec::with_capacity(anchors.len());
        anchors.retain(|anchor| {
            let fresh = !seen.contains(anchor);
            seen.push(anchor.clone());
            fresh
        });
    };
    for resource in &mut page.resources {
        if let Some(table) = resource.type_key.as_mut()
            && table.as_str() == from
        {
            *table = new_key.clone();
        }
        reconnect_anchors(&mut resource.anchors);
    }
    // Steps move with their card: the same statement or call under the new key.
    let mut steps = Vec::new();
    for link in &mut page.links {
        if let Some(step) = link.step.as_mut()
            && let Some(new_step) = moved(step.as_str(), from, to)
        {
            steps.push((std::mem::replace(step, new_step.clone()), new_step));
        }
        if let Some(carries) = link.carries.as_mut()
            && carries.as_str() == from
        {
            *carries = new_key.clone();
        }
        reconnect_anchors(&mut link.anchors);
    }
    for (index, after) in anchors {
        let anchor = &mut page.proposals[index].after;
        steps.push((std::mem::replace(anchor, after.clone()), after));
    }
    page.guards
        .retain(|entry| entry.key.as_str() != from && !steps.iter().any(|(old, _)| old == &entry.key));
    let fresh = std::iter::once(new_key).chain(steps.into_iter().map(|(_, step)| step));
    for key in fresh {
        if let Some(guard) = current_guard(graph, key.as_str()) {
            put_guard(page, guard);
        }
    }
    Ok(())
}

/// Replaces anchor `index` of resource card or connection `item` with `to`, which must be in
/// the graph, and saves a fresh guard for a key.
fn reconnect_anchor(
    page: &mut Page,
    graph: &BoardGraph,
    item: &str,
    index: usize,
    to: &Anchor,
) -> Result<(), BoardError> {
    let target = if page.resource(item).is_some() {
        Target::Resource(item.to_owned())
    } else {
        Target::Link(item.to_owned())
    };
    let exists = match to {
        Anchor::Key { key } => graph.exists(key.as_str()),
        Anchor::File { file, lines } => matches!(graph.lines(file, *lines), Lines::Bytes(_)),
    };
    if !exists {
        return Err(problem(
            page,
            target,
            "the new anchor is not in the graph: cite a block or entrance key, or lines an evidence file has",
        ));
    }
    let anchors = match &target {
        Target::Resource(_) => page
            .resources
            .iter_mut()
            .find(|resource| resource.id == item)
            .map(|resource| &mut resource.anchors),
        _ => page
            .links
            .iter_mut()
            .find(|link| link.id == item)
            .map(|link| &mut link.anchors),
    };
    let Some(slot) = anchors.and_then(|anchors| anchors.get_mut(index)) else {
        return Err(problem(page, target, "has no anchor at that position"));
    };
    *slot = to.clone();
    if let Anchor::Key { key } = to
        && let Some(guard) = current_guard(graph, key.as_str())
    {
        put_guard(page, guard);
    }
    Ok(())
}
