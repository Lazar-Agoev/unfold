//! Open boards: the last saved version of each board a window holds, its health against the
//! current graph, and what follows from them (the Problems list's board items, conflicts with
//! outside edits).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use unfold_reader::board::files::{BoardFile, Change, Revision};
use unfold_reader::board::{Board, BoardGraph, Health, HealthKind};

use super::store::{BoardStore, board_id};
use crate::commands::FolderExtras;
use crate::events::WorkspaceEvent;
use crate::host::HostState;
use crate::ipc::error::CommandResult;
use crate::problems::{ProblemCategory, ProblemItem};
use crate::shared::lock;
use crate::workspace::Workspace;

/// One open board: the version last loaded or saved, which the UI edits on top of.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSession {
    /// Its file.
    pub file: BoardFile,
    /// The board as last loaded or saved.
    pub board: Board,
    /// The file's revision then.
    pub revision: Revision,
    /// Its health against the current graph.
    pub health: Health,
    /// The outside change last reported to the holding window, so it is reported once.
    pub reported: Option<Change>,
}

/// The part of the graph `board` touches (empty before the first Compile).
pub fn load_graph(workspace: &Workspace, board: &Board) -> CommandResult<BoardGraph> {
    match workspace.readers() {
        Ok(pool) => Ok(BoardGraph::load(&pool.get()?, workspace.root(), board)?),
        Err(_) => Ok(BoardGraph::new()),
    }
}

/// The Boards view's label for a board: shown only when something is wrong.
pub fn health_label(health: &Health) -> Option<String> {
    let parts: Vec<String> = [
        (health.missing, "missing"),
        (health.outdated, "outdated"),
        (health.broken_groups, "broken groups"),
        (health.review, "to review"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, what)| {
        if count == 1 && what == "broken groups" {
            "1 broken group".to_owned()
        } else {
            format!("{count} {what}")
        }
    })
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The Problems list's items for the open boards: one per health item.
pub fn board_problems(sessions: &BTreeMap<String, BoardSession>) -> Vec<ProblemItem> {
    let mut items = Vec::new();
    for (id, session) in sessions {
        for item in &session.health.items {
            let what = match item.kind {
                HealthKind::Outdated => "description outdated",
                HealthKind::Missing => "missing reference",
                HealthKind::BrokenGroup => "broken group",
                HealthKind::Review => "needs review",
            };
            items.push(ProblemItem::new(
                ProblemCategory::Board,
                id.clone(),
                format!("{}: {what}: {}", session.board.title, item.message),
            ));
        }
    }
    items
}

/// Publishes the open boards' health to the Problems list. Call without holding the sessions
/// lock: it sends `problems_changed`.
pub fn publish_problems(workspace: &Workspace, extras: &FolderExtras) {
    let problems = board_problems(&lock(&extras.boards));
    workspace.set_board_problems(problems);
}

/// Coalesces health refreshes: graph updates can arrive faster than open boards' health is
/// recomputed, so at most one refresh runs per folder, and requests made while it runs make it
/// run once more (with the newest graph) instead of starting threads that race each other.
#[derive(Debug, Default)]
pub struct HealthRefresh {
    pending: AtomicBool,
    running: AtomicBool,
}

impl HealthRefresh {
    /// Asks for a refresh; whether the caller must start the refreshing thread (none runs).
    pub fn request(&self) -> bool {
        self.pending.store(true, Ordering::SeqCst);
        !self.running.swap(true, Ordering::SeqCst)
    }

    /// Gives up the running slot (the thread couldn't start, or the refresh ended).
    pub fn release(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// After a refresh: whether another was requested meanwhile and this thread takes it.
    fn again(&self) -> bool {
        self.release();
        self.pending.load(Ordering::SeqCst) && !self.running.swap(true, Ordering::SeqCst)
    }
}

/// Releases the running slot if a refresh panics, so later refreshes still start.
struct ReleaseOnPanic<'a>(&'a HealthRefresh);

impl Drop for ReleaseOnPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.release();
        }
    }
}

/// Recomputes every open board's health after the graph changed (runs on its own thread, started
/// when [`HealthRefresh::request`] says so), until no newer request is waiting.
pub fn refresh_health(_state: &Arc<HostState>, workspace: &Workspace, extras: &FolderExtras) {
    let _guard = ReleaseOnPanic(&extras.health);
    loop {
        extras.health.pending.store(false, Ordering::SeqCst);
        refresh_once(workspace, extras);
        if !extras.health.again() {
            return;
        }
    }
}

fn refresh_once(workspace: &Workspace, extras: &FolderExtras) {
    let boards: Vec<(String, Board, Revision)> = lock(&extras.boards)
        .iter()
        .map(|(id, session)| (id.clone(), session.board.clone(), session.revision))
        .collect();
    let mut fresh = Vec::with_capacity(boards.len());
    for (id, board, revision) in boards {
        if let Ok(graph) = load_graph(workspace, &board) {
            fresh.push((id, revision, unfold_reader::board::health(&board, &graph)));
        }
    }
    {
        let mut sessions = lock(&extras.boards);
        for (id, revision, health) in fresh {
            // A board saved meanwhile already has the health of what it saved.
            if let Some(session) = sessions.get_mut(&id)
                && session.revision == revision
            {
                session.health = health;
            }
        }
    }
    publish_problems(workspace, extras);
}

/// How often a board file caught mid-write is read again.
const SETTLE_PAUSE: Duration = Duration::from_millis(25);

/// How long a board file that isn't a valid board must keep the same bytes before it counts as
/// its writer's final version (a writer that left it broken) rather than a write in progress.
const SETTLE_QUIET: Duration = Duration::from_millis(1000);

/// The longest a check waits for a board file to settle; after it, the bytes then are reported.
const SETTLE_LIMIT: Duration = Duration::from_secs(5);

/// Tells each window holding an open board that its file changed on disk outside the app
/// (runs on its own thread after `boards_changed`).
///
/// Writers that don't replace the file atomically (truncate, then write) let a check catch the
/// file empty or half written, so a changed file is reported only once it is a valid board, or
/// once its bytes have stopped changing ([`SETTLE_QUIET`]). Checks run one at a time and each
/// reports the revision it settled on, so the latest `board_conflict` a window gets carries the
/// file's final revision.
pub fn check_disk(state: &Arc<HostState>, workspace: &Workspace, extras: &FolderExtras) {
    let _serial = lock(&extras.disk_check);
    let store = BoardStore::new(workspace);
    let open: Vec<(String, BoardFile, Revision)> = lock(&extras.boards)
        .iter()
        .map(|(id, session)| (id.clone(), session.file.clone(), session.revision))
        .collect();
    let mut conflicts = Vec::new();
    for (id, file, revision) in open {
        // Settled without the sessions lock: board commands mustn't wait for a slow writer.
        let Some(change) = settled_change(&store, &file, revision) else {
            continue;
        };
        let mut sessions = lock(&extras.boards);
        // A board closed, saved, or reloaded meanwhile is compared again by the next check.
        let Some(session) = sessions
            .get_mut(&id)
            .filter(|session| session.revision == revision)
        else {
            continue;
        };
        if change == Change::Unchanged || session.reported == Some(change) {
            continue;
        }
        session.reported = Some(change);
        let Some(window) = workspace.claims().holder(&id) else {
            continue;
        };
        let disk_revision = match change {
            Change::Changed(revision) => Some(revision.0.to_string()),
            Change::Missing | Change::Unchanged => None,
        };
        conflicts.push((
            window,
            WorkspaceEvent::BoardConflict {
                folder: workspace.folder().to_owned(),
                board: board_id(&session.file),
                saved_revision: session.revision.0.to_string(),
                disk_revision,
            },
        ));
    }
    for (window, event) in conflicts {
        state.deliver(&[window], &event);
    }
}

/// How a board file compares with `revision` once its writer is done: a changed file is read
/// again until it is a valid board, or its bytes stay the same for [`SETTLE_QUIET`], or
/// [`SETTLE_LIMIT`] passes. `None` when the file can't be examined.
fn settled_change(store: &BoardStore<'_>, file: &BoardFile, revision: Revision) -> Option<Change> {
    let started = Instant::now();
    let mut quiet_since = started;
    let mut last: Option<Change> = None;
    loop {
        let change = store.change(file, revision).ok()?;
        if !matches!(change, Change::Changed(_)) {
            return Some(change);
        }
        // The bytes parsed are the bytes compared only if the file didn't change in between.
        if store
            .load(file)
            .is_ok_and(|loaded| Change::Changed(loaded.revision) == change)
        {
            return Some(change);
        }
        let now = Instant::now();
        if last != Some(change) {
            last = Some(change);
            quiet_since = now;
        }
        if now.duration_since(quiet_since) >= SETTLE_QUIET || now.duration_since(started) >= SETTLE_LIMIT {
            return Some(change);
        }
        std::thread::sleep(SETTLE_PAUSE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_coalesce_into_one_running_thread() {
        let refresh = HealthRefresh::default();
        assert!(refresh.request(), "the first request starts a thread");
        assert!(!refresh.request(), "a request while it runs doesn't");
        // The running thread clears the pending flag before refreshing.
        refresh.pending.store(false, Ordering::SeqCst);
        assert!(!refresh.request());
        assert!(refresh.again(), "a request made meanwhile runs once more");
        refresh.pending.store(false, Ordering::SeqCst);
        assert!(!refresh.again(), "then the thread ends");
        assert!(refresh.request(), "and the next request starts a new one");
    }
}
