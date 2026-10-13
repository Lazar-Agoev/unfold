//! The board commands' state of one live project ([`BoardExtras`]): the open boards' sessions
//! and the health cache of every board.
//!
//! A **session** is the version of an open board last loaded or saved, which the window edits on
//! top of: the board, its file's revision, and its health. A save writes the window's version
//! over the file, whatever it holds; an outside change to the file reloads the session
//! ([`super::health::check_disk`]), keeping the version it replaced until the next save: a window
//! whose save is failing ignores the reload and still edits on that version.
//!
//! The **health cache** holds the health of every board of the project (Home's Health line and
//! the Problems list), each under the [`HealthKey`] it was computed for: a cached health is
//! current while the board's file and the project's graph are the ones it was computed from.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};

use unfold_reader::board::files::{Change, Loaded, Revision};
use unfold_reader::board::{Board, Health, HealthKind, board_health};

use super::graphs::{HealthKey, Source};
use crate::commands::count;
use crate::commands::projects::BoardHealthCount;
use crate::ipc::error::CommandResult;
use crate::problems::{ProblemCategory, ProblemItem};
use crate::shared::lock;
use crate::workspace::Workspace;

/// One open board. See the [module documentation](self).
#[derive(Debug, Clone)]
pub(super) struct Session {
    /// The board as last loaded or saved.
    pub(super) board: Board,
    /// Its file's revision then.
    pub(super) revision: Revision,
    /// Its health.
    pub(super) health: Health,
    /// The outside change last found that the session didn't take (the file gone, or not a
    /// valid board), so it is handled once.
    pub(super) skipped: Option<Change>,
    /// The versions (with their health) that outside changes replaced since the board was last
    /// saved or loaded by a command, oldest first. A window whose save was failing ignored those
    /// reloads, so its Retry is checked against them too.
    pub(super) replaced: Vec<(Board, Health)>,
}

impl Session {
    /// A session on a board read from disk.
    pub(super) fn new(loaded: Loaded, health: Health) -> Self {
        Self {
            board: loaded.board,
            revision: loaded.revision,
            health,
            skipped: None,
            replaced: Vec::new(),
        }
    }

    /// Takes `reloaded`, an outside change's version, keeping the version it replaces.
    pub(super) fn reload(&mut self, reloaded: Self) {
        let previous = std::mem::replace(self, reloaded);
        self.replaced = previous.replaced;
        self.replaced.push((previous.board, previous.health));
    }
}

/// A board's cached health.
#[derive(Debug, Clone)]
struct Cached {
    key: HealthKey,
    title: String,
    health: Health,
}

/// The board commands' state of one live project. See the [module documentation](self).
/// Lock order: sessions, then the health cache.
#[derive(Debug, Default)]
pub struct BoardExtras {
    sessions: Mutex<BTreeMap<String, Session>>,
    health: Mutex<BTreeMap<String, Cached>>,
    /// Orders disk checks, so outside changes are taken in the order they settle.
    pub(super) disk_check: Mutex<()>,
}

impl BoardExtras {
    /// The open boards' sessions, by board id.
    pub(super) fn sessions(&self) -> MutexGuard<'_, BTreeMap<String, Session>> {
        lock(&self.sessions)
    }

    /// The cached health of board `id` if it is current for `key`.
    fn cached(&self, id: &str, key: &HealthKey) -> Option<Health> {
        lock(&self.health)
            .get(id)
            .filter(|cached| cached.key == *key)
            .map(|cached| cached.health.clone())
    }

    /// Caches the health of board `id`, as `board` read from a file with `revision` (nothing
    /// while the project has no graph: see [`Source::key`]).
    pub(super) fn cache(
        &self,
        source: &Source,
        id: &str,
        board: &Board,
        revision: Revision,
        health: &Health,
    ) {
        if let Some(key) = source.key(revision) {
            lock(&self.health).insert(
                id.to_owned(),
                Cached {
                    key,
                    title: board.title.clone(),
                    health: health.clone(),
                },
            );
        }
    }

    /// The cached health of board `id` if it was computed for file revision `revision`.
    pub(super) fn health_at(&self, id: &str, revision: Revision) -> Option<Health> {
        lock(&self.health)
            .get(id)
            .filter(|cached| cached.key.revision == revision)
            .map(|cached| cached.health.clone())
    }

    /// Board `id`'s file went from revision `from` to `to` without a change its health depends
    /// on (its folder moved): a health cached for `from` holds for `to`.
    pub(super) fn rekey(&self, id: &str, from: Revision, to: Revision) {
        if let Some(cached) = lock(&self.health).get_mut(id)
            && cached.key.revision == from
        {
            cached.key.revision = to;
        }
    }

    /// Forgets board `id`'s health (it was deleted).
    pub(super) fn forget(&self, id: &str) {
        lock(&self.health).remove(id);
    }

    /// Keeps only the health of the boards `ids`.
    pub(super) fn retain(&self, ids: &BTreeSet<String>) {
        lock(&self.health).retain(|id, _| ids.contains(id));
    }

    /// The Problems list's board items: one per health item, by board id.
    fn problems(&self) -> Vec<ProblemItem> {
        let cache = lock(&self.health);
        let mut items = Vec::new();
        for (id, cached) in cache.iter() {
            for item in &cached.health.items {
                let what = match item.kind {
                    HealthKind::Missing => "missing reference",
                    HealthKind::Outdated => "description outdated",
                    HealthKind::BrokenGroup => "broken group",
                    HealthKind::Review => "needs review",
                    HealthKind::BrokenLink => "broken link",
                };
                items.push(ProblemItem::new(
                    ProblemCategory::Board,
                    id.clone(),
                    format!("{}: {what}: {}", cached.title, item.message),
                ));
            }
        }
        items
    }

    /// The boards whose health has items, most items first (then by id).
    pub(crate) fn health_rows(&self) -> Vec<BoardHealthCount> {
        let mut rows: Vec<BoardHealthCount> = lock(&self.health)
            .iter()
            .filter(|(_, cached)| !cached.health.is_healthy())
            .map(|(id, cached)| BoardHealthCount {
                id: id.clone(),
                title: cached.title.clone(),
                items: count(cached.health.items.len()),
            })
            .collect();
        rows.sort_by(|a, b| b.items.cmp(&a.items).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    /// Forgets the sessions of `boards` (they were closed).
    pub(crate) fn forget_sessions(&self, boards: &[String]) {
        let mut sessions = self.sessions();
        for board in boards {
            sessions.remove(board);
        }
    }
}

/// Board `id`'s health as read from disk (`loaded`), from the cache when it is current, else
/// computed and cached; `None` while the project has no graph.
pub(super) fn health_of(
    extras: &BoardExtras,
    source: &Source,
    id: &str,
    loaded: &Loaded,
) -> CommandResult<Option<Health>> {
    let Some(key) = source.key(loaded.revision) else {
        return Ok(None);
    };
    if let Some(health) = extras.cached(id, &key) {
        return Ok(Some(health));
    }
    let health = board_health(&loaded.board, &source.board(&loaded.board)?);
    extras.cache(source, id, &loaded.board, loaded.revision, &health);
    Ok(Some(health))
}

/// Publishes the health of every cached board to the Problems list. Call with no board lock
/// held: it sends `problems_changed`.
pub(super) fn publish_problems(workspace: &Workspace, extras: &BoardExtras) {
    workspace.set_board_problems(extras.problems());
}
