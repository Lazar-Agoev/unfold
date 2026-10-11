//! Checking a page against the current graphs: group validity, guarded references, and health
//! with its repairs (architecture §9.1, §10.5).
//!
//! A page's references are checked per project, each against that project's graph from
//! [`PageGraphs`], with the same rules. **Reconnecting** after a graph update is a matter of
//! looking keys up again: a surviving key reconnects; a missing one stays on the page as a
//! placeholder (health: missing); a guarded reference whose kind or owner changed, or a
//! position-based (ordinal) reference whose code changed, is held for review instead of silently
//! attaching its text to different code. A rename or move leaves the old key missing, and
//! [`Repair::Reconnect`] is the manual repair.
//!
//! A card from a project with no graph is **not connected**: a placeholder, never an error, and
//! its project's labels, groups, and guards are not checked. A drawn link is checked only for
//! its ends: it is **broken** when an end's card is missing from its connected project (a
//! resource or frame end is never missing); its label is never outdated (decision U-44).

use std::fmt;

use serde::Serialize;

use super::BoardError;
use super::graph::{BoardGraph, PageGraphs};
use super::items::remove_items;
use super::model::{Board, CardRef, Group, GuardEntry, Page, ProjectName, Signature, is_ordinal};

/// A Flow label's or a reference's address on a page: its key in a project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProjectKey {
    /// The project; absent for the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub project: Option<ProjectName>,
    /// The key.
    pub key: String,
}

impl ProjectKey {
    /// `key` in `project` (`None`: the board's own).
    pub fn new(project: Option<&ProjectName>, key: &str) -> Self {
        Self {
            project: project.cloned(),
            key: key.to_owned(),
        }
    }
}

/// What a health item or repair is about, on one page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// A card, by id.
    Card(String),
    /// A group, by id.
    Group(String),
    /// A Flow label, by project and block key.
    Label(ProjectKey),
    /// A drawn link, by id.
    Link(String),
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Card(id) => write!(f, "card `{id}`"),
            Self::Group(id) => write!(f, "group `{id}`"),
            Self::Label(ProjectKey { project: None, key }) => write!(f, "label of `{key}`"),
            Self::Label(ProjectKey {
                project: Some(project),
                key,
            }) => write!(f, "label of `{key}` in {project}"),
            Self::Link(id) => write!(f, "link `{id}`"),
        }
    }
}

/// The kind of a health problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum HealthKind {
    /// A reference is not in its project's graph (a placeholder until repaired).
    Missing,
    /// A description's or label's source changed since it was signed.
    Outdated,
    /// A group no longer meets the group rules.
    BrokenGroup,
    /// A guarded reference changed and is held for review.
    Review,
    /// A card's project is not connected (a placeholder: "From <name>, not connected").
    NotConnected,
    /// An end of a drawn link is a missing card.
    BrokenLink,
}

/// A repair the UI can offer for a health item; [`apply_repair`] performs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RepairKind {
    /// Point the reference at another key (after a rename or move).
    Reconnect,
    /// Remove the card (and its groups and links).
    RemoveCard,
    /// Remove the label.
    RemoveLabel,
    /// Remove the group, keeping its statements.
    Ungroup,
    /// Remove the drawn link.
    RemoveLink,
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
    /// The page it is on.
    pub page: String,
    /// What kind of problem.
    pub kind: HealthKind,
    /// What it is about.
    pub target: Target,
    /// The project of the reference involved; absent for the board's own (and for links).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub project: Option<ProjectName>,
    /// The reference involved, when it is not the target itself (a group member).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<String>,
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
    /// Every problem: per page, cards first, then groups, labels, and links.
    pub items: Vec<HealthItem>,
    /// The number of missing references.
    pub missing: u32,
    /// The number of outdated descriptions and labels.
    pub outdated: u32,
    /// The number of broken-group items.
    pub broken_groups: u32,
    /// The number of references held for review.
    pub review: u32,
    /// The number of cards from projects that are not connected.
    pub not_connected: u32,
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
            HealthKind::NotConnected => &mut self.not_connected,
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
            (self.not_connected, "not connected", "not connected"),
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

/// Why `group` breaks the group rules in `graph` (its card's project's graph), if it does: a
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

/// Why guarded reference (`project`, `key`) is held for review in `graph` (that project's), if
/// it is: its kind or owner changed, or it is position-based and its source changed. Missing
/// references are not held (they are placeholders), and references without a guard are never
/// held.
pub(crate) fn guard_problem(
    page: &Page,
    graph: &BoardGraph,
    project: Option<&ProjectName>,
    key: &str,
) -> Option<&'static str> {
    let saved = page.guard(project, key)?;
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

/// The guard to save for (`project`, `key`) now, from `graph` (that project's): its kind,
/// owner, and current signature.
pub fn current_guard(graph: &BoardGraph, project: Option<&ProjectName>, key: &str) -> Option<GuardEntry> {
    Some(GuardEntry {
        project: project.cloned(),
        key: unfold_core::Key::from_text(key),
        kind: graph.guard_kind(key)?,
        owner: graph.guard_owner(key)?,
        signature: graph.signature(key)?,
    })
}

/// Sets the guard of its (project, key) on `page`, replacing any saved one.
pub(crate) fn put_guard(page: &mut Page, guard: GuardEntry) {
    match page
        .guards
        .iter_mut()
        .find(|entry| entry.project == guard.project && entry.key == guard.key)
    {
        Some(entry) => *entry = guard,
        None => page.guards.push(guard),
    }
}

fn outdated(saved: Option<Signature>, current: Option<Signature>) -> bool {
    saved.is_some() && current != saved
}

/// Whether link end `end` is missing: gone from the page, or a card whose key is gone from its
/// connected project (a card of a project that is not connected is not missing, and neither is
/// a resource or frame of the page).
fn missing_end(page: &Page, graphs: &PageGraphs<'_>, end: &str) -> bool {
    match page.card(end) {
        None => page.resource(end).is_none() && page.frame(end).is_none(),
        Some(card) => graphs
            .get(card.reference.project())
            .is_some_and(|graph| !graph.exists(card.reference.key().as_str())),
    }
}

/// The page's health against `graphs`.
pub fn health(page: &Page, graphs: &PageGraphs<'_>) -> Health {
    let mut health = Health::default();
    let item = |kind,
                target: Target,
                project: Option<&ProjectName>,
                key: Option<&str>,
                message: &str,
                repairs: &[RepairKind]| HealthItem {
        page: page.id.clone(),
        kind,
        target,
        project: project.cloned(),
        key: key.map(str::to_owned),
        message: message.to_owned(),
        repairs: repairs.to_vec(),
    };
    for card in &page.cards {
        let key = card.reference.key().as_str();
        let project = card.reference.project();
        let target = Target::Card(card.id.clone());
        let Some(graph) = graphs.get(project) else {
            let name = project.map_or("", ProjectName::as_str);
            health.add(item(
                HealthKind::NotConnected,
                target,
                project,
                None,
                &format!("From {name}, not connected"),
                &[RepairKind::RemoveCard],
            ));
            continue;
        };
        if !graph.exists(key) {
            health.add(item(
                HealthKind::Missing,
                target,
                project,
                None,
                &format!("Code reference is missing: {key}"),
                &[RepairKind::Reconnect, RepairKind::RemoveCard],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(page, graph, project, key) {
            health.add(item(
                HealthKind::Review,
                target.clone(),
                project,
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
                project,
                None,
                CHANGED,
                &[RepairKind::Resign, RepairKind::RemoveDescription],
            ));
        }
    }
    for group in &page.groups {
        let project = page.group_project(group);
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        let target = Target::Group(group.id.clone());
        if let Some(problem) = group_problem(page, graph, group) {
            health.add(item(
                HealthKind::BrokenGroup,
                target,
                project,
                None,
                problem,
                &[RepairKind::Ungroup],
            ));
            continue;
        }
        for key in &group.members {
            if let Some(problem) = guard_problem(page, graph, project, key.as_str()) {
                health.add(item(
                    HealthKind::Review,
                    target.clone(),
                    project,
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
                    project,
                    None,
                    CHANGED,
                    &[RepairKind::Resign, RepairKind::RemoveDescription],
                ));
            }
        }
    }
    for entry in &page.labels {
        let project = entry.project.as_ref();
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        let key = entry.key.as_str();
        let target = Target::Label(ProjectKey::new(project, key));
        if graph.block(key).is_none() {
            health.add(item(
                HealthKind::Missing,
                target,
                project,
                None,
                &format!("Labeled block is missing: {key}"),
                &[RepairKind::Reconnect, RepairKind::RemoveLabel],
            ));
            continue;
        }
        if let Some(problem) = guard_problem(page, graph, project, key) {
            health.add(item(
                HealthKind::Review,
                target,
                project,
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
                project,
                None,
                LABEL_CHANGED,
                &[RepairKind::Resign, RepairKind::RemoveLabel],
            ));
        }
    }
    // Groups in one card may neither overlap nor nest.
    for (index, left) in page.groups.iter().enumerate() {
        let Some(graph) = graphs.get(page.group_project(left)) else {
            continue;
        };
        for right in &page.groups[index + 1..] {
            if left.card != right.card || !overlap(graph, left, right) {
                continue;
            }
            for (group, other) in [(left, right), (right, left)] {
                health.add(item(
                    HealthKind::BrokenGroup,
                    Target::Group(group.id.clone()),
                    page.group_project(group),
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
            .find(|end| missing_end(page, graphs, end))
        {
            health.add(item(
                HealthKind::BrokenLink,
                Target::Link(link.id.clone()),
                None,
                None,
                &format!("Linked card {end} is missing"),
                &[RepairKind::RemoveLink],
            ));
        }
    }
    health
}

/// The health of every page of `board`, in page order.
pub fn board_health(board: &Board, graphs: &PageGraphs<'_>) -> Health {
    let mut health = Health::default();
    for page in &board.pages {
        health.append(self::health(page, graphs));
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
    /// Replace reference `from` with `to` in `project` everywhere on the page: card references,
    /// group members, the label's key, and its guard (a fresh one is saved).
    Reconnect {
        /// The project (`None`: the board's own); it must be connected.
        project: Option<ProjectName>,
        /// The missing key.
        from: String,
        /// The key it should point at now.
        to: String,
    },
    /// Remove a card with what would dangle: the groups on it, the links drawn to or from it,
    /// its place in a frame, and the notes attached to it (they stay, unattached).
    RemoveCard(String),
    /// Remove a label.
    RemoveLabel(ProjectKey),
    /// Remove a group.
    Ungroup(String),
    /// Remove a drawn link (the notes attached to it stay, unattached).
    RemoveLink(String),
    /// Remove a card's or group's description, a label, or a link's label.
    RemoveDescription(Target),
    /// Sign a description, label, or link label against the current source, keeping its text.
    Resign(Target),
    /// Save a fresh guard for a reference, accepting its current code.
    Accept(ProjectKey),
}

fn problem(page: &Page, target: Target, message: &str) -> BoardError {
    BoardError::Problem {
        page: page.id.clone(),
        target,
        message: message.to_owned(),
    }
}

/// Applies `repair` to `page`. A repair that removes cards can change the board's
/// [`Board::projects`]: saving ([`super::prepare_save`]) refreshes it.
pub fn apply_repair(page: &mut Page, graphs: &PageGraphs<'_>, repair: &Repair) -> Result<(), BoardError> {
    match repair {
        Repair::Reconnect { project, from, to } => reconnect(page, graphs, project.as_ref(), from, to)?,
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
        Repair::RemoveLabel(label) => remove_label(page, label),
        Repair::Ungroup(id) => page.groups.retain(|group| &group.id != id),
        Repair::RemoveDescription(target) => match target {
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
                }
            }
        },
        Repair::Resign(target) => {
            let signature = super::save::current_signature(page, graphs, target)?;
            match super::save::description_mut(page, target) {
                Some(description) => description.signature = Some(signature),
                None => return Err(problem(page, target.clone(), "has no description to sign")),
            }
        }
        Repair::Accept(ProjectKey { project, key }) => {
            match graphs
                .get(project.as_ref())
                .and_then(|graph| current_guard(graph, project.as_ref(), key))
            {
                Some(guard) => put_guard(page, guard),
                None => return Err(BoardError::Unverified { what: key.clone() }),
            }
        }
    }
    Ok(())
}

fn remove_label(page: &mut Page, label: &ProjectKey) {
    page.labels
        .retain(|entry| !(entry.project == label.project && entry.key.as_str() == label.key));
}

fn reconnect(
    page: &mut Page,
    graphs: &PageGraphs<'_>,
    project: Option<&ProjectName>,
    from: &str,
    to: &str,
) -> Result<(), BoardError> {
    let entrance = from.starts_with("entrance:");
    let graph = graphs.get(project);
    let fits = graph.is_some_and(|graph| {
        if entrance {
            graph.entrance(to).is_some()
        } else {
            graph.block(to).is_some()
        }
    });
    if !fits {
        let target = page
            .cards
            .iter()
            .find(|card| card.reference.project() == project && card.reference.key().as_str() == from)
            .map_or_else(
                || Target::Label(ProjectKey::new(project, from)),
                |card| Target::Card(card.id.clone()),
            );
        let message = if graph.is_none() {
            "the reference's project is not connected"
        } else {
            "the new reference is not in the graph or is a different kind of reference"
        };
        return Err(problem(page, target, message));
    }
    let new_key = unfold_core::Key::from_text(to);
    for card in &mut page.cards {
        if card.reference.project() == project && card.reference.key().as_str() == from {
            card.reference = card.reference.with_key(new_key.clone());
        }
    }
    let in_project: Vec<String> = page
        .groups
        .iter()
        .filter(|group| page.group_project(group) == project)
        .map(|group| group.id.clone())
        .collect();
    for group in page
        .groups
        .iter_mut()
        .filter(|group| in_project.contains(&group.id))
    {
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
    if page.label_entry(project, from).is_some() {
        // The moved label replaces one the new key had: labels are unique per key.
        remove_label(page, &ProjectKey::new(project, to));
        if let Some(entry) = page
            .labels
            .iter_mut()
            .find(|entry| entry.project.as_ref() == project && entry.key.as_str() == from)
        {
            entry.key = new_key;
        }
    }
    page.guards
        .retain(|entry| !(entry.project.as_ref() == project && entry.key.as_str() == from));
    if let Some(guard) = graph.and_then(|graph| current_guard(graph, project, to)) {
        put_guard(page, guard);
    }
    Ok(())
}
