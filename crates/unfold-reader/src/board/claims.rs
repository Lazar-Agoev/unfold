//! Authored claims: resource cards and connections (links with a `kind`) cite code, and frame
//! descriptions describe their members. This module says what each claim is signed over, checks
//! what it cites against the graphs (health), and grounds the claims an agent adds or changes
//! (at save).
//!
//! A claim cites **parts**: a resource card its table's type, then its anchors; a connection its
//! step, then its anchors. An anchor is a block or entrance key, or lines of an evidence file.
//!
//! | Claim | Signed over |
//! | --- | --- |
//! | Resource card | Its type's block range (a table backed by a type), then each anchor's bytes, each followed by `\n` |
//! | Connection | Its step's block range, then each anchor's bytes, each followed by `\n` |
//! | Frame description | Its member cards' signatures (direct and nested, sorted by card id), hex, joined with `\n` |
//!
//! An anchor's bytes are a block's range, an entrance's evidence, or an evidence file's lines
//! (from the start of the first through the end of the last), always verified against the hash
//! the graph recorded. A resource card's description and a connection's description and label
//! are signed over the claim's bytes; a claim that cites nothing is not signed.
//!
//! **Grounding.** A save checks every claim that is new or changed (in what it says or cites,
//! not its position or text): a table backed by a type marks only the type's fields (inherited
//! ones included), and a `relates` connection joins only columns its tables have. A claim an
//! agent wrote must also be grounded in the code it cites:
//!
//! - a resource card other than an `actor` has an anchor, and so does an `http`, `rpc`, `queue`,
//!   `event`, or `calls` connection;
//! - a resource card's name and the columns a table lists itself appear in its cited bytes;
//! - a connection's channel appears in its step's or anchors' bytes, unless its `to` end is an
//!   entrance, whose channel the save fills in (a route's `METHOD path`, a worker's queue);
//! - a carried type is used (by a type link) inside the step's function or an anchor's block;
//! - a `calls` connection leaves from a call Unfold can't follow (unresolved or external).
//!
//! A refusal says what failed, why, and what to do instead.

use rustc_hash::FxHashSet;
use unfold_core::RepoPath;
use unfold_core::vocab::{BlockKind, EntranceKind, Resolution};

use super::BoardError;
use super::check::{HealthItem, HealthKind, RepairKind, Target, guard_problem};
use super::graph::{BoardGraph, Lines, PageGraphs};
use super::model::{
    Anchor, AuthorKind, Card, CardRef, Frame, Link, LinkKind, Page, ProjectName, Resource, ResourceKind,
    Signature,
};
use crate::mermaid::entrance_title;

/// The connection kinds that need an anchor for their receiving side when an agent draws them.
const ANCHORED: &[LinkKind] = &[
    LinkKind::Http,
    LinkKind::Rpc,
    LinkKind::Queue,
    LinkKind::Event,
    LinkKind::Calls,
];

/// The most field names a refusal lists.
const LISTED_FIELDS: usize = 12;

/// One thing a claim cites, in a project (`None`: the board's own).
#[derive(Debug, Clone, Copy)]
enum Part<'p> {
    /// A block or entrance key.
    Key(Option<&'p ProjectName>, &'p str),
    /// Lines of an evidence file.
    Lines(Option<&'p ProjectName>, &'p RepoPath, [u32; 2]),
}

impl<'p> Part<'p> {
    fn of(anchor: &'p Anchor) -> Self {
        match anchor {
            Anchor::Key { key, project } => Self::Key(project.as_ref(), key.as_str()),
            Anchor::File { file, lines, project } => Self::Lines(project.as_ref(), file, *lines),
        }
    }

    fn project(self) -> Option<&'p ProjectName> {
        match self {
            Self::Key(project, _) | Self::Lines(project, ..) => project,
        }
    }
}

/// What resource card `resource` cites: its type, then its anchors.
fn resource_parts(resource: &Resource) -> Vec<Part<'_>> {
    let table = resource
        .type_key
        .iter()
        .map(|key| Part::Key(resource.project.as_ref(), key.as_str()));
    table.chain(resource.anchors.iter().map(Part::of)).collect()
}

/// What connection `link` of `page` cites: its step, then its anchors (nothing for a drawn link).
fn connection_parts<'p>(page: &'p Page, link: &'p Link) -> Vec<Part<'p>> {
    if link.kind.is_none() {
        return Vec::new();
    }
    let step = link
        .step
        .iter()
        .map(|key| Part::Key(page.step_project(link), key.as_str()));
    step.chain(link.anchors.iter().map(Part::of)).collect()
}

/// Whether link `link` of `page` is a connection that cites code (a step or anchors): its texts
/// are signed over that code, not over its ends.
pub(crate) fn cites(page: &Page, link: &Link) -> bool {
    !connection_parts(page, link).is_empty()
}

/// What one part's bytes are now.
enum Found<'g> {
    Bytes(&'g [u8]),
    Missing,
    Unverified,
    NotConnected,
}

fn found<'g>(graphs: &PageGraphs<'g>, part: Part<'_>) -> Found<'g> {
    let Some(graph) = graphs.get(part.project()) else {
        return Found::NotConnected;
    };
    match part {
        Part::Key(_, key) if !graph.exists(key) => Found::Missing,
        Part::Key(_, key) => graph.key_bytes(key).map_or(Found::Unverified, Found::Bytes),
        Part::Lines(_, file, lines) => match graph.lines(file, lines) {
            Lines::Bytes(bytes) => Found::Bytes(bytes),
            Lines::Unverified => Found::Unverified,
            Lines::NotRecorded | Lines::PastEnd => Found::Missing,
        },
    }
}

/// A claim's signature now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signed {
    /// It cites nothing to sign.
    Nothing,
    /// Its parts verify: their signature.
    Signature(Signature),
    /// A part is in a project that is not connected: kept as it is.
    NotConnected,
    /// A part is gone from its graph.
    Missing,
    /// A part's bytes don't verify against the graph's hash.
    Unverified,
}

impl Signed {
    fn signature(self) -> Option<Signature> {
        match self {
            Self::Signature(signature) => Some(signature),
            _ => None,
        }
    }
}

/// The signature over `parts`' bytes, each followed by `\n`.
fn signed(graphs: &PageGraphs<'_>, parts: &[Part<'_>]) -> Signed {
    if parts.is_empty() {
        return Signed::Nothing;
    }
    let mut bytes = Vec::new();
    let mut problem = None;
    for part in parts {
        match found(graphs, *part) {
            Found::Bytes(found) => {
                bytes.extend_from_slice(found);
                bytes.push(b'\n');
            }
            Found::NotConnected => return Signed::NotConnected,
            Found::Missing => problem = Some(Signed::Missing),
            Found::Unverified => problem = problem.or(Some(Signed::Unverified)),
        }
    }
    problem.unwrap_or_else(|| Signed::Signature(Signature::of(&bytes)))
}

/// Resource card `resource`'s signature now.
pub(crate) fn resource_signed(graphs: &PageGraphs<'_>, resource: &Resource) -> Signed {
    signed(graphs, &resource_parts(resource))
}

/// Connection `link`'s signature now ([`Signed::Nothing`] for a drawn link).
pub(crate) fn connection_signed(page: &Page, graphs: &PageGraphs<'_>, link: &Link) -> Signed {
    signed(graphs, &connection_parts(page, link))
}

/// The cards of frame `frame`, directly and in the frames it holds, sorted by id.
fn member_cards<'p>(page: &'p Page, frame: &'p Frame) -> Vec<&'p Card> {
    let mut cards: Vec<&Card> = Vec::new();
    let mut frames = vec![frame];
    let mut seen = FxHashSet::default();
    while let Some(frame) = frames.pop() {
        if !seen.insert(frame.id.as_str()) {
            continue;
        }
        for member in &frame.members {
            match (page.card(member), page.frame(member)) {
                (Some(card), _) => cards.push(card),
                (None, Some(inner)) => frames.push(inner),
                (None, None) => {}
            }
        }
    }
    cards.sort_by(|a, b| a.id.cmp(&b.id));
    cards.dedup_by(|a, b| a.id == b.id);
    cards
}

/// The signature frame `frame`'s description gets now: over its member cards' signatures.
pub(crate) fn frame_signed(page: &Page, graphs: &PageGraphs<'_>, frame: &Frame) -> Signed {
    let mut hexes = Vec::new();
    let mut problem = None;
    for card in member_cards(page, frame) {
        let Some(graph) = graphs.get(card.reference.project()) else {
            return Signed::NotConnected;
        };
        let key = card.reference.key().as_str();
        match graph.signature(key) {
            Some(signature) => hexes.push(signature.0.to_string()),
            None if graph.exists(key) => problem = problem.or(Some(Signed::Unverified)),
            None => problem = Some(Signed::Missing),
        }
    }
    problem.unwrap_or_else(|| Signed::Signature(Signature::of(hexes.join("\n").as_bytes())))
}

/// The channel a save fills in for connection `link`: its `to` end's route (`METHOD path`) for
/// an `http` connection, or its worker's queue for a `queue` connection, when that end is such
/// an entrance card.
pub(crate) fn entrance_channel(page: &Page, graphs: &PageGraphs<'_>, link: &Link) -> Option<String> {
    let card = page.card(&link.to)?;
    let CardRef::Entrance { entrance: key, .. } = &card.reference else {
        return None;
    };
    let entrance = graphs.get(card.reference.project())?.entrance(key.as_str())?;
    let trigger = entrance.trigger.as_deref()?;
    match (link.kind?, entrance.kind) {
        (LinkKind::Http, EntranceKind::Http) => entrance
            .method
            .map(|method| format!("{} {trigger}", method.as_str())),
        (LinkKind::Queue, EntranceKind::Worker) => Some(trigger.to_owned()),
        _ => None,
    }
}

/// A health item about a claim.
fn item(
    page: &Page,
    kind: HealthKind,
    target: &Target,
    part: Option<(Option<&ProjectName>, &str)>,
    anchor: Option<usize>,
    message: String,
    repairs: &[RepairKind],
) -> HealthItem {
    HealthItem {
        page: page.id.clone(),
        kind,
        target: target.clone(),
        project: part.and_then(|(project, _)| project.cloned()),
        key: part.map(|(_, key)| key.to_owned()),
        anchor: anchor.and_then(|index| u32::try_from(index).ok()),
        message,
        repairs: repairs.to_vec(),
    }
}

/// Why `anchor` is gone from `graph`, if it is; `claim` names what cites it ("resource card",
/// "connection").
fn anchor_gone(graph: &BoardGraph, anchor: &Anchor, claim: &str) -> Option<String> {
    match anchor {
        Anchor::Key { key, .. } => (!graph.exists(key.as_str())).then(|| {
            format!(
                "Anchor {key} is not in the graph: the code was renamed, moved, or removed. Reconnect it to the code's new key, or remove the {claim}"
            )
        }),
        Anchor::File {
            file,
            lines: [first, last],
            ..
        } => match graph.lines(file, [*first, *last]) {
            Lines::NotRecorded => Some(format!(
                "Anchor {file} is not an evidence file of the project: list it under `[evidence] files` in .unfold/config.toml and compile, or reconnect the anchor"
            )),
            Lines::PastEnd => Some(format!(
                "Anchor {file} has no lines {first}–{last}: the file got shorter. Reconnect the anchor to the lines it means now"
            )),
            Lines::Bytes(_) | Lines::Unverified => None,
        },
    }
}

/// Why type `key` (a table's type, or a connection's carried type: its `role`) is gone from
/// `graph`, if it is; `claim` names what cites it.
fn type_gone(graph: &BoardGraph, key: &str, role: &str, claim: &str) -> Option<String> {
    match graph.block(key) {
        None => Some(format!(
            "Type {key} is not in the graph: it was renamed, moved, or removed. Reconnect the {role} to the type's new key, or remove the {claim}"
        )),
        Some(block) if !block.block().kind.is_type_declaration() => Some(format!(
            "{key} is not a type declaration or class: the {role} is the interface, type, enum, or class the code writes"
        )),
        Some(_) => None,
    }
}

/// The health items of the page's claims: resource cards (missing, held, outdated), connections
/// (broken, held, outdated; a missing end is reported with every link), and frame descriptions
/// (outdated). Claims citing a project that is not connected are kept unchecked.
pub(super) fn health(page: &Page, graphs: &PageGraphs<'_>) -> Vec<HealthItem> {
    let mut items = Vec::new();
    for resource in &page.resources {
        resource_health(page, graphs, resource, &mut items);
    }
    for link in page.links.iter().filter(|link| link.kind.is_some()) {
        connection_health(page, graphs, link, &mut items);
    }
    for frame in &page.frames {
        let Some(description) = &frame.description else {
            continue;
        };
        let current = frame_signed(page, graphs, frame);
        if current != Signed::NotConnected && outdated(description.signature, current) {
            items.push(item(
                page,
                HealthKind::Outdated,
                &Target::Frame(frame.id.clone()),
                None,
                None,
                "Code changed since this description was written".to_owned(),
                &[RepairKind::Resign, RepairKind::RemoveDescription],
            ));
        }
    }
    items
}

fn outdated(saved: Option<Signature>, current: Signed) -> bool {
    saved.is_some() && current.signature() != saved
}

/// How a claim's problems are reported: the kind of a gone part, the claim in words, and the
/// repair that removes it.
#[derive(Clone, Copy)]
struct Claim<'w> {
    gone: HealthKind,
    word: &'w str,
    remove: RepairKind,
}

/// Items about the claim's anchors that are gone and the keys (its other keys, then its key
/// anchors) held for review; whether there is one.
fn parts_health(
    page: &Page,
    graphs: &PageGraphs<'_>,
    target: &Target,
    claim: Claim<'_>,
    anchors: &[Anchor],
    keys: &[(Option<&ProjectName>, &str)],
    items: &mut Vec<HealthItem>,
) -> bool {
    let mut broken = false;
    for (index, anchor) in anchors.iter().enumerate() {
        let Some(graph) = graphs.get(anchor.project()) else {
            continue;
        };
        if let Some(message) = anchor_gone(graph, anchor, claim.word) {
            broken = true;
            let key = anchor.key().map(|key| (anchor.project(), key.as_str()));
            let repairs = [RepairKind::Reconnect, claim.remove];
            items.push(item(
                page,
                claim.gone,
                target,
                key,
                Some(index),
                message,
                &repairs,
            ));
        }
    }
    let held = [RepairKind::Accept, RepairKind::Reconnect, claim.remove];
    let anchor_keys = anchors
        .iter()
        .filter_map(|anchor| anchor.key().map(|key| (anchor.project(), key.as_str())));
    for (project, key) in keys.iter().copied().chain(anchor_keys) {
        let Some(graph) = graphs.get(project) else {
            continue;
        };
        if let Some(problem) = guard_problem(page, graph, project, key) {
            broken = true;
            let at = anchors
                .iter()
                .position(|anchor| anchor.key().map(|known| known.as_str()) == Some(key));
            items.push(item(
                page,
                HealthKind::Review,
                target,
                Some((project, key)),
                at,
                problem.to_owned(),
                &held,
            ));
        }
    }
    broken
}

fn resource_health(page: &Page, graphs: &PageGraphs<'_>, resource: &Resource, items: &mut Vec<HealthItem>) {
    let target = Target::Resource(resource.id.clone());
    let claim = Claim {
        gone: HealthKind::Missing,
        word: "resource card",
        remove: RepairKind::RemoveResource,
    };
    let remove = [RepairKind::Reconnect, claim.remove];
    let mut broken = false;
    let mut keys = Vec::new();
    if let Some(table) = &resource.type_key {
        let project = resource.project.as_ref();
        if let Some(graph) = graphs.get(project) {
            if let Some(message) = type_gone(graph, table.as_str(), "table's type", "resource card") {
                broken = true;
                items.push(item(
                    page,
                    HealthKind::Missing,
                    &target,
                    Some((project, table.as_str())),
                    None,
                    message,
                    &remove,
                ));
            }
            keys.push((project, table.as_str()));
        }
    }
    broken |= parts_health(page, graphs, &target, claim, &resource.anchors, &keys, items);
    let current = resource_signed(graphs, resource);
    if broken || current == Signed::NotConnected {
        return;
    }
    let description = resource.description.as_ref();
    if outdated(resource.signature, current)
        || description.is_some_and(|description| outdated(description.signature, current))
    {
        let mut repairs = vec![RepairKind::Resign];
        repairs.extend(description.map(|_| RepairKind::RemoveDescription));
        repairs.push(RepairKind::RemoveResource);
        items.push(item(
            page,
            HealthKind::Outdated,
            &target,
            None,
            None,
            "Code changed since this resource card was drawn".to_owned(),
            &repairs,
        ));
    }
}

fn connection_health(page: &Page, graphs: &PageGraphs<'_>, link: &Link, items: &mut Vec<HealthItem>) {
    let target = Target::Link(link.id.clone());
    let claim = Claim {
        gone: HealthKind::BrokenLink,
        word: "connection",
        remove: RepairKind::RemoveLink,
    };
    let remove = [RepairKind::Reconnect, claim.remove];
    let mut broken = false;
    let mut keys = Vec::new();
    if let Some(step) = &link.step {
        let project = page.step_project(link);
        if let Some(graph) = graphs.get(project) {
            let card = page
                .card(&link.from)
                .and_then(|card| graph.block(card.reference.key().as_str()));
            let message = match (graph.block(step.as_str()), card) {
                (None, _) => Some(format!(
                    "Step {step} is not in the graph: the code changed. Reconnect it to the statement's new key, or remove the connection"
                )),
                (Some(block), Some(card)) if !block.within(&card) => Some(format!(
                    "Step {step} is not inside its card's block: a connection leaves from a statement or call of its `from` card. Reconnect the step, or remove the connection"
                )),
                _ => None,
            };
            if let Some(message) = message {
                broken = true;
                items.push(item(
                    page,
                    HealthKind::BrokenLink,
                    &target,
                    Some((project, step.as_str())),
                    None,
                    message,
                    &remove,
                ));
            }
            keys.push((project, step.as_str()));
        }
    }
    if let Some(carries) = &link.carries {
        let project = link.carries_project.as_ref();
        if let Some(graph) = graphs.get(project) {
            if let Some(message) = type_gone(graph, carries.as_str(), "carried type", "connection") {
                broken = true;
                items.push(item(
                    page,
                    HealthKind::BrokenLink,
                    &target,
                    Some((project, carries.as_str())),
                    None,
                    message,
                    &remove,
                ));
            }
            keys.push((project, carries.as_str()));
        }
    }
    broken |= parts_health(page, graphs, &target, claim, &link.anchors, &keys, items);
    let current = connection_signed(page, graphs, link);
    if broken || matches!(current, Signed::NotConnected | Signed::Nothing) {
        return;
    }
    let texts = [link.description.as_ref(), link.label.as_ref()];
    if outdated(link.signature, current)
        || texts
            .iter()
            .flatten()
            .any(|text| outdated(text.signature, current))
    {
        let mut repairs = vec![RepairKind::Resign];
        if texts.iter().any(Option::is_some) {
            repairs.push(RepairKind::RemoveDescription);
        }
        repairs.push(RepairKind::RemoveLink);
        items.push(item(
            page,
            HealthKind::Outdated,
            &target,
            None,
            None,
            "Code changed since this connection was drawn".to_owned(),
            &repairs,
        ));
    }
}

/// Whether resource card `new` says or cites something else than `old` (its kind, name, type,
/// columns, anchors, or author), or is new.
fn resource_changed(old: Option<&Resource>, new: &Resource) -> bool {
    old.is_none_or(|old| {
        old.kind != new.kind
            || old.name != new.name
            || (&old.project, &old.type_key) != (&new.project, &new.type_key)
            || old.columns != new.columns
            || old.anchors != new.anchors
            || old.author != new.author
    })
}

/// Whether connection `new` says or cites something else than `old` (its kind, ends, step,
/// columns, channel, carried type, cardinality, anchors, or author), or is new.
fn connection_changed(old: Option<&Link>, new: &Link) -> bool {
    old.is_none_or(|old| {
        old.kind != new.kind
            || (&old.from, &old.to) != (&new.from, &new.to)
            || old.step != new.step
            || (&old.from_column, &old.to_column) != (&new.from_column, &new.to_column)
            || old.channel != new.channel
            || (&old.carries, &old.carries_project) != (&new.carries, &new.carries_project)
            || old.cardinality != new.cardinality
            || old.anchors != new.anchors
            || old.author != new.author
    })
}

/// Sets the unsigned ones of a claim's `slots` (its signature, its texts' signatures) to
/// `current`. Nothing to sign, a project that isn't connected, or a missing part (which the save
/// then refuses) leaves them as they are; a source that changed since the graph was built can't
/// be signed (`what` names the claim).
fn put<'s>(
    current: Signed,
    slots: impl IntoIterator<Item = &'s mut Option<Signature>>,
    what: &str,
) -> Result<(), BoardError> {
    for slot in slots.into_iter().filter(|slot| slot.is_none()) {
        match current {
            Signed::Signature(signature) => *slot = Some(signature),
            Signed::Unverified => {
                return Err(BoardError::Unverified {
                    what: what.to_owned(),
                });
            }
            Signed::Nothing | Signed::NotConnected | Signed::Missing => {}
        }
    }
    Ok(())
}

/// Fills in the channels entrances give ([`entrance_channel`]) and signs the page's claims from
/// verified source: resource cards with their descriptions, connections that cite code with
/// their descriptions and labels, and frame descriptions. A resource card or connection whose
/// parts changed since `previous` is signed again; signed text stays as it is (it goes outdated
/// when its code changed).
pub(super) fn sign(
    page: &mut Page,
    previous: Option<&Page>,
    graphs: &PageGraphs<'_>,
) -> Result<(), BoardError> {
    let channels: Vec<(usize, String)> = page
        .links
        .iter()
        .enumerate()
        .filter_map(|(index, link)| Some((index, entrance_channel(page, graphs, link)?)))
        .collect();
    for (index, channel) in channels {
        page.links[index].channel = Some(channel);
    }
    for index in 0..page.resources.len() {
        let resource = &page.resources[index];
        let moved = previous
            .and_then(|old| old.resource(&resource.id))
            .is_some_and(|old| {
                (&old.project, &old.type_key, &old.anchors)
                    != (&resource.project, &resource.type_key, &resource.anchors)
            });
        let current = resource_signed(graphs, resource);
        let resource = &mut page.resources[index];
        if moved {
            resource.signature = None;
        }
        let what = format!("resource `{}`", resource.id);
        let description = resource.description.as_mut().map(|text| &mut text.signature);
        put(
            current,
            [Some(&mut resource.signature), description].into_iter().flatten(),
            &what,
        )?;
    }
    for index in 0..page.links.len() {
        let link = &page.links[index];
        let moved = previous.and_then(|old| old.link(&link.id)).is_some_and(|old| {
            (&old.from, &old.step, &old.anchors) != (&link.from, &link.step, &link.anchors)
        });
        let current = connection_signed(page, graphs, link);
        if current == Signed::Nothing {
            continue;
        }
        let link = &mut page.links[index];
        if moved {
            link.signature = None;
        }
        let what = format!("connection `{}`", link.id);
        let texts = [link.description.as_mut(), link.label.as_mut()]
            .into_iter()
            .flatten()
            .map(|text| &mut text.signature);
        put(current, std::iter::once(&mut link.signature).chain(texts), &what)?;
    }
    for index in 0..page.frames.len() {
        let current = frame_signed(page, graphs, &page.frames[index]);
        let frame = &mut page.frames[index];
        let what = format!("the description of frame `{}`", frame.id);
        let description = frame.description.as_mut().map(|text| &mut text.signature);
        put(current, description, &what)?;
    }
    Ok(())
}

/// A claim's refusal at save.
fn refuse(page: &Page, target: Target, message: String) -> BoardError {
    BoardError::Problem {
        page: page.id.clone(),
        target,
        message,
    }
}

/// How a page item reads in a message: a card's qualified name or entrance, a resource card's
/// kind and name, or a frame's title.
fn item_name(page: &Page, graphs: &PageGraphs<'_>, id: &str) -> String {
    if let Some(card) = page.card(id) {
        let key = card.reference.key().as_str();
        let graph = graphs.get(card.reference.project());
        let name = match &card.reference {
            CardRef::Block { .. } => graph.and_then(|graph| graph.qualified_name(key)),
            CardRef::Entrance { .. } => graph.and_then(|graph| graph.entrance(key)).map(entrance_title),
        };
        return name.unwrap_or_else(|| key.to_owned());
    }
    if let Some(resource) = page.resource(id) {
        return format!("the {} \"{}\"", resource.kind.as_str(), resource.name);
    }
    page.frame(id)
        .map_or_else(|| id.to_owned(), |frame| format!("the frame \"{}\"", frame.title))
}

/// How a part reads in a message: a block's name and file, an entrance, or an evidence file's
/// lines.
fn part_name(graphs: &PageGraphs<'_>, part: Part<'_>) -> String {
    match part {
        Part::Key(project, key) => {
            let Some(graph) = graphs.get(project) else {
                return key.to_owned();
            };
            if let Some(block) = graph.block(key) {
                let path = &block.file().file.path;
                let kind = block.block().kind;
                let name = if kind.is_function_like() || kind.is_type_declaration() {
                    graph.qualified_name(key)
                } else {
                    block.block().name.as_deref().map(str::to_owned)
                };
                return match name {
                    Some(name) => format!("{name} in {path}"),
                    None => format!("{path}:{}", block.block().lines.start),
                };
            }
            graph.entrance(key).map_or_else(
                || key.to_owned(),
                |entrance| format!("the {} entrance", entrance_title(entrance)),
            )
        }
        Part::Lines(_, file, [first, last]) => format!("{file} lines {first}–{last}"),
    }
}

/// Parts' names joined for a message: "a, b, and c".
fn names(graphs: &PageGraphs<'_>, parts: &[Part<'_>]) -> String {
    let names: Vec<String> = parts.iter().map(|part| part_name(graphs, *part)).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// The verified bytes of `parts`, or the refusal when a project isn't connected or a part's
/// source changed (`what` names the claim in the refusal).
fn cited<'g>(
    page: &Page,
    graphs: &PageGraphs<'g>,
    parts: &[Part<'_>],
    target: &Target,
    what: &str,
) -> Result<Vec<&'g [u8]>, BoardError> {
    let mut bytes = Vec::with_capacity(parts.len());
    for part in parts {
        match found(graphs, *part) {
            Found::Bytes(found) => bytes.push(found),
            Found::Missing => {}
            Found::Unverified => {
                return Err(BoardError::Unverified {
                    what: what.to_owned(),
                });
            }
            Found::NotConnected => {
                let project = part.project().map_or("", ProjectName::as_str);
                return Err(refuse(
                    page,
                    target.clone(),
                    format!(
                        "Can't check {what}: it cites {project}, which is not connected. Connect that project, or cite code of a connected one"
                    ),
                ));
            }
        }
    }
    Ok(bytes)
}

/// Whether `needle` appears in any of `haystacks`.
fn appears(haystacks: &[&[u8]], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty()
        && haystacks
            .iter()
            .any(|haystack| haystack.windows(needle.len()).any(|window| window == needle))
}

/// The name of type `key` (and its file) for a message.
fn type_name(graph: &BoardGraph, key: &str) -> String {
    match graph.block(key) {
        Some(block) => {
            let name = graph.qualified_name(key).unwrap_or_else(|| key.to_owned());
            format!("{name} ({})", block.file().file.path)
        }
        None => key.to_owned(),
    }
}

/// The fields a refusal offers.
fn listed(fields: &[Box<str>]) -> String {
    let shown: Vec<&str> = fields.iter().take(LISTED_FIELDS).map(|field| &**field).collect();
    let more = fields.len().saturating_sub(LISTED_FIELDS);
    if more > 0 {
        format!("{}, and {more} more", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

/// Checks the claims of `page` that are new or changed since `previous` (see the
/// [module documentation](self)). Run after the page's references were checked, so what the
/// claims cite exists.
pub(super) fn ground(
    page: &Page,
    previous: Option<&Page>,
    graphs: &PageGraphs<'_>,
) -> Result<(), BoardError> {
    for resource in &page.resources {
        if resource_changed(previous.and_then(|old| old.resource(&resource.id)), resource) {
            ground_resource(page, graphs, resource)?;
        }
    }
    for link in page.links.iter().filter(|link| link.kind.is_some()) {
        if connection_changed(previous.and_then(|old| old.link(&link.id)), link) {
            ground_connection(page, graphs, link)?;
        }
    }
    Ok(())
}

fn ground_resource(page: &Page, graphs: &PageGraphs<'_>, resource: &Resource) -> Result<(), BoardError> {
    let target = Target::Resource(resource.id.clone());
    let what = format!("the {} \"{}\"", resource.kind.as_str(), resource.name);
    let table = resource.type_key.as_ref().and_then(|key| {
        let graph = graphs.get(resource.project.as_ref())?;
        Some((graph, key.as_str(), graph.fields(key.as_str())?))
    });
    if let Some((graph, key, fields)) = table
        && let Some(column) = resource
            .columns
            .iter()
            .find(|column| !fields.iter().any(|field| **field == *column.name))
    {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't add {what}: \"{}\" isn't a field of {}. A table backed by a type marks its fields; mark one of: {}",
                column.name,
                type_name(graph, key),
                listed(fields)
            ),
        ));
    }
    if resource.author.kind() != AuthorKind::Agent {
        return Ok(());
    }
    if resource.kind != ResourceKind::Actor && resource.anchors.is_empty() {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't add {what}: it has no anchors. Anchor it to the code that defines or uses it (a block or entrance key), or to lines of an evidence file"
            ),
        ));
    }
    let parts = resource_parts(resource);
    let bytes = cited(page, graphs, &parts, &target, &what)?;
    if !parts.is_empty() && !appears(&bytes, &resource.name) {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't add {what}: \"{}\" doesn't appear in its anchors ({}). Use the name the code writes, or anchor it where the name is written",
                resource.name,
                names(graphs, &parts)
            ),
        ));
    }
    if resource.type_key.is_none()
        && let Some(column) = resource
            .columns
            .iter()
            .find(|column| !appears(&bytes, &column.name))
    {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't add {what}: the column \"{}\" doesn't appear in its anchors ({}). Cite where the column is defined, or leave it out",
                column.name,
                names(graphs, &parts)
            ),
        ));
    }
    Ok(())
}

fn ground_connection(page: &Page, graphs: &PageGraphs<'_>, link: &Link) -> Result<(), BoardError> {
    let target = Target::Link(link.id.clone());
    let kind = link.kind.unwrap_or(LinkKind::Other);
    let what = format!(
        "{} to {} ({})",
        item_name(page, graphs, &link.from),
        item_name(page, graphs, &link.to),
        kind.as_str()
    );
    if link.relates() {
        relates_columns(page, graphs, link, &target, &what)?;
    }
    if link
        .author
        .as_ref()
        .is_none_or(|author| author.kind() != AuthorKind::Agent)
    {
        return Ok(());
    }
    if ANCHORED.contains(&kind) && link.anchors.is_empty() {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't connect {what}: it has no anchors. A {} connection cites its receiving side: anchor it to the code or entrance that receives it",
                kind.as_str()
            ),
        ));
    }
    let parts = connection_parts(page, link);
    let bytes = cited(page, graphs, &parts, &target, &format!("the connection {what}"))?;
    if kind == LinkKind::Calls {
        calls_step(page, graphs, link, &target, &what)?;
    }
    if let Some(channel) = &link.channel
        && entrance_channel(page, graphs, link).is_none()
        && !appears(&bytes, channel)
    {
        return Err(refuse(
            page,
            target,
            format!(
                "Can't connect {what}: the channel \"{channel}\" doesn't appear in its step or anchors ({}). Anchor it where the channel is named, or leave the channel out",
                names(graphs, &parts)
            ),
        ));
    }
    if let Some(carries) = &link.carries {
        carried(page, graphs, link, carries.as_str(), &target, &what)?;
    }
    Ok(())
}

/// Refuses a `relates` connection naming a column its table, backed by a type, doesn't have.
fn relates_columns(
    page: &Page,
    graphs: &PageGraphs<'_>,
    link: &Link,
    target: &Target,
    what: &str,
) -> Result<(), BoardError> {
    for (end, column) in [(&link.from, &link.from_column), (&link.to, &link.to_column)] {
        let (Some(table), Some(column)) = (page.resource(end), column) else {
            continue;
        };
        let Some((graph, key)) = table
            .type_key
            .as_ref()
            .and_then(|key| Some((graphs.get(table.project.as_ref())?, key.as_str())))
        else {
            continue;
        };
        if let Some(fields) = graph.fields(key)
            && !fields.iter().any(|field| **field == **column)
        {
            return Err(refuse(
                page,
                target.clone(),
                format!(
                    "Can't connect {what}: \"{column}\" isn't a column of the table \"{}\", whose columns are the fields of {}. Use one of: {}",
                    table.name,
                    type_name(graph, key),
                    listed(fields)
                ),
            ));
        }
    }
    Ok(())
}

/// Refuses a `calls` connection whose step isn't a call Unfold can't follow.
fn calls_step(
    page: &Page,
    graphs: &PageGraphs<'_>,
    link: &Link,
    target: &Target,
    what: &str,
) -> Result<(), BoardError> {
    let fail = |message: String| Err(refuse(page, target.clone(), message));
    let Some(step) = &link.step else {
        return fail(format!(
            "Can't connect {what}: it has no step. A calls connection leaves from the call Unfold can't follow; give that call as its step"
        ));
    };
    let Some(block) = graphs
        .get(page.step_project(link))
        .and_then(|graph| graph.block(step.as_str()))
    else {
        return Ok(());
    };
    let line = block.block().lines.start;
    if !matches!(block.block().kind, BlockKind::Call | BlockKind::New) {
        return fail(format!(
            "Can't connect {what}: its step (line {line}) isn't a call. Give the call itself as the step, such as {step}/call[0]"
        ));
    }
    match block.occurrence() {
        Some(edge) if edge.resolution == Resolution::Resolved => {
            let callee = edge.target.as_ref().map_or_else(String::new, |key| {
                graphs
                    .get(page.step_project(link))
                    .and_then(|graph| graph.qualified_name(key.as_str()))
                    .unwrap_or_else(|| key.to_string())
            });
            fail(format!(
                "Can't connect {what}: the call {} (line {line}) resolves to {callee}, so Unfold already draws it. A calls connection is for a call Unfold can't follow; open the call's arrow instead",
                edge.text
            ))
        }
        _ => Ok(()),
    }
}

/// Refuses a carried type that neither the step's function nor an anchor's block uses.
fn carried(
    page: &Page,
    graphs: &PageGraphs<'_>,
    link: &Link,
    carries: &str,
    target: &Target,
    what: &str,
) -> Result<(), BoardError> {
    let project = link.carries_project.as_ref();
    let Some(graph) = graphs.get(project) else {
        return Ok(());
    };
    let owner = link
        .step
        .as_ref()
        .filter(|_| page.step_project(link) == project)
        .and_then(|step| graph.block(step.as_str()))
        .and_then(|step| graph.block(step.owner().as_str()));
    let anchors = link
        .anchors
        .iter()
        .filter(|anchor| anchor.project() == project)
        .filter_map(|anchor| {
            let key = anchor.key()?.as_str();
            match graph.block(key) {
                Some(block) => Some(block),
                None => graph
                    .entrance(key)
                    .and_then(|entrance| entrance.evidence.block.as_ref())
                    .and_then(|evidence| graph.block(evidence.as_str())),
            }
        });
    if owner
        .iter()
        .copied()
        .chain(anchors)
        .any(|block| block.uses_type(carries))
    {
        return Ok(());
    }
    let owner_name = owner
        .and_then(|block| graph.qualified_name(block.block().key.as_str()))
        .unwrap_or_else(|| "its step's function".to_owned());
    Err(refuse(
        page,
        target.clone(),
        format!(
            "Can't connect {what}: {} isn't used in {owner_name} or in its anchors. Carry a type the sending code or an anchor writes, or leave carries out",
            type_name(graph, carries)
        ),
    ))
}
