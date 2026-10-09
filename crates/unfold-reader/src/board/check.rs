//! Checking a board against the current graph: group validity, guarded references, and board
//! health with its repairs (architecture §9.1, §10.5).
//!
//! **Reconnecting** after a graph update is a matter of looking keys up again: a surviving key
//! reconnects; a missing one stays on the board as a placeholder (health: missing); a guarded
//! reference whose kind or owner changed, or a position-based (ordinal) reference whose code
//! changed, is held for review instead of silently attaching its text to different code. A
//! rename or move leaves the old key missing, and [`Repair::Reconnect`] is the manual repair.

use serde::Serialize;

use super::BoardError;
use super::graph::BoardGraph;
use super::model::{Board, CardRef, Group, Guard, Signature, is_ordinal};

/// What a health item or repair is about.
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
}

/// The kind of a health problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum HealthKind {
    /// A reference is not in the graph (a placeholder until repaired).
    Missing,
    /// A description's or label's source changed since it was signed.
    Outdated,
    /// A group no longer meets the group rules.
    BrokenGroup,
    /// A guarded reference changed and is held for review.
    Review,
}

/// A repair the UI can offer for a health item; [`apply_repair`] performs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RepairKind {
    /// Point the reference at another key (after a rename or move).
    Reconnect,
    /// Remove the card (and its groups).
    RemoveCard,
    /// Remove the label.
    RemoveLabel,
    /// Remove the group, keeping its statements.
    Ungroup,
    /// Remove the description.
    RemoveDescription,
    /// Keep the text and sign it against the current source.
    Resign,
    /// Accept the reference's current code: save a fresh guard for it.
    Accept,
}

/// One health problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct HealthItem {
    /// What kind of problem.
    pub kind: HealthKind,
    /// What it is about.
    pub target: Target,
    /// The reference involved, when it is not the target itself (a group member).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<String>,
    /// The explanation.
    pub message: String,
    /// The repairs to offer.
    pub repairs: Vec<RepairKind>,
}

/// A board's health.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// Every problem, cards first, then groups, then labels.
    pub items: Vec<HealthItem>,
    /// The number of missing references.
    pub missing: u32,
    /// The number of outdated descriptions and labels.
    pub outdated: u32,
    /// The number of broken-group items.
    pub broken_groups: u32,
    /// The number of references held for review.
    pub review: u32,
}

impl Health {
    /// Whether there are no problems.
    pub fn is_healthy(&self) -> bool {
        self.items.is_empty()
    }

    fn add(&mut self, item: HealthItem) {
        if self.items.iter().any(|known| {
            known.kind == item.kind && known.target == item.target && known.message == item.message
        }) {
            return;
        }
        match item.kind {
            HealthKind::Missing => self.missing += 1,
            HealthKind::Outdated => self.outdated += 1,
            HealthKind::BrokenGroup => self.broken_groups += 1,
            HealthKind::Review => self.review += 1,
        }
        self.items.push(item);
    }

    /// Whether `target` has a problem of `kind`.
    pub fn has(&self, kind: HealthKind, target: &Target) -> bool {
        self.items
            .iter()
            .any(|item| item.kind == kind && &item.target == target)
    }
}

const CHANGED: &str = "Code changed since this description was written";
const LABEL_CHANGED: &str = "Code changed since this label was written";

/// Why `group` breaks the group rules in the current graph, if it does: a run of consecutive
/// sibling statements with one parent and one body role (`body`, `then`, `else`, `catch`,
/// `finally`), inside its block card, never crossing a body boundary.
pub fn group_problem(board: &Board, graph: &BoardGraph, group: &Group) -> Option<&'static str> {
    let Some(CardRef::Block(card_key)) = board.card(&group.card).map(|card| &card.reference) else {
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

/// Why guarded reference `key` is held for review, if it is: its kind or owner changed, or it
/// is position-based and its source changed. Missing references are not held (they are
/// placeholders), and references without a guard are never held.
pub fn guard_problem(board: &Board, graph: &BoardGraph, key: &str) -> Option<&'static str> {
    let saved = board.guards.get(key)?;
    let kind = graph.guard_kind(key)?;
    let owner = graph.guard_owner(key)?;
    if kind != saved.kind || owner != saved.owner {
        return Some("Saved reference kind or owner changed; needs review");
    }
    let signature = graph.signature(key);
    if is_ordinal(key) && signature.is_some_and(|signature| signature != saved.signature) {
        return Some("Position-based reference changed; needs review before reconnecting");
    }
    None
}

/// The guard to save for `key` now: its kind, owner, and current signature.
pub fn current_guard(graph: &BoardGraph, key: &str) -> Option<Guard> {
    Some(Guard {
        kind: graph.guard_kind(key)?,
        owner: graph.guard_owner(key)?,
        signature: graph.signature(key)?,
    })
}

fn outdated(saved: Option<Signature>, current: Option<Signature>) -> bool {
    saved.is_some() && current != saved
}

/// The board's health against `graph`.
pub fn health(board: &Board, graph: &BoardGraph) -> Health {
    let mut health = Health::default();
    let item = |kind, target: Target, key: Option<&str>, message: &str, repairs: &[RepairKind]| HealthItem {
        kind,
        target,
        key: key.map(str::to_owned),
        message: message.to_owned(),
        repairs: repairs.to_vec(),
    };
    for card in &board.cards {
        let key = card.reference.key().as_str();
        let target = Target::Card(card.id.clone());
        if !graph.exists(key) {
            let message = format!("Code reference is missing: {key}");
            health.add(item(
                HealthKind::Missing,
                target,
                None,
                &message,
                &[RepairKind::Reconnect, RepairKind::RemoveCard],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(board, graph, key) {
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
    for group in &board.groups {
        let target = Target::Group(group.id.clone());
        if let Some(problem) = group_problem(board, graph, group) {
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
            if let Some(problem) = guard_problem(board, graph, key.as_str()) {
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
    for (key, label) in &board.labels {
        let target = Target::Label(key.clone());
        if graph.block(key).is_none() {
            let message = format!("Labeled block is missing: {key}");
            health.add(item(
                HealthKind::Missing,
                target,
                None,
                &message,
                &[RepairKind::Reconnect, RepairKind::RemoveLabel],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(board, graph, key) {
            health.add(item(
                HealthKind::Review,
                target,
                None,
                problem,
                &[RepairKind::Accept, RepairKind::RemoveLabel],
            ));
            continue;
        }
        if outdated(label.signature, graph.signature(key)) {
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
    for (index, left) in board.groups.iter().enumerate() {
        for right in &board.groups[index + 1..] {
            if left.card != right.card || !overlap(graph, left, right) {
                continue;
            }
            for (group, other) in [(left, right), (right, left)] {
                let message = format!("Groups overlap or nest with {}", other.id);
                health.add(item(
                    HealthKind::BrokenGroup,
                    Target::Group(group.id.clone()),
                    None,
                    &message,
                    &[RepairKind::Ungroup],
                ));
            }
        }
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

/// Checks that every reference exists and every group is valid, as a board written for the
/// current graph must be. Loading is lenient instead: it keeps missing references as
/// placeholders and reports them through [`health`].
pub fn check(board: &Board, graph: &BoardGraph) -> Result<(), BoardError> {
    for card in &board.cards {
        let key = card.reference.key().as_str();
        let found = match &card.reference {
            CardRef::Block(_) => graph.block(key).is_some(),
            CardRef::Entrance(_) => graph.entrance(key).is_some(),
        };
        if !found {
            return Err(BoardError::Problem {
                target: Target::Card(card.id.clone()),
                message: format!("unknown key {key}"),
            });
        }
    }
    for key in board.labels.keys() {
        if graph.block(key).is_none() {
            return Err(BoardError::Problem {
                target: Target::Label(key.clone()),
                message: format!("unknown key {key}"),
            });
        }
    }
    for group in &board.groups {
        if let Some(problem) = group_problem(board, graph, group) {
            return Err(BoardError::Problem {
                target: Target::Group(group.id.clone()),
                message: problem.to_owned(),
            });
        }
    }
    let health = health(board, graph);
    match health
        .items
        .iter()
        .find(|item| item.kind == HealthKind::BrokenGroup)
    {
        Some(item) => Err(BoardError::Problem {
            target: item.target.clone(),
            message: item.message.clone(),
        }),
        None => Ok(()),
    }
}

/// A repair with everything it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// Replace reference `from` with `to` everywhere: card references, group members, the
    /// label's key, and its guard (a fresh one is saved).
    Reconnect {
        /// The missing key.
        from: String,
        /// The key it should point at now.
        to: String,
    },
    /// Remove a card and the groups on it.
    RemoveCard(String),
    /// Remove a label.
    RemoveLabel(String),
    /// Remove a group.
    Ungroup(String),
    /// Remove a card's or group's description.
    RemoveDescription(Target),
    /// Sign a description or label against the current source, keeping its text.
    Resign(Target),
    /// Save a fresh guard for reference `key`, accepting its current code.
    Accept(String),
}

/// Applies `repair` to `board`.
pub fn apply_repair(board: &mut Board, graph: &BoardGraph, repair: &Repair) -> Result<(), BoardError> {
    let problem = |target: Target, message: &str| BoardError::Problem {
        target,
        message: message.to_owned(),
    };
    match repair {
        Repair::Reconnect { from, to } => {
            let entrance = from.starts_with("entrance:");
            let fits = if entrance {
                graph.entrance(to).is_some()
            } else {
                graph.block(to).is_some()
            };
            if !fits {
                let target = board
                    .cards
                    .iter()
                    .find(|card| card.reference.key().as_str() == from)
                    .map_or_else(
                        || Target::Label(from.clone()),
                        |card| Target::Card(card.id.clone()),
                    );
                return Err(problem(
                    target,
                    "the new reference is not in the graph or is a different kind of reference",
                ));
            }
            let new_key = unfold_core::Key::from_text(to);
            for card in &mut board.cards {
                if card.reference.key().as_str() == from {
                    card.reference = if entrance {
                        CardRef::Entrance(new_key.clone())
                    } else {
                        CardRef::Block(new_key.clone())
                    };
                }
            }
            for group in &mut board.groups {
                // A group already holding the new key just loses the old one: a repeated member
                // would make the saved file unreadable.
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
            if let Some(label) = board.labels.remove(from) {
                board.labels.insert(to.clone(), label);
            }
            board.guards.remove(from);
            if let Some(guard) = current_guard(graph, to) {
                board.guards.insert(to.clone(), guard);
            }
        }
        Repair::RemoveCard(id) => {
            board.cards.retain(|card| &card.id != id);
            board.groups.retain(|group| &group.card != id);
        }
        Repair::RemoveLabel(key) => {
            board.labels.remove(key);
        }
        Repair::Ungroup(id) => board.groups.retain(|group| &group.id != id),
        Repair::RemoveDescription(target) => match target {
            Target::Card(id) => {
                if let Some(card) = board.cards.iter_mut().find(|card| &card.id == id) {
                    card.description = None;
                }
            }
            Target::Group(id) => {
                if let Some(group) = board.groups.iter_mut().find(|group| &group.id == id) {
                    group.description = None;
                }
            }
            Target::Label(key) => {
                board.labels.remove(key);
            }
        },
        Repair::Resign(target) => {
            let signature = super::save::current_signature(board, graph, target)?;
            let description = match target {
                Target::Card(id) => board
                    .cards
                    .iter_mut()
                    .find(|card| &card.id == id)
                    .and_then(|card| card.description.as_mut()),
                Target::Group(id) => board
                    .groups
                    .iter_mut()
                    .find(|group| &group.id == id)
                    .and_then(|group| group.description.as_mut()),
                Target::Label(key) => board.labels.get_mut(key),
            };
            match description {
                Some(description) => description.signature = Some(signature),
                None => return Err(problem(target.clone(), "has no description to sign")),
            }
        }
        Repair::Accept(key) => match current_guard(graph, key) {
            Some(guard) => {
                board.guards.insert(key.clone(), guard);
            }
            None => {
                return Err(BoardError::Unverified { what: key.clone() });
            }
        },
    }
    Ok(())
}
