//! Saving: preparing a board for writing, page by page and project by project — signing what
//! is unsigned, saving guards, signing link labels, and refusing new problems (contract §6.3,
//! §6.5).
//!
//! A signature is only ever computed from bytes that verified against the facts' hash, so text
//! can never be signed against source the graph has not seen. Text about a project that is not
//! connected stays as it is: it is signed once its project is connected again.

use rustc_hash::FxHashSet;

use super::BoardError;
use super::check::{
    Health, HealthKind, ProjectKey, Target, current_guard, group_problem, guard_problem, health, put_guard,
};
use super::graph::PageGraphs;
use super::model::{Board, Description, Page, ProjectName, Signature};

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

/// The current signature of a card's reference, a group's span, a labeled block, or a link's
/// ends.
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
            link_signature(page, &link.from, &link.to).ok_or_else(|| unverified(id))
        }
    }
}

/// The description, label, or link label `target` names.
pub(crate) fn description_mut<'a>(page: &'a mut Page, target: &Target) -> Option<&'a mut Description> {
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
    }
}

/// The projects a page's references are in: the board's own first, then the others it names
/// (cards, labels), without duplicates.
fn page_projects(page: &Page) -> Vec<Option<ProjectName>> {
    let mut projects = vec![None];
    for project in page
        .cards
        .iter()
        .filter_map(|card| card.reference.project())
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
        for key in page.references(project.as_ref()) {
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

    let links: Vec<(usize, Signature)> = page
        .links
        .iter()
        .enumerate()
        .filter(|(_, link)| link.label.as_ref().is_some_and(|label| label.signature.is_none()))
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
            (Some(a), Some(b)) => a.from == b.from && a.to == b.to,
            _ => false,
        },
    }
}

/// Prepares one page for writing over `previous` (the version being replaced, with its
/// health): guards the new version dropped come back from `previous`, unsigned text is signed,
/// guards are saved, link labels signed, and the result is refused when it introduces a missing
/// reference, a broken group, a held reference, or a broken link that `previous` did not
/// already have (with the same reference). Problems carried over unchanged are kept, so a page
/// with a placeholder can still be moved, retitled, or described elsewhere. Outdated text and
/// cards of projects that are not connected never refuse a save.
fn prepare_page(
    mut page: Page,
    previous: Option<(&Page, &Health)>,
    graphs: &PageGraphs<'_>,
) -> Result<(Page, Health), BoardError> {
    if let Some((old, _)) = previous {
        for guard in &old.guards {
            if page.guard(guard.project.as_ref(), guard.key.as_str()).is_none() {
                page.guards.push(guard.clone());
            }
        }
    }
    sign(&mut page, graphs)?;
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
    Ok((page, health))
}

/// Prepares `board` for writing over `previous` (the version being replaced, with its health,
/// if any), page by page against `graphs`: a page equal to the previous version's page keeps
/// its health unchecked; every other page is signed and checked (see the module documentation
/// and contract §6.5). [`Board::projects`] is refreshed. Returns the board and its health.
pub fn prepare_save(
    mut board: Board,
    previous: Option<(&Board, &Health)>,
    graphs: &PageGraphs<'_>,
) -> Result<(Board, Health), BoardError> {
    let mut health = Health::default();
    let mut pages = Vec::with_capacity(board.pages.len());
    for page in std::mem::take(&mut board.pages) {
        let old = previous.and_then(|(old, old_health)| {
            old.page(&page.id)
                .map(|old_page| (old_page, old_health.of_page(&page.id)))
        });
        let (page, page_health) = match &old {
            Some((old_page, old_health)) if **old_page == page => (page, old_health.clone()),
            Some((old_page, old_health)) => prepare_page(page, Some((old_page, old_health)), graphs)?,
            None => prepare_page(page, None, graphs)?,
        };
        health.append(page_health);
        pages.push(page);
    }
    board.pages = pages;
    board.refresh_projects();
    Ok((board, health))
}
