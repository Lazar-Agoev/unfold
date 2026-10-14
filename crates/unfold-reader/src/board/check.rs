//! Checking a page against the current graph: guarded references, the overlay rules for
//! combined steps, and health with its repairs.
//!
//! **Reconnecting** after a graph update is a matter of looking keys up again: a surviving key
//! reconnects; a missing one stays on the page (health: missing); a guarded reference whose kind
//! or owner changed, or a position-based reference whose code changed, is held for review
//! instead of silently attaching its text to different code. A rename or move leaves the old key
//! missing, and [`Repair::Reconnect`] is the repair. For a missing app or entrance, health offers
//! the one app or entrance that has the same `code` ([`HealthItem::reconnect`]).
//!
//! | Health | About |
//! | --- | --- |
//! | Missing | a card's, name's, or proposed step's key is not in the graph; a resource card's anchor or type is gone |
//! | Outdated | the code under a description, name, combined step, resource card, or connection changed since it was signed |
//! | Held for review | a guarded reference changed kind or owner, or is position-based and its code changed |
//! | Broken combined step | a combined step no longer follows the overlay rules ([`check_combined`], [`check_disjoint`]) |
//! | Broken link | a connection's end card, step, carried type, or anchor is gone |
//!
//! The claims (resource cards, connections, and frame descriptions) are checked by [`claims`].

use std::fmt;

use serde::Serialize;
use unfold_core::Key;
use unfold_core::key::{ParsedKey, Segment};

use super::BoardError;
use super::claims;
use super::graph::{BoardGraph, Lines, code_signature};
use super::items::remove_items;
use super::model::{Anchor, Board, CardRef, Combined, GuardEntry, Page, Signature};
use crate::overlay::{OverlayError, check_combined, check_disjoint};

/// The guard kind of an app or entrance.
pub(crate) const ENTRANCE: &str = "entrance";

/// What a health item or repair is about, on one page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// A card, by id.
    Card(String),
    /// A combined step, by id.
    Combined(String),
    /// A block's name, by block key.
    Name(String),
    /// A connection, by id.
    Connection(String),
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
            Self::Combined(id) => write!(f, "combined step `{id}`"),
            Self::Name(key) => write!(f, "name of `{key}`"),
            Self::Connection(id) => write!(f, "connection `{id}`"),
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
    /// A reference (a card's, a name's, a proposed step's place, or a resource card's anchor or
    /// type) is not in the graph.
    Missing,
    /// The source under a description, name, combined step, resource card, or connection
    /// changed since it was signed.
    Outdated,
    /// A guarded reference changed and is held for review.
    Review,
    /// A combined step no longer follows the overlay rules.
    BrokenCombined,
    /// An end of a connection is a missing card, or its step, carried type, or anchor is gone.
    BrokenLink,
}

/// A repair the UI can offer for a health item; [`apply_repair`] performs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RepairKind {
    /// Point the reference at another key (after a rename or move).
    Reconnect,
    /// Keep the text and sign it against the current source.
    Resign,
    /// Accept the reference's current code: save a fresh guard for it.
    Accept,
    /// Remove the item.
    Remove,
    /// Remove the item's description.
    RemoveDescription,
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
    /// The reference involved, when it is not the target itself (a combined step's member; a
    /// resource card's or connection's anchor, type, step, or carried type; a proposed step's
    /// place).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<String>,
    /// The position of the anchor involved among its resource card's or connection's anchors.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub anchor: Option<u32>,
    /// The key Reconnect offers, when exactly one fits: for a missing app or entrance, the app
    /// or entrance that has the same `code`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reconnect: Option<String>,
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
    /// Every problem: per page, cards first, then names, combined steps, connections, resource
    /// cards, frames, and proposed steps.
    pub items: Vec<HealthItem>,
    /// The number of missing references.
    pub missing: u32,
    /// The number of outdated texts and claims.
    pub outdated: u32,
    /// The number of references held for review.
    pub review: u32,
    /// The number of broken combined steps.
    pub broken_combined: u32,
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
            HealthKind::Review => &mut self.review,
            HealthKind::BrokenCombined => &mut self.broken_combined,
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
            (self.review, "to review", "to review"),
            (
                self.broken_combined,
                "broken combined step",
                "broken combined steps",
            ),
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

/// Why a guarded reference is held for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// Its kind or owner changed.
    KindOrOwner,
    /// It is position-based and its source changed.
    Position,
}

impl Held {
    /// The health message of a held reference.
    const fn message(self) -> &'static str {
        match self {
            Self::KindOrOwner => "Saved reference kind or owner changed; needs review",
            Self::Position => "Position-based reference changed; needs review before reconnecting",
        }
    }

    /// The health message of a proposed step whose place is held: where it goes needs review.
    const fn placement(self) -> &'static str {
        match self {
            Self::KindOrOwner => {
                "The block this proposal is placed by changed kind or owner; check where it goes, then accept the current code, reconnect it to another block, or remove the proposal"
            }
            Self::Position => {
                "The block this proposal is placed by is position-based and its code changed; check where it goes, then accept the current code, reconnect it to another block, or remove the proposal"
            }
        }
    }
}

/// Whether block key `key` names a block by its position somewhere along it (`if[0]`,
/// `callback[0]`, a repeated name or call), so it shifts when code is inserted before it.
fn by_position(key: &Key) -> bool {
    let Ok(ParsedKey::Block(block)) = key.parse() else {
        return false;
    };
    block.segments().iter().any(|segment| {
        matches!(
            segment,
            Segment::Name { repeat: Some(_), .. }
                | Segment::Form { ordinal: Some(_), .. }
                | Segment::Call { repeat: Some(_), .. }
                | Segment::Inline { .. }
        )
    })
}

/// Why guarded reference `key` is held for review in `graph`, if it is: its kind or owner
/// changed, or it is position-based and its source changed. Missing references are not held,
/// and references without a guard are never held.
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
    let changed = graph
        .guard_signature(key)
        .is_some_and(|signature| signature != saved.signature);
    (changed && by_position(&saved.key)).then_some(Held::Position)
}

/// The guard to save for `key` now, from `graph`: its kind, owner, and current guard signature.
pub fn current_guard(graph: &BoardGraph, key: &str) -> Option<GuardEntry> {
    Some(GuardEntry {
        key: Key::from_text(key),
        kind: graph.guard_kind(key)?,
        owner: graph.guard_owner(key)?,
        signature: graph.guard_signature(key)?,
    })
}

/// Sets the guard of its key on `page`, replacing any saved one.
pub(crate) fn put_guard(page: &mut Page, guard: GuardEntry) {
    match page.guards.iter_mut().find(|entry| entry.key == guard.key) {
        Some(entry) => *entry = guard,
        None => page.guards.push(guard),
    }
}

/// The key Reconnect offers for missing reference `key`: for an app or entrance, the one app or
/// entrance whose `code` is the `code` its guard recorded.
fn offer(page: &Page, graph: &BoardGraph, key: &str) -> Option<String> {
    let guard = page.guard(key).filter(|guard| guard.kind == ENTRANCE)?;
    let mut same = graph
        .starts()
        .filter(|start| code_signature(start) == guard.signature);
    let one = same.next()?;
    same.next().is_none().then(|| one.key().to_string())
}

/// The signature of combined step `combined`'s span now: from its first block's start to its
/// last block's end.
pub(crate) fn span_signature(graph: &BoardGraph, combined: &Combined) -> Option<Signature> {
    let (first, last) = (combined.members.first()?, combined.members.last()?);
    graph.span_signature(first.as_str(), last.as_str())
}

fn outdated(saved: Option<Signature>, current: Option<Signature>) -> bool {
    saved.is_some() && current != saved
}

/// Whether connection end `end` is missing: gone from the page, or a card whose key is gone
/// from the graph (a resource or frame of the page is never missing).
fn missing_end(page: &Page, graph: &BoardGraph, end: &str) -> bool {
    match page.card(end) {
        None => page.resource(end).is_none() && page.frame(end).is_none(),
        Some(card) => !graph.exists(card.reference.key().as_str()),
    }
}

/// Builds the health items of one page.
struct Report<'p> {
    page: &'p Page,
    health: Health,
}

impl Report<'_> {
    fn add(
        &mut self,
        kind: HealthKind,
        target: Target,
        key: Option<&str>,
        message: &str,
        repairs: &[RepairKind],
    ) {
        self.add_item(HealthItem {
            page: self.page.id.clone(),
            kind,
            target,
            key: key.map(str::to_owned),
            anchor: None,
            reconnect: None,
            message: message.to_owned(),
            repairs: repairs.to_vec(),
        });
    }

    fn add_item(&mut self, item: HealthItem) {
        self.health.add(item);
    }
}

/// The page's health against `graph`.
pub fn health(page: &Page, graph: &BoardGraph) -> Health {
    use RepairKind::{Accept, Reconnect, Remove, RemoveDescription, Resign};
    let mut report = Report {
        page,
        health: Health::default(),
    };
    for card in &page.cards {
        let key = card.reference.key().as_str();
        let target = Target::Card(card.id.clone());
        if !graph.exists(key) {
            report.add_item(HealthItem {
                page: page.id.clone(),
                kind: HealthKind::Missing,
                target,
                key: None,
                anchor: None,
                reconnect: offer(page, graph, key),
                message: format!("Code reference is missing: {key}"),
                repairs: vec![Reconnect, Remove],
            });
            continue;
        }
        if let Some(problem) = guard_problem(page, graph, key) {
            report.add(
                HealthKind::Review,
                target.clone(),
                None,
                problem,
                &[Accept, Reconnect, Remove],
            );
        }
        if let Some(description) = &card.description
            && outdated(description.signature, graph.signature(key))
        {
            report.add(
                HealthKind::Outdated,
                target,
                None,
                CHANGED,
                &[Resign, RemoveDescription],
            );
        }
    }
    for entry in &page.names {
        let key = entry.key.as_str();
        let target = Target::Name(key.to_owned());
        if graph.block(key).is_none() {
            report.add(
                HealthKind::Missing,
                target,
                None,
                &format!("Named block is missing: {key}"),
                &[Reconnect, Remove],
            );
        } else if let Some(problem) = guard_problem(page, graph, key) {
            report.add(
                HealthKind::Review,
                target,
                None,
                problem,
                &[Accept, Reconnect, Remove],
            );
        } else if outdated(entry.name.signature, graph.signature(key)) {
            report.add(
                HealthKind::Outdated,
                target,
                None,
                "Code changed since this name was written",
                &[Resign, Remove],
            );
        }
    }
    combined_health(&mut report, graph);
    for connection in &page.links {
        if let Some(end) = [&connection.from, &connection.to]
            .into_iter()
            .find(|end| missing_end(page, graph, end))
        {
            report.add(
                HealthKind::BrokenLink,
                Target::Connection(connection.id.clone()),
                None,
                &format!("Connected card {end} is missing"),
                &[Remove],
            );
        }
    }
    for item in claims::health(page, graph) {
        report.add_item(item);
    }
    for proposal in &page.proposals {
        let target = Target::Proposal(proposal.id.clone());
        let key = proposal.place.key().as_str();
        if graph.block(key).is_none() {
            report.add(
                HealthKind::Missing,
                target,
                Some(key),
                &format!(
                    "The block this proposal is placed by is missing: {key}. Reconnect it to the block it goes by now, or remove the proposal"
                ),
                &[Reconnect, Remove],
            );
        } else if let Some(held) = held(page, graph, key) {
            report.add(
                HealthKind::Review,
                target,
                Some(key),
                held.placement(),
                &[Accept, Reconnect, Remove],
            );
        }
    }
    report.health
}

/// The health of the page's combined steps: each follows the overlay rules ([`check_combined`]),
/// no two overlap or nest ([`check_disjoint`]), their members aren't held, and their span
/// hasn't changed since they were signed.
fn combined_health(report: &mut Report<'_>, graph: &BoardGraph) {
    use RepairKind::{Accept, Reconnect, Remove, RemoveDescription, Resign};
    let page = report.page;
    let mut valid: Vec<&Combined> = Vec::new();
    for combined in &page.combined {
        let target = Target::Combined(combined.id.clone());
        if let Err(error) = check_combined(graph, &combined.members) {
            let (key, repairs): (Option<&str>, &[RepairKind]) = match &error {
                OverlayError::Missing(key) => (Some(key.as_str()), &[Reconnect, Remove]),
                _ => (None, &[Remove]),
            };
            report.add(
                HealthKind::BrokenCombined,
                target,
                key,
                &capitalized(&error),
                repairs,
            );
            continue;
        }
        valid.push(combined);
        for key in &combined.members {
            if let Some(problem) = guard_problem(page, graph, key.as_str()) {
                report.add(
                    HealthKind::Review,
                    target.clone(),
                    Some(key.as_str()),
                    problem,
                    &[Accept, Reconnect, Remove],
                );
            }
        }
        let current = span_signature(graph, combined);
        let description = combined.description.as_ref();
        if outdated(combined.signature, current)
            || description.is_some_and(|description| outdated(description.signature, current))
        {
            let mut repairs = vec![Resign];
            repairs.extend(description.map(|_| RemoveDescription));
            repairs.push(Remove);
            report.add(
                HealthKind::Outdated,
                target,
                None,
                "Code changed since these steps were combined",
                &repairs,
            );
        }
    }
    for (index, left) in valid.iter().enumerate() {
        for right in &valid[index + 1..] {
            if let Err(error) = check_disjoint(graph, &[&left.members, &right.members]) {
                for (step, other) in [(left, right), (right, left)] {
                    report.add(
                        HealthKind::BrokenCombined,
                        Target::Combined(step.id.clone()),
                        None,
                        &format!("{} (with combined step `{}`)", capitalized(&error), other.id),
                        &[Remove],
                    );
                }
            }
        }
    }
}

/// An overlay error as a sentence.
fn capitalized(error: &OverlayError) -> String {
    let text = error.to_string();
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// The health of every page of `board`, in page order.
pub fn board_health(board: &Board, graph: &BoardGraph) -> Health {
    let mut health = Health::default();
    for page in &board.pages {
        health.append(self::health(page, graph));
    }
    health
}

/// A repair with everything it needs (the page is the caller's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// Replace reference `from` with `to` everywhere on the page: card references, opened calls,
    /// names, combined steps' members, proposed steps' places, resource cards' types and
    /// anchors, and connections' steps, carried types, and anchors; a fresh guard is saved.
    /// Only the key itself moves: a key inside a reconnected card is a reference of its own.
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
    /// Sign a description, name, combined step, resource card, or connection (with its texts)
    /// against the current source, keeping its text.
    Resign(Target),
    /// Save a fresh guard for a reference key, accepting its current code.
    Accept(String),
    /// Remove an item. A card, resource card, or frame goes with what would dangle: the
    /// connections to and from it and its place in a frame; the notes attached to it stay,
    /// unattached.
    Remove(Target),
    /// Remove the description of a card, combined step, resource card, connection, or frame.
    RemoveDescription(Target),
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
        Repair::Resign(target) => super::save::resign(page, graph, target)?,
        Repair::Accept(key) => match current_guard(graph, key) {
            Some(guard) => put_guard(page, guard),
            None => return Err(BoardError::Unverified { what: key.clone() }),
        },
        Repair::Remove(Target::Name(key)) => page.names.retain(|entry| entry.key.as_str() != key),
        Repair::Remove(
            Target::Card(id)
            | Target::Combined(id)
            | Target::Connection(id)
            | Target::Resource(id)
            | Target::Frame(id)
            | Target::Proposal(id),
        ) => remove_items(page, &[id]),
        Repair::RemoveDescription(target) => remove_description(page, target),
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
        Target::Combined(id) => {
            if let Some(combined) = page.combined.iter_mut().find(|combined| &combined.id == id) {
                combined.description = None;
            }
        }
        Target::Connection(id) => {
            if let Some(connection) = page.links.iter_mut().find(|connection| &connection.id == id) {
                connection.description = None;
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
        // A name is its own text, and a proposed step's title and reason are the proposal.
        Target::Name(_) | Target::Proposal(_) => {}
    }
}

/// What a repair about reference `key` is about: the card showing it, else its name, else the
/// combined step holding it, else the resource card or connection citing it, else the proposed
/// step placed by it.
fn target_of(page: &Page, key: &str) -> Target {
    let is = |known: Option<&Key>| known.is_some_and(|known| known.as_str() == key);
    let cited = |anchors: &[Anchor]| anchors.iter().any(|anchor| is(anchor.key()));
    if let Some(card) = page.cards.iter().find(|card| is(Some(card.reference.key()))) {
        return Target::Card(card.id.clone());
    }
    if page.name_entry(key).is_some() {
        return Target::Name(key.to_owned());
    }
    if let Some(combined) = page
        .combined
        .iter()
        .find(|combined| combined.members.iter().any(|member| is(Some(member))))
    {
        return Target::Combined(combined.id.clone());
    }
    if let Some(resource) = page
        .resources
        .iter()
        .find(|resource| is(resource.type_key.as_ref()) || cited(&resource.anchors))
    {
        return Target::Resource(resource.id.clone());
    }
    if let Some(connection) = page.links.iter().find(|connection| {
        is(connection.step.as_ref()) || is(connection.carries.as_ref()) || cited(&connection.anchors)
    }) {
        return Target::Connection(connection.id.clone());
    }
    match page
        .proposals
        .iter()
        .find(|proposal| is(Some(proposal.place.key())))
    {
        Some(proposal) => Target::Proposal(proposal.id.clone()),
        None => Target::Name(key.to_owned()),
    }
}

/// Keeps the first of each repeated item.
fn first_of_each<T: PartialEq + Clone>(items: &mut Vec<T>) {
    let mut seen: Vec<T> = Vec::with_capacity(items.len());
    items.retain(|item| {
        let fresh = !seen.contains(item);
        seen.push(item.clone());
        fresh
    });
}

fn reconnect(page: &mut Page, graph: &BoardGraph, from: &str, to: &str) -> Result<(), BoardError> {
    let is = |key: &Key| key.as_str() == from;
    let shown_as_start = page
        .cards
        .iter()
        .any(|card| matches!(&card.reference, CardRef::Entrance { entrance } if is(entrance)));
    let needs_block = page
        .cards
        .iter()
        .any(|card| card.reference.block_key().is_some_and(is) || card.open.iter().flatten().any(is))
        || page.names.iter().any(|entry| is(&entry.key))
        || page
            .combined
            .iter()
            .any(|combined| combined.members.iter().any(is))
        || page.proposals.iter().any(|proposal| is(proposal.place.key()))
        || page
            .resources
            .iter()
            .any(|resource| resource.type_key.as_ref().is_some_and(is))
        || page.links.iter().any(|connection| {
            connection.step.as_ref().is_some_and(is) || connection.carries.as_ref().is_some_and(is)
        });
    let fits = graph.exists(to)
        && (!shown_as_start || graph.entrance(to).is_some())
        && (!needs_block || graph.block(to).is_some());
    if !fits {
        return Err(problem(
            page,
            target_of(page, from),
            "the new reference is not in the graph or is a different kind of reference",
        ));
    }
    let new_key = Key::from_text(to);
    let replace = |key: &mut Key| {
        if is(key) {
            *key = new_key.clone();
        }
    };
    for card in &mut page.cards {
        if is(card.reference.key()) {
            card.reference = card.reference.with_key(new_key.clone());
        }
        for path in &mut card.open {
            path.iter_mut().for_each(replace);
        }
        first_of_each(&mut card.open);
    }
    if page.name_entry(from).is_some() {
        // The moved name replaces one the new key had: names are unique per key.
        page.names.retain(|entry| entry.key.as_str() != to);
        page.names.iter_mut().for_each(|entry| replace(&mut entry.key));
    }
    for combined in &mut page.combined {
        combined.members.iter_mut().for_each(replace);
        first_of_each(&mut combined.members);
    }
    for proposal in &mut page.proposals {
        if is(proposal.place.key()) {
            proposal.place = proposal.place.with_key(new_key.clone());
        }
    }
    let reconnect_anchors = |anchors: &mut Vec<Anchor>| {
        for anchor in anchors.iter_mut() {
            if let Anchor::Key { key } = anchor {
                replace(key);
            }
        }
        first_of_each(anchors);
    };
    for resource in &mut page.resources {
        resource.type_key.iter_mut().for_each(replace);
        reconnect_anchors(&mut resource.anchors);
    }
    for connection in &mut page.links {
        connection.step.iter_mut().for_each(replace);
        connection.carries.iter_mut().for_each(replace);
        reconnect_anchors(&mut connection.anchors);
    }
    page.guards.retain(|entry| entry.key.as_str() != from);
    if let Some(guard) = current_guard(graph, to) {
        put_guard(page, guard);
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
        Target::Connection(item.to_owned())
    };
    let exists = match to {
        Anchor::Key { key } => graph.exists(key.as_str()),
        Anchor::File { file, lines } => matches!(graph.lines(file, *lines), Lines::Bytes(_)),
    };
    if !exists {
        return Err(problem(
            page,
            target,
            "the new anchor is not in the graph: cite a block, app, or entrance key, or lines an evidence file has",
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
            .find(|connection| connection.id == item)
            .map(|connection| &mut connection.anchors),
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
