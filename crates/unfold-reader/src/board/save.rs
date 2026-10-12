//! Saving: preparing a board for writing, page by page and project by project — signing what
//! is unsigned (descriptions, labels, link labels, and the claims: resource cards, connections,
//! and frame descriptions), saving guards, refusing new problems, placing new or moved proposed
//! steps, and grounding new or changed claims ([`claims`]).
//!
//! A signature is only ever computed from bytes that verified against the facts' hash, so text
//! can never be signed against source the graph has not seen. Text about a project that is not
//! connected stays as it is: it is signed once its project is connected again. Notes and cards'
//! expanded calls pass through as they are; proposed steps are never signed, but their anchors
//! are guarded.

use rustc_hash::FxHashSet;

use super::BoardError;
use super::check::{
    Health, HealthKind, ProjectKey, Target, current_guard, group_problem, guard_problem, health,
    placement_problem, put_guard,
};
use super::claims::{self, Signed};
use super::graph::PageGraphs;
use super::items::check_items;
use super::model::{Board, Description, Page, ProjectName, Proposal, Signature};
use super::parse::Path;

fn refuse(page: &Page, target: &Target, message: &str) -> BoardError {
    BoardError::Problem {
        page: page.id.clone(),
        target: target.clone(),
        message: message.to_owned(),
    }
}

/// The signature a link label gets: `sha256:` of `"<from>\n<to>"`, each the hex digits of the
/// end's guard signature. `None` while an end has no guard.
fn link_signature(page: &Page, from: &str, to: &str) -> Option<Signature> {
    let end = |id: &str| {
        let card = page.card(id)?;
        page.guard(card.reference.project(), card.reference.key().as_str())
            .map(|guard| guard.signature.0)
    };
    let (from, to) = (end(from)?, end(to)?);
    Some(Signature::of(format!("{from}\n{to}").as_bytes()))
}

/// A claim's signature now, or why it can't be signed.
fn claim_signature(page: &Page, target: &Target, signed: Signed) -> Result<Signature, BoardError> {
    match signed {
        Signed::Signature(signature) => Ok(signature),
        Signed::Nothing => Err(refuse(page, target, "cites no code to sign against")),
        Signed::NotConnected => Err(refuse(page, target, "its project is not connected")),
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

/// The current signature of a card's reference, a group's span, a labeled block, a drawn link's
/// ends, a connection's or resource card's cited code, or a frame's member cards.
pub(crate) fn current_signature(
    page: &Page,
    graphs: &PageGraphs<'_>,
    target: &Target,
) -> Result<Signature, BoardError> {
    let unverified = |what: &str| BoardError::Unverified {
        what: what.to_owned(),
    };
    let connected = |project: Option<&ProjectName>| {
        graphs
            .get(project)
            .ok_or_else(|| refuse(page, target, "its project is not connected"))
    };
    match target {
        Target::Card(id) => {
            let card = page
                .card(id)
                .ok_or_else(|| refuse(page, target, "unknown card"))?;
            let project = card.reference.project();
            let graph = connected(project)?;
            let key = card.reference.key().as_str();
            if let Some(problem) = guard_problem(page, graph, project, key) {
                return Err(refuse(page, target, problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
        Target::Group(id) => {
            let group = page
                .group(id)
                .ok_or_else(|| refuse(page, target, "unknown group"))?;
            let project = page.group_project(group);
            let graph = connected(project)?;
            if let Some(problem) = group_problem(page, graph, group) {
                return Err(refuse(page, target, problem));
            }
            if let Some(problem) = group
                .members
                .iter()
                .find_map(|key| guard_problem(page, graph, project, key.as_str()))
            {
                return Err(refuse(page, target, problem));
            }
            let (Some(first), Some(last)) = (group.members.first(), group.members.last()) else {
                return Err(refuse(page, target, "has no members"));
            };
            graph
                .span_signature(first.as_str(), last.as_str())
                .ok_or_else(|| unverified(id))
        }
        Target::Label(ProjectKey { project, key }) => {
            let graph = connected(project.as_ref())?;
            if let Some(problem) = guard_problem(page, graph, project.as_ref(), key) {
                return Err(refuse(page, target, problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
        Target::Link(id) => {
            let link = page
                .link(id)
                .ok_or_else(|| refuse(page, target, "unknown link"))?;
            if claims::cites(page, link) {
                return claim_signature(page, target, claims::connection_signed(page, graphs, link));
            }
            link_signature(page, &link.from, &link.to).ok_or_else(|| unverified(id))
        }
        Target::Resource(id) => {
            let resource = page
                .resource(id)
                .ok_or_else(|| refuse(page, target, "unknown resource card"))?;
            claim_signature(page, target, claims::resource_signed(graphs, resource))
        }
        Target::Frame(id) => {
            let frame = page
                .frame(id)
                .ok_or_else(|| refuse(page, target, "unknown frame"))?;
            claim_signature(page, target, claims::frame_signed(page, graphs, frame))
        }
        Target::Proposal(_) => Err(refuse(
            page,
            target,
            "a proposed step is never signed: it has no implementation source",
        )),
    }
}

/// The signatures the Resign repair sets for `target`: a card's or group's description, a
/// label, a drawn link's label, a connection's own signature with its description and label, a
/// resource card's with its description, or a frame's description.
fn signatures_mut<'a>(page: &'a mut Page, target: &Target) -> Vec<&'a mut Option<Signature>> {
    let texts = |texts: [Option<&'a mut Description>; 2]| {
        texts
            .into_iter()
            .flatten()
            .map(|text| &mut text.signature)
            .collect::<Vec<_>>()
    };
    match target {
        Target::Link(id) => {
            let cites = page.link(id).is_some_and(|link| claims::cites(page, link));
            let Some(link) = page.links.iter_mut().find(|link| &link.id == id) else {
                return Vec::new();
            };
            if !cites {
                return texts([link.label.as_mut(), None]);
            }
            let mut slots = vec![&mut link.signature];
            slots.extend(texts([link.description.as_mut(), link.label.as_mut()]));
            slots
        }
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
        Target::Card(_) | Target::Group(_) | Target::Label(_) => texts([description_mut(page, target), None]),
        Target::Proposal(_) => Vec::new(),
    }
}

/// Signs `target`'s text (and a resource card's or connection's own signature) against the
/// current source, keeping the text: the Resign repair.
pub(crate) fn resign(page: &mut Page, graphs: &PageGraphs<'_>, target: &Target) -> Result<(), BoardError> {
    let signature = current_signature(page, graphs, target)?;
    let slots = signatures_mut(page, target);
    if slots.is_empty() {
        return Err(refuse(page, target, "has no description to sign"));
    }
    for slot in slots {
        *slot = Some(signature);
    }
    Ok(())
}

/// The description, label, or link label `target` names.
fn description_mut<'a>(page: &'a mut Page, target: &Target) -> Option<&'a mut Description> {
    match target {
        Target::Card(id) => page
            .cards
            .iter_mut()
            .find(|card| &card.id == id)
            .and_then(|card| card.description.as_mut()),
        Target::Group(id) => page
            .groups
            .iter_mut()
            .find(|group| &group.id == id)
            .and_then(|group| group.description.as_mut()),
        Target::Label(ProjectKey { project, key }) => page
            .labels
            .iter_mut()
            .find(|entry| &entry.project == project && entry.key.as_str() == key)
            .map(|entry| &mut entry.label),
        Target::Link(id) => page
            .links
            .iter_mut()
            .find(|link| &link.id == id)
            .and_then(|link| link.label.as_mut()),
        Target::Resource(_) | Target::Frame(_) | Target::Proposal(_) => None,
    }
}

/// The projects a page's references are in: the board's own first, then the others it names
/// (cards, labels, and the claims' projects), without duplicates.
fn page_projects(page: &Page) -> Vec<Option<ProjectName>> {
    let mut projects = vec![None];
    for project in page
        .projects()
        .chain(page.labels.iter().filter_map(|entry| entry.project.as_ref()))
    {
        if !projects.iter().any(|known| known.as_ref() == Some(project)) {
            projects.push(Some(project.clone()));
        }
    }
    projects
}

/// Signs every unsigned description and label of a connected project from verified source,
/// keeps the guards of references still on the page (adding fresh ones for new references of
/// connected projects), drops the rest, then signs unsigned link labels from the ends' guards.
fn sign(page: &mut Page, graphs: &PageGraphs<'_>) -> Result<(), BoardError> {
    let connected = |project: Option<&ProjectName>| graphs.get(project).is_some();
    let unsigned =
        |description: Option<&Description>| description.is_some_and(|item| item.signature.is_none());
    let targets: Vec<Target> = page
        .cards
        .iter()
        .filter(|card| unsigned(card.description.as_ref()) && connected(card.reference.project()))
        .map(|card| Target::Card(card.id.clone()))
        .chain(
            page.groups
                .iter()
                .filter(|group| unsigned(group.description.as_ref()) && connected(page.group_project(group)))
                .map(|group| Target::Group(group.id.clone())),
        )
        .chain(
            page.labels
                .iter()
                .filter(|entry| entry.label.signature.is_none() && connected(entry.project.as_ref()))
                .map(|entry| Target::Label(ProjectKey::new(entry.project.as_ref(), entry.key.as_str()))),
        )
        .collect();
    let mut signatures = Vec::with_capacity(targets.len());
    for target in targets {
        let signature = current_signature(page, graphs, &target)?;
        signatures.push((target, signature));
    }
    for (target, signature) in signatures {
        if let Some(description) = description_mut(page, &target) {
            description.signature = Some(signature);
        }
    }

    let mut referenced: Vec<(Option<ProjectName>, String)> = Vec::new();
    for project in page_projects(page) {
        for key in page.references(project.as_ref()).keys {
            referenced.push((project.clone(), key.to_owned()));
        }
    }
    let kept: FxHashSet<(Option<&ProjectName>, &str)> = referenced
        .iter()
        .map(|(project, key)| (project.as_ref(), key.as_str()))
        .collect();
    page.guards
        .retain(|entry| kept.contains(&(entry.project.as_ref(), entry.key.as_str())));
    for (project, key) in &referenced {
        if page.guard(project.as_ref(), key).is_none()
            && let Some(guard) = graphs
                .get(project.as_ref())
                .and_then(|graph| current_guard(graph, project.as_ref(), key))
        {
            put_guard(page, guard);
        }
    }

    // A connection that cites code signs its label over that code (`claims::sign`).
    let links: Vec<(usize, Signature)> = page
        .links
        .iter()
        .enumerate()
        .filter(|(_, link)| link.label.as_ref().is_some_and(|label| label.signature.is_none()))
        .filter(|(_, link)| !claims::cites(page, link))
        .filter_map(|(index, link)| Some((index, link_signature(page, &link.from, &link.to)?)))
        .collect();
    for (index, signature) in links {
        if let Some(label) = page.links[index].label.as_mut() {
            label.signature = Some(signature);
        }
    }
    Ok(())
}

/// Whether `target` is the same reference (or group, label, or link) on both pages.
fn unchanged(old: &Page, new: &Page, target: &Target) -> bool {
    match target {
        Target::Card(id) => {
            old.card(id).map(|card| &card.reference) == new.card(id).map(|card| &card.reference)
        }
        Target::Group(id) => match (old.group(id), new.group(id)) {
            (Some(a), Some(b)) => a.card == b.card && a.members == b.members,
            _ => false,
        },
        Target::Label(ProjectKey { project, key }) => {
            old.label_entry(project.as_ref(), key) == new.label_entry(project.as_ref(), key)
        }
        Target::Link(id) => match (old.link(id), new.link(id)) {
            (Some(a), Some(b)) => {
                (
                    &a.from,
                    &a.to,
                    &a.step,
                    &a.carries,
                    &a.carries_project,
                    &a.anchors,
                ) == (
                    &b.from,
                    &b.to,
                    &b.step,
                    &b.carries,
                    &b.carries_project,
                    &b.anchors,
                )
            }
            _ => false,
        },
        Target::Resource(id) => match (old.resource(id), new.resource(id)) {
            (Some(a), Some(b)) => {
                (&a.project, &a.type_key, &a.anchors) == (&b.project, &b.type_key, &b.anchors)
            }
            _ => false,
        },
        Target::Frame(id) => {
            old.frame(id).map(|frame| &frame.members) == new.frame(id).map(|frame| &frame.members)
        }
        Target::Proposal(id) => match (old.proposal(id), new.proposal(id)) {
            (Some(a), Some(b)) => same_place(old, a, new, b),
            _ => false,
        },
    }
}

/// Whether proposed step `a` of page `old` goes where `b` of page `new` does: after the same
/// anchor, on the same path, on the same card showing the same block.
fn same_place(old: &Page, a: &Proposal, new: &Page, b: &Proposal) -> bool {
    (&a.card, &a.after, a.path) == (&b.card, &b.after, b.path)
        && old.card(&a.card).map(|card| &card.reference) == new.card(&b.card).map(|card| &card.reference)
}

/// Refuses a proposed step that is new or placed elsewhere since `previous` ([`same_place`])
/// unless it goes after a step of its card's own Flow that something runs after, on a path that
/// step has ([`placement_problem`]). One placed as before is kept as it is, even when its anchor went
/// missing or is held: that is health's to report.
fn place_proposals(page: &Page, previous: Option<&Page>, graphs: &PageGraphs<'_>) -> Result<(), BoardError> {
    for proposal in &page.proposals {
        if previous
            .and_then(|old| Some((old, old.proposal(&proposal.id)?)))
            .is_some_and(|(old, before)| same_place(old, before, page, proposal))
        {
            continue;
        }
        let target = Target::Proposal(proposal.id.clone());
        // The page's items were checked: the card is a block card of the page.
        let Some(card) = page.card(&proposal.card) else {
            return Err(refuse(page, &target, "unknown card"));
        };
        let project = card.reference.project();
        let Some(graph) = graphs.get(project) else {
            let name = project.map_or("", ProjectName::as_str);
            return Err(refuse(
                page,
                &target,
                &format!(
                    "Can't place it: its card's project {name} is not connected. Connect that project, or propose the step on a card of a connected project"
                ),
            ));
        };
        if let Some(message) = placement_problem(
            graph,
            card.reference.key().as_str(),
            proposal.after.as_str(),
            proposal.path,
        ) {
            return Err(refuse(page, &target, &message));
        }
    }
    Ok(())
}

/// Prepares one page (at `path`) for writing over `previous` (the version being replaced, with
/// its health): the references between its items must hold (so Rust's own edits, such as
/// repairs, never write a page the parser would refuse), guards the new version dropped come
/// back from `previous`, unsigned text is signed, guards are saved, link labels and claims
/// signed, and the result is refused when it places a proposed step where it can't go
/// ([`place_proposals`]), introduces a missing reference, a broken group, a held reference, or a
/// broken link that `previous` did not already have (with the same reference), or has a new or
/// changed claim that isn't grounded ([`claims::ground`]). Problems carried over unchanged are
/// kept, so a page with a placeholder can still be moved, retitled, or described elsewhere.
/// Outdated text and cards of projects that are not connected never refuse a save.
fn prepare_page(
    mut page: Page,
    path: &Path,
    previous: Option<(&Page, &Health)>,
    graphs: &PageGraphs<'_>,
) -> Result<(Page, Health), BoardError> {
    check_items(&page, path)?;
    if let Some((old, _)) = previous {
        for guard in &old.guards {
            if page.guard(guard.project.as_ref(), guard.key.as_str()).is_none() {
                page.guards.push(guard.clone());
            }
        }
    }
    sign(&mut page, graphs)?;
    claims::sign(&mut page, previous.map(|(old, _)| old), graphs)?;
    place_proposals(&page, previous.map(|(old, _)| old), graphs)?;
    let health = health(&page, graphs);
    for item in health
        .items
        .iter()
        .filter(|item| !matches!(item.kind, HealthKind::Outdated | HealthKind::NotConnected))
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
    claims::ground(&page, previous.map(|(old, _)| old), graphs)?;
    Ok((page, health))
}

/// Prepares `board` for writing over `previous` (the version being replaced, with its health,
/// if any), page by page against `graphs`: a page equal to the previous version's page keeps
/// its health unchecked; every other page is signed and checked (see the module documentation). [`Board::projects`] is refreshed. Returns the board and its health.
pub fn prepare_save(
    mut board: Board,
    previous: Option<(&Board, &Health)>,
    graphs: &PageGraphs<'_>,
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
            Some((old_page, old_health)) => prepare_page(page, &path, Some((old_page, old_health)), graphs)?,
            None => prepare_page(page, &path, None, graphs)?,
        };
        health.append(page_health);
        pages.push(page);
    }
    board.pages = pages;
    board.refresh_projects();
    Ok((board, health))
}
