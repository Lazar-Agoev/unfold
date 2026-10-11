//! The items of one page and the references between them: cards, groups, frames, resource
//! cards, links, and notes share one id space per page, and the rules that need nothing but the
//! page (no graph) are checked here, on every parse and before every write.
//!
//! - **Groups** belong to a card of the page.
//! - **Frames** hold one or more cards, resources, notes, or frames of the page; an item is in at
//!   most one frame, frames never hold each other in a cycle, and they nest at most
//!   [`MAX_FRAME_DEPTH`] deep.
//! - **Links** join two different cards, resources, or frames. A `relates` connection joins two
//!   tables (by columns a table that lists its own columns has); a step leaves from a block card.
//! - **Notes** attach to a card, frame, resource, or link.
//!
//! Removing an item ([`remove_items`]) removes what would be left dangling, so a page stays
//! readable.

use rustc_hash::{FxHashMap, FxHashSet};

use super::BoardError;
use super::model::{CardRef, MAX_FRAME_DEPTH, Page, ResourceKind};
use super::parse::Path;

/// What an id names on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Card,
    Group,
    Frame,
    Resource,
    Link,
    Note,
}

impl Kind {
    /// The kind in words, after "a".
    fn word(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Group => "group",
            Self::Frame => "frame",
            Self::Resource => "resource",
            Self::Link => "link",
            Self::Note => "note",
        }
    }

    /// The page's array of items of this kind.
    fn array(self) -> &'static str {
        match self {
            Self::Card => "cards",
            Self::Group => "groups",
            Self::Frame => "frames",
            Self::Resource => "resources",
            Self::Link => "links",
            Self::Note => "notes",
        }
    }
}

/// The kind of every item id of a page; refuses an id used twice.
fn kinds<'p>(page: &'p Page, path: &Path) -> Result<FxHashMap<&'p str, Kind>, BoardError> {
    let ids: [(Kind, Vec<&str>); 6] = [
        (
            Kind::Card,
            page.cards.iter().map(|item| item.id.as_str()).collect(),
        ),
        (
            Kind::Group,
            page.groups.iter().map(|item| item.id.as_str()).collect(),
        ),
        (
            Kind::Frame,
            page.frames.iter().map(|item| item.id.as_str()).collect(),
        ),
        (
            Kind::Resource,
            page.resources.iter().map(|item| item.id.as_str()).collect(),
        ),
        (
            Kind::Link,
            page.links.iter().map(|item| item.id.as_str()).collect(),
        ),
        (
            Kind::Note,
            page.notes.iter().map(|item| item.id.as_str()).collect(),
        ),
    ];
    let mut kinds = FxHashMap::default();
    for (kind, list) in ids {
        for (index, id) in list.into_iter().enumerate() {
            if kinds.insert(id, kind).is_some() {
                return Err(path
                    .join(kind.array())
                    .join(index)
                    .join("id")
                    .fail(format!("duplicate id `{id}`")));
            }
        }
    }
    Ok(kinds)
}

/// Fails at `path` unless `id` names an item of one of the kinds `allowed` (described as
/// `what`).
fn expect(
    kinds: &FxHashMap<&str, Kind>,
    id: &str,
    allowed: &[Kind],
    what: &str,
    path: &Path,
) -> Result<(), BoardError> {
    match kinds.get(id) {
        Some(kind) if allowed.contains(kind) => Ok(()),
        Some(kind) => Err(path.fail(format!("`{id}` is a {}; {what}", kind.word()))),
        None => Err(path.fail(format!("refers to an unknown item `{id}`"))),
    }
}

/// Checks the references between the items of `page` (at `path`). See the
/// [module documentation](self).
pub(super) fn check_items(page: &Page, path: &Path) -> Result<(), BoardError> {
    let kinds = kinds(page, path)?;
    for (index, group) in page.groups.iter().enumerate() {
        if kinds.get(group.card.as_str()) != Some(&Kind::Card) {
            return Err(path
                .join("groups")
                .join(index)
                .join("card")
                .fail(format!("refers to an unknown card `{}`", group.card)));
        }
    }
    check_frames(page, &kinds, path)?;
    for (index, link) in page.links.iter().enumerate() {
        let link_path = path.join("links").join(index);
        for (end, id) in [("from", &link.from), ("to", &link.to)] {
            let what = "a link joins cards, resources, and frames";
            expect(
                &kinds,
                id,
                &[Kind::Card, Kind::Resource, Kind::Frame],
                what,
                &link_path.join(end),
            )?;
        }
        if link.from == link.to {
            return Err(link_path.join("to").fail("must be another item than `from`"));
        }
        if link.relates() {
            let ends = [
                ("from", &link.from, "fromColumn", &link.from_column),
                ("to", &link.to, "toColumn", &link.to_column),
            ];
            for (end, id, column_field, column) in ends {
                let Some(table) = page
                    .resource(id)
                    .filter(|resource| resource.kind == ResourceKind::Table)
                else {
                    return Err(link_path
                        .join(end)
                        .fail("must be a table: a `relates` connection joins two tables"));
                };
                if let Some(column) = column
                    && table.type_key.is_none()
                    && !table.columns.iter().any(|known| &known.name == column)
                {
                    return Err(link_path
                        .join(column_field)
                        .fail(format!("names no column of table `{id}`")));
                }
            }
        }
        let from_block = page
            .card(&link.from)
            .is_some_and(|card| matches!(card.reference, CardRef::Block { .. }));
        if link.step.is_some() && !from_block {
            return Err(link_path.join("step").fail(
                "a step is a statement or call inside the `from` card's block: `from` must be a block card",
            ));
        }
    }
    for (index, note) in page.notes.iter().enumerate() {
        if let Some(target) = &note.attach {
            expect(
                &kinds,
                target,
                &[Kind::Card, Kind::Frame, Kind::Resource, Kind::Link],
                "a note attaches to a card, frame, resource, or link",
                &path.join("notes").join(index).join("attach"),
            )?;
        }
    }
    Ok(())
}

/// Frame membership: members exist and are cards, resources, notes, or frames; an item is in
/// at most one frame; no cycles; at most [`MAX_FRAME_DEPTH`] deep.
fn check_frames(page: &Page, kinds: &FxHashMap<&str, Kind>, path: &Path) -> Result<(), BoardError> {
    let mut parent: FxHashMap<&str, &str> = FxHashMap::default();
    for (index, frame) in page.frames.iter().enumerate() {
        for (position, member) in frame.members.iter().enumerate() {
            let member_path = path.join("frames").join(index).join("members").join(position);
            expect(
                kinds,
                member,
                &[Kind::Card, Kind::Resource, Kind::Note, Kind::Frame],
                "a frame holds cards, resources, notes, and frames",
                &member_path,
            )?;
            if let Some(other) = parent.insert(member, &frame.id) {
                return Err(member_path.fail(format!(
                    "`{member}` is already in frame `{other}`; an item is in at most one frame"
                )));
            }
        }
    }
    // Walk up from every frame: a frame met twice closes a cycle.
    for (index, frame) in page.frames.iter().enumerate() {
        let mut seen = FxHashSet::default();
        let mut at = frame.id.as_str();
        while let Some(&up) = parent.get(at) {
            if !seen.insert(up) {
                return Err(path
                    .join("frames")
                    .join(index)
                    .fail("is inside itself: frames can't hold each other in a cycle"));
            }
            at = up;
        }
        if seen.len() >= MAX_FRAME_DEPTH {
            return Err(path.join("frames").join(index).fail(format!(
                "is nested {} frames deep; frames nest at most {MAX_FRAME_DEPTH} deep",
                seen.len() + 1
            )));
        }
    }
    Ok(())
}

/// The most items a position walks through: a link's end frames, and their nested frames.
const POSITION_DEPTH: usize = MAX_FRAME_DEPTH + 2;

/// Where item `id` is on the canvas, as far as the file says: a card's or resource's position,
/// the top-left of a frame's cards, resources, and nested frames, or the midpoint of a link's
/// ends. `None` for anything else (a frame of notes only).
fn position(page: &Page, id: &str, depth: usize) -> Option<[f64; 2]> {
    if depth > POSITION_DEPTH {
        return None;
    }
    if let Some(card) = page.card(id) {
        return Some(card.at);
    }
    if let Some(resource) = page.resource(id) {
        return Some(resource.at);
    }
    if let Some(frame) = page.frame(id) {
        return frame
            .members
            .iter()
            .filter_map(|member| position(page, member, depth + 1))
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1])]);
    }
    let link = page.link(id)?;
    let (from, to) = (
        position(page, &link.from, depth + 1)?,
        position(page, &link.to, depth + 1)?,
    );
    Some([(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0])
}

/// Removes items `ids` from `page` with everything that would dangle: a removed card's groups,
/// the links that end at a removed item, removed items from the frames that hold them (a frame
/// left empty is removed too), and notes attached to a removed item, which stay where they were,
/// unattached. Removing a frame keeps its members.
pub(crate) fn remove_items(page: &mut Page, ids: &[&str]) {
    let mut removed: FxHashSet<String> = ids.iter().map(|id| (*id).to_owned()).collect();
    loop {
        let before = removed.len();
        let dangling: Vec<String> = page
            .groups
            .iter()
            .filter(|group| removed.contains(&group.card))
            .map(|group| group.id.clone())
            .chain(
                page.links
                    .iter()
                    .filter(|link| removed.contains(&link.from) || removed.contains(&link.to))
                    .map(|link| link.id.clone()),
            )
            .chain(
                page.frames
                    .iter()
                    .filter(|frame| frame.members.iter().all(|member| removed.contains(member)))
                    .map(|frame| frame.id.clone()),
            )
            .collect();
        removed.extend(dangling);
        if removed.len() == before {
            break;
        }
    }
    let detached: Vec<(usize, [f64; 2])> = page
        .notes
        .iter()
        .enumerate()
        .filter(|(_, note)| !removed.contains(&note.id))
        .filter_map(|(index, note)| {
            let target = note.attach.as_ref().filter(|target| removed.contains(*target))?;
            let origin = position(page, target, 0).unwrap_or([0.0, 0.0]);
            Some((index, [origin[0] + note.at[0], origin[1] + note.at[1]]))
        })
        .collect();
    for (index, at) in detached {
        let note = &mut page.notes[index];
        note.at = at;
        note.attach = None;
    }
    let kept = |id: &String| !removed.contains(id);
    page.cards.retain(|card| kept(&card.id));
    page.groups.retain(|group| kept(&group.id));
    page.frames.retain(|frame| kept(&frame.id));
    for frame in &mut page.frames {
        frame.members.retain(kept);
    }
    page.resources.retain(|resource| kept(&resource.id));
    page.links.retain(|link| kept(&link.id));
    page.notes.retain(|note| kept(&note.id));
}
