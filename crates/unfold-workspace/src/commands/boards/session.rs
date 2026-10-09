//! The board commands' state of one live project ([`BoardExtras`]): the open boards' sessions
//! and the health cache of every board.
//!
//! A **session** is the version of an open board last loaded or saved, which the window edits on
//! top of: its working file (draft, changes copy, or published file), the board, the file's
//! revision, and its health. Saves compare the working file with the session's revision, so an
//! outside edit is never overwritten silently (`board_conflict`).
//!
//! The **health cache** holds the health of every board of the project (Home's Health line and
//! the Problems list), each under the [`HealthKey`] it was computed for: a cached health is
//! current while the board's working file and the graphs of the projects its cards resolve to
//! are the ones it was computed from.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};

use unfold_reader::board::files::{BoardFile, Change, Loaded, Revision};
use unfold_reader::board::{Board, Health, HealthKind, board_health};

use super::files::{Working, Written, working_file};
use super::graphs::{HealthKey, Resolver};
use super::types::PublishedState;
use crate::commands::count;
use crate::commands::projects::BoardHealthCount;
use crate::events::WorkingFile;
use crate::ipc::error::CommandResult;
use crate::problems::{ProblemCategory, ProblemItem};
use crate::projects::ProjectId;
use crate::shared::lock;
use crate::workspace::Workspace;

/// One open board. See the [module documentation](self).
#[derive(Debug, Clone)]
pub(super) struct Session {
    /// The board's own file (its id).
    pub(super) file: BoardFile,
    /// Which file its working version is in.
    pub(super) kind: WorkingFile,
    /// The working version as last loaded or saved.
    pub(super) board: Board,
    /// The working file's revision then.
    pub(super) revision: Revision,
    /// For a changes copy, the published revision it started from.
    pub(super) base: Option<Revision>,
    /// Its health.
    pub(super) health: Health,
    /// For a published board, its publish state then.
    pub(super) published: Option<PublishedState>,
    /// The outside change last reported to the holding window, so it is reported once.
    pub(super) reported: Option<Change>,
}

impl Session {
    /// A session on a working version read from disk.
    pub(super) fn new(
        file: BoardFile,
        working: Working,
        health: Health,
        published: Option<PublishedState>,
    ) -> Self {
        Self {
            file,
            kind: working.kind,
            board: working.loaded.board,
            revision: working.loaded.revision,
            base: working.loaded.base,
            health,
            published,
            reported: None,
        }
    }

    /// The file its working version is in.
    pub(super) fn working_file(&self) -> BoardFile {
        working_file(&self.file, self.kind)
    }

    /// The published revision a changes copy written now starts from (see [`Working::base`]).
    pub(super) fn base(&self) -> Option<Revision> {
        match self.kind {
            WorkingFile::Changes => self.base,
            WorkingFile::Published => Some(self.revision),
            WorkingFile::Draft => None,
        }
    }

    /// The session after `written` was written, with `health`.
    pub(super) fn wrote(&mut self, written: Written, health: Health) {
        self.kind = written.kind;
        self.board = written.board;
        self.revision = written.revision;
        self.base = written.base;
        self.published = written.published;
        self.health = health;
        self.reported = None;
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
    /// Orders disk checks, so the last `board_conflict` carries a file's final revision.
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
    /// while the project has no graph: see [`Resolver::key`]).
    pub(super) fn cache(
        &self,
        resolver: &mut Resolver<'_>,
        id: &str,
        board: &Board,
        revision: Revision,
        health: &Health,
    ) {
        if let Some(key) = resolver.key(board, revision) {
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

    /// The cached health of board `id` if it was computed for working file revision `revision`.
    pub(super) fn health_at(&self, id: &str, revision: Revision) -> Option<Health> {
        lock(&self.health)
            .get(id)
            .filter(|cached| cached.key.revision == revision)
            .map(|cached| cached.health.clone())
    }

    /// Board `id`'s working file went from revision `from` to `to` without a change its health
    /// depends on (its folder moved): a health cached for `from` holds for `to`.
    pub(super) fn rekey(&self, id: &str, from: Revision, to: Revision) {
        if let Some(cached) = lock(&self.health).get_mut(id)
            && cached.key.revision == from
        {
            cached.key.revision = to;
        }
    }

    /// Forgets board `id`'s health (it was deleted, or its id changed).
    pub(super) fn forget(&self, id: &str) {
        lock(&self.health).remove(id);
    }

    /// Keeps only the health of the boards `ids`.
    pub(super) fn retain(&self, ids: &BTreeSet<String>) {
        lock(&self.health).retain(|id, _| ids.contains(id));
    }

    /// Whether a board's cached health depends on project `project`'s graph.
    pub(crate) fn references(&self, project: &ProjectId) -> bool {
        lock(&self.health)
            .values()
            .any(|cached| cached.key.references(project))
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
                    HealthKind::NotConnected => "project not connected",
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
}

/// Board `id`'s health as read from disk (`loaded`), from the cache when it is current, else
/// computed and cached; `None` while the project has no graph.
pub(super) fn health_of(
    extras: &BoardExtras,
    resolver: &mut Resolver<'_>,
    id: &str,
    loaded: &Loaded,
) -> CommandResult<Option<Health>> {
    let Some(key) = resolver.key(&loaded.board, loaded.revision) else {
        return Ok(None);
    };
    if let Some(health) = extras.cached(id, &key) {
        return Ok(Some(health));
    }
    let graphs = resolver.board_graphs(&loaded.board)?;
    let health = board_health(&loaded.board, &graphs.page());
    extras.cache(resolver, id, &loaded.board, loaded.revision, &health);
    Ok(Some(health))
}

/// Publishes the health of every cached board to the Problems list. Call with no board lock
/// held: it sends `problems_changed`.
pub(super) fn publish_problems(workspace: &Workspace, extras: &BoardExtras) {
    workspace.set_board_problems(extras.problems());
}
