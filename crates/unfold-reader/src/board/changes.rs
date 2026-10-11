//! How many unpublished changes a published board has (decision U-43): a
//! structural count of the differences between the working version and the published one, so
//! the header can say "2 unpublished changes", and a copy with none can be deleted.

use std::collections::BTreeMap;

use super::model::{Board, LabelEntry, Page, ProjectName};

/// The number of differences from `published` to `working`: the title (1), the folder (1), the
/// page order (1), each page added or removed (1), and for each page in both its title (1) and
/// default view (1), each card, group, link, frame, resource, and note added, removed, or
/// changed (by id), and each label added, removed, or changed (by project and key). Guards (and
/// so a reference's signature alone) are not counted. Zero means the two are the same board.
pub fn changes(working: &Board, published: &Board) -> u32 {
    let mut count =
        u32::from(working.title != published.title) + u32::from(working.folder != published.folder);
    let shared = |pages: &[Page], other: &Board| -> Vec<String> {
        pages
            .iter()
            .filter(|page| other.page(&page.id).is_some())
            .map(|page| page.id.clone())
            .collect()
    };
    count += u32::from(shared(&working.pages, published) != shared(&published.pages, working));
    for page in &working.pages {
        count += match published.page(&page.id) {
            Some(before) => page_changes(page, before),
            None => 1,
        };
    }
    count += number(
        published
            .pages
            .iter()
            .filter(|page| working.page(&page.id).is_none())
            .count(),
    );
    count
}

fn number(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The differences between two versions of one page.
fn page_changes(page: &Page, before: &Page) -> u32 {
    u32::from(page.title != before.title)
        + u32::from(page.default_view != before.default_view)
        + by_id(&page.cards, &before.cards, |card| &card.id)
        + by_id(&page.groups, &before.groups, |group| &group.id)
        + by_id(&page.links, &before.links, |link| &link.id)
        + by_id(&page.frames, &before.frames, |frame| &frame.id)
        + by_id(&page.resources, &before.resources, |resource| &resource.id)
        + by_id(&page.notes, &before.notes, |note| &note.id)
        + by_id(&page.labels, &before.labels, label_key)
}

fn label_key(entry: &LabelEntry) -> (Option<&ProjectName>, &str) {
    (entry.project.as_ref(), entry.key.as_str())
}

/// The items added, removed, or changed between `now` and `before`, matched by `key`.
fn by_id<'a, T: PartialEq, K: Ord>(now: &'a [T], before: &'a [T], key: impl Fn(&'a T) -> K) -> u32 {
    let before: BTreeMap<K, &T> = before.iter().map(|item| (key(item), item)).collect();
    let mut seen = 0;
    let mut count = 0;
    for item in now {
        match before.get(&key(item)) {
            Some(old) => {
                seen += 1;
                count += u32::from(*old != item);
            }
            None => count += 1,
        }
    }
    // Every item of `before` not seen in `now` was removed.
    count + number(before.len().saturating_sub(seen))
}
