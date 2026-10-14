//! Saving: preparing a board for writing, page by page — signing what is unsigned (card
//! descriptions, names, combined steps, and the claims: resource cards, connections, and frame
//! descriptions), saving guards, checking the overlay ([`crate::overlay`]) and the cards, refusing
//! new problems, and grounding new or changed claims ([`claims`]).
//!
//! | Signed | Over |
//! | --- | --- |
//! | A card's description | its block's source, or the source of an app's or entrance's code |
//! | A name | its block's source |
//! | A combined step, and its description | the source from its first block's start to its last block's end |
//! | Claims | what they cite ([`claims`]) |
//!
//! A signature is only ever computed from bytes that verified against the facts' hash, so text
//! can never be signed against source the graph has not seen. Notes pass through as they are;
//! proposed steps and opened calls are never signed, but a proposed step's place is guarded.
//!
//! **The overlay checks** run on what is new or changed since the previous version: a combined
//! step follows the rules and overlaps or nests in no other (through health), a proposed step
//! goes after a block or at the start of a body ([`check_proposal`]), and a card's opened calls
//! are project calls or callbacks, at most 3 deep and 64 per card ([`check_open_paths`]).

use rustc_hash::FxHashSet;
use unfold_core::Key;

use super::BoardError;
use super::check::{
    Health, HealthKind, Target, current_guard, guard_problem, health, put_guard, span_signature,
};
use super::claims::{self, Signed};
use super::graph::BoardGraph;
use super::items::check_items;
use super::model::{Board, Card, CardRef, Description, Page, Signature};
use super::parse::Path;
use crate::overlay::{check_combined, check_open_paths, check_proposal};

fn refuse(page: &Page, target: &Target, message: &str) -> BoardError {
    BoardError::Problem {
        page: page.id.clone(),
        target: target.clone(),
        message: message.to_owned(),
    }
}

/// A claim's signature now, or why it can't be signed.
fn claim_signature(page: &Page, target: &Target, signed: Signed) -> Result<Signature, BoardError> {
    match signed {
        Signed::Signature(signature) => Ok(signature),
        Signed::Nothing => Err(refuse(page, target, "cites no code to sign against")),
        Signed::Missing => Err(refuse(
            page,
            target,
            "cites code that is gone; reconnect it first",
        )),
        Signed::Unverified => Err(BoardError::Unverified {
            what: target.to_string(),
        }),
    }
}

/// The current signature of a card's code, a named block, a combined step's span, a
/// connection's or resource card's cited code, or a frame's member cards.
pub(crate) fn current_signature(
    page: &Page,
    graph: &BoardGraph,
    target: &Target,
) -> Result<Signature, BoardError> {
    let unverified = |what: &str| BoardError::Unverified {
        what: what.to_owned(),
    };
    match target {
        Target::Card(id) => {
            let card = page
                .card(id)
                .ok_or_else(|| refuse(page, target, "unknown card"))?;
            let key = card.reference.key().as_str();
            if !graph.exists(key) {
                return Err(refuse(page, target, &format!("Code reference is missing: {key}")));
            }
            if let Some(problem) = guard_problem(page, graph, key) {
                return Err(refuse(page, target, problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
        Target::Combined(id) => {
            let combined = page
                .combined_step(id)
                .ok_or_else(|| refuse(page, target, "unknown combined step"))?;
            if let Err(error) = check_combined(graph, &combined.members) {
                return Err(refuse(page, target, &error.to_string()));
            }
            if let Some(problem) = combined
                .members
                .iter()
                .find_map(|key| guard_problem(page, graph, key.as_str()))
            {
                return Err(refuse(page, target, problem));
            }
            span_signature(graph, combined).ok_or_else(|| unverified(id))
        }
        Target::Name(key) => {
            if graph.block(key).is_none() {
                return Err(refuse(page, target, &format!("Named block is missing: {key}")));
            }
            if let Some(problem) = guard_problem(page, graph, key) {
                return Err(refuse(page, target, problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
        Target::Connection(id) => {
            let connection = page
                .connection(id)
                .ok_or_else(|| refuse(page, target, "unknown connection"))?;
            claim_signature(page, target, claims::connection_signed(graph, connection))
        }
        Target::Resource(id) => {
            let resource = page
                .resource(id)
                .ok_or_else(|| refuse(page, target, "unknown resource card"))?;
            claim_signature(page, target, claims::resource_signed(graph, resource))
        }
        Target::Frame(id) => {
            let frame = page
                .frame(id)
                .ok_or_else(|| refuse(page, target, "unknown frame"))?;
            claim_signature(page, target, claims::frame_signed(page, graph, frame))
        }
        Target::Proposal(_) => Err(refuse(
            page,
            target,
            "a proposed step is never signed: it has no source",
        )),
    }
}

/// The signatures the Resign repair sets for `target`: a card's description, a name, a combined
/// step's own signature with its description's, a connection's own with its description's and
/// label's, a resource card's own with its description's, or a frame's description.
fn signatures_mut<'a>(page: &'a mut Page, target: &Target) -> Vec<&'a mut Option<Signature>> {
    let texts = |texts: [Option<&'a mut Description>; 2]| {
        texts
            .into_iter()
            .flatten()
            .map(|text| &mut text.signature)
            .collect::<Vec<_>>()
    };
    match target {
        Target::Card(id) => texts([
            page.cards
                .iter_mut()
                .find(|card| &card.id == id)
                .and_then(|card| card.description.as_mut()),
            None,
        ]),
        Target::Name(key) => texts([
            page.names
                .iter_mut()
                .find(|entry| entry.key.as_str() == key)
                .map(|entry| &mut entry.name),
            None,
        ]),
        Target::Combined(id) => match page.combined.iter_mut().find(|combined| &combined.id == id) {
            Some(combined) => {
                let mut slots = vec![&mut combined.signature];
                slots.extend(texts([combined.description.as_mut(), None]));
                slots
            }
            None => Vec::new(),
        },
        Target::Connection(id) => match page.links.iter_mut().find(|connection| &connection.id == id) {
            Some(connection) => {
                let mut slots = vec![&mut connection.signature];
                slots.extend(texts([
                    connection.description.as_mut(),
                    connection.label.as_mut(),
                ]));
                slots
            }
            None => Vec::new(),
        },
        Target::Resource(id) => match page.resources.iter_mut().find(|resource| &resource.id == id) {
            Some(resource) => {
                let mut slots = vec![&mut resource.signature];
                slots.extend(texts([resource.description.as_mut(), None]));
                slots
            }
            None => Vec::new(),
        },
        Target::Frame(id) => texts([
            page.frames
                .iter_mut()
                .find(|frame| &frame.id == id)
                .and_then(|frame| frame.description.as_mut()),
            None,
        ]),
        Target::Proposal(_) => Vec::new(),
    }
}

/// Signs `target`'s text (and a combined step's, resource card's, or connection's own
/// signature) against the current source, keeping the text: the Resign repair.
pub(crate) fn resign(page: &mut Page, graph: &BoardGraph, target: &Target) -> Result<(), BoardError> {
    let signature = current_signature(page, graph, target)?;
    let slots = signatures_mut(page, target);
    if slots.is_empty() {
        return Err(refuse(page, target, "has no description to sign"));
    }
    for slot in slots {
        *slot = Some(signature);
    }
    Ok(())
}

/// Signs every unsigned card description, name, and combined step (with its description) from
/// verified source, keeps the guards of references still on the page (adding fresh ones for new
/// references), and drops the rest.
fn sign(page: &mut Page, graph: &BoardGraph) -> Result<(), BoardError> {
    let unsigned =
        |description: Option<&Description>| description.is_some_and(|item| item.signature.is_none());
    let targets: Vec<Target> = page
        .cards
        .iter()
        .filter(|card| unsigned(card.description.as_ref()))
        .map(|card| Target::Card(card.id.clone()))
        .chain(
            page.names
                .iter()
                .filter(|entry| entry.name.signature.is_none())
                .map(|entry| Target::Name(entry.key.as_str().to_owned())),
        )
        .chain(
            page.combined
                .iter()
                .filter(|combined| combined.signature.is_none() || unsigned(combined.description.as_ref()))
                .map(|combined| Target::Combined(combined.id.clone())),
        )
        .collect();
    let mut signatures = Vec::with_capacity(targets.len());
    for target in targets {
        let signature = current_signature(page, graph, &target)?;
        signatures.push((target, signature));
    }
    for (target, signature) in signatures {
        for slot in signatures_mut(page, &target) {
            if slot.is_none() {
                *slot = Some(signature);
            }
        }
    }

    let referenced: Vec<String> = page.references().keys.into_iter().map(str::to_owned).collect();
    let kept: FxHashSet<&str> = referenced.iter().map(String::as_str).collect();
    page.guards.retain(|entry| kept.contains(entry.key.as_str()));
    for key in &referenced {
        if page.guard(key).is_none()
            && let Some(guard) = current_guard(graph, key)
        {
            put_guard(page, guard);
        }
    }
    Ok(())
}

/// Whether `target` is the same reference (or combined step, name, or connection) on both pages.
fn unchanged(old: &Page, new: &Page, target: &Target) -> bool {
    match target {
        Target::Card(id) => {
            old.card(id).map(|card| &card.reference) == new.card(id).map(|card| &card.reference)
        }
        Target::Combined(id) => {
            old.combined_step(id).map(|combined| &combined.members)
                == new.combined_step(id).map(|combined| &combined.members)
        }
        Target::Name(key) => old.name_entry(key) == new.name_entry(key),
        Target::Connection(id) => match (old.connection(id), new.connection(id)) {
            (Some(a), Some(b)) => {
                (&a.from, &a.to, &a.step, &a.carries, &a.anchors)
                    == (&b.from, &b.to, &b.step, &b.carries, &b.anchors)
            }
            _ => false,
        },
        Target::Resource(id) => match (old.resource(id), new.resource(id)) {
            (Some(a), Some(b)) => (&a.type_key, &a.anchors) == (&b.type_key, &b.anchors),
            _ => false,
        },
        Target::Frame(id) => {
            old.frame(id).map(|frame| &frame.members) == new.frame(id).map(|frame| &frame.members)
        }
        Target::Proposal(id) => {
            old.proposal(id).map(|proposal| &proposal.place)
                == new.proposal(id).map(|proposal| &proposal.place)
        }
    }
}

/// Refuses a proposed step that is new or placed elsewhere since `previous` unless it goes after
/// a block or at the start of a body ([`check_proposal`]). One placed as before is kept as it
/// is, even when its place went missing or is held: that is health's to report.
fn place_proposals(page: &Page, previous: Option<&Page>, graph: &BoardGraph) -> Result<(), BoardError> {
    for proposal in &page.proposals {
        let placed_before = previous
            .and_then(|old| old.proposal(&proposal.id))
            .is_some_and(|before| before.place == proposal.place);
        if placed_before || graph.block(proposal.place.key().as_str()).is_none() {
            continue;
        }
        if let Err(error) = check_proposal(graph, &proposal.place.spot()) {
            let target = Target::Proposal(proposal.id.clone());
            return Err(refuse(page, &target, &format!("Can't place it: {error}")));
        }
    }
    Ok(())
}

/// Refuses a card that is new or changed since `previous` (in what it shows or opens) unless a
/// block card shows a module, definition, or type, and its opened calls are project calls or
/// callbacks drawn where they open, at most 3 deep and 64 per card ([`check_open_paths`]). A
/// missing reference is health's to report, and an open path the card already had whose code
/// is gone is kept as it was (a Reconnect leaves it at its old keys).
fn check_cards(page: &Page, previous: Option<&Page>, graph: &BoardGraph) -> Result<(), BoardError> {
    let before = |card: &Card| previous.and_then(|old| old.card(&card.id));
    let same = |card: &Card| {
        before(card).is_some_and(|before| (&before.reference, &before.open) == (&card.reference, &card.open))
    };
    for card in page.cards.iter().filter(|card| !same(card)) {
        let CardRef::Block { block: key } = &card.reference else {
            continue;
        };
        let Some(block) = graph.block(key.as_str()) else {
            continue;
        };
        let target = Target::Card(card.id.clone());
        if !block.kind().is_definition() {
            return Err(refuse(
                page,
                &target,
                &format!(
                    "`{key}` is a {}: a card shows a module, a class, a function, or a type",
                    block.kind().as_str()
                ),
            ));
        }
        let gone = |path: &Vec<Key>| {
            before(card).is_some_and(|before| before.open.contains(path))
                && path.iter().any(|key| !graph.exists(key.as_str()))
        };
        let paths = card.open.iter().filter(|path| !gone(path)).map(Vec::as_slice);
        if let Err(error) = check_open_paths(graph, key, paths) {
            return Err(refuse(page, &target, &format!("Can't open it in place: {error}")));
        }
    }
    Ok(())
}

/// Prepares one page (at `path`) for writing over `previous` (the version being replaced, with
/// its health): the references between its items must hold (so Rust's own edits, such as
/// repairs, never write a page the parser would refuse), guards the new version dropped come
/// back from `previous`, unsigned text is signed, guards are saved, claims signed, the new or
/// changed cards and proposed steps checked ([`check_cards`], [`place_proposals`]), and the
/// result is refused when it introduces a missing reference, a broken combined step, a held
/// reference, or a broken link that `previous` did not already have (with the same reference),
/// or has a new or changed claim that isn't grounded ([`claims::ground`]). Problems carried over
/// unchanged are kept, so a page with a missing reference can still be moved, retitled, or
/// described elsewhere. Outdated text never refuses a save.
fn prepare_page(
    mut page: Page,
    path: &Path,
    previous: Option<(&Page, &Health)>,
    graph: &BoardGraph,
) -> Result<(Page, Health), BoardError> {
    check_items(&page, path)?;
    if let Some((old, _)) = previous {
        for guard in &old.guards {
            if page.guard(guard.key.as_str()).is_none() {
                page.guards.push(guard.clone());
            }
        }
    }
    sign(&mut page, graph)?;
    claims::sign(&mut page, previous.map(|(old, _)| old), graph)?;
    check_cards(&page, previous.map(|(old, _)| old), graph)?;
    place_proposals(&page, previous.map(|(old, _)| old), graph)?;
    let health = health(&page, graph);
    for item in health
        .items
        .iter()
        .filter(|item| item.kind != HealthKind::Outdated)
    {
        let carried = previous.is_some_and(|(old, old_health)| {
            old_health.items.iter().any(|known| {
                known.kind == item.kind && known.target == item.target && known.message == item.message
            }) && unchanged(old, &page, &item.target)
        });
        if !carried {
            return Err(refuse(&page, &item.target, &item.message));
        }
    }
    claims::ground(&page, previous.map(|(old, _)| old), graph)?;
    Ok((page, health))
}

/// Prepares `board` for writing over `previous` (the version being replaced, with its health,
/// if any), page by page against `graph`: a page equal to the previous version's page keeps
/// its health unchecked; every other page is signed and checked (see the module documentation).
/// Returns the board and its health.
pub fn prepare_save(
    mut board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    let mut health = Health::default();
    let mut pages = Vec::with_capacity(board.pages.len());
    let pages_path = Path::default().join("pages");
    for (index, page) in std::mem::take(&mut board.pages).into_iter().enumerate() {
        let old = previous.and_then(|(old, old_health)| {
            old.page(&page.id)
                .map(|old_page| (old_page, old_health.of_page(&page.id)))
        });
        let path = pages_path.join(index);
        let (page, page_health) = match &old {
            Some((old_page, old_health)) if **old_page == page => (page, old_health.clone()),
            Some((old_page, old_health)) => prepare_page(page, &path, Some((old_page, old_health)), graph)?,
            None => prepare_page(page, &path, None, graph)?,
        };
        health.append(page_health);
        pages.push(page);
    }
    board.pages = pages;
    Ok((board, health))
}
