//! Keeping a live project's board health and open sessions in step with the disk and the
//! graph: the health refresh after graph updates, and the reload of an open board whose file
//! changed outside the app (`board_reloaded`).

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use unfold_reader::board::files::{BoardFile, Change, Revision};

use super::files::{Files, parse_id};
use super::graphs::Source;
use super::session::{BoardExtras, health_of, publish_problems};
use super::{board_state, load_session};
use crate::events::WorkspaceEvent;
use crate::host::HostState;
use crate::projects::Projects;
use crate::shared::lock;
use crate::workspace::Workspace;

/// Brings the cached health of a live project's boards up to date: of the boards `only` names,
/// or of every board (then boards that are gone are forgotten). Open sessions take their
/// board's new health, and the Problems list gets the board items.
pub(crate) fn refresh(workspace: &Workspace, extras: &BoardExtras, only: Option<&[String]>) {
    let files = Files::live(workspace);
    let Ok(source) = Source::live(workspace) else {
        return;
    };
    let boards: Vec<(String, BoardFile)> = match only {
        Some(ids) => ids
            .iter()
            .filter_map(|id| parse_id(id).ok().map(|file| (id.clone(), file)))
            .collect(),
        None => match files.boards() {
            Ok(boards) => boards.into_iter().map(|file| (file.relative(), file)).collect(),
            Err(_) => return,
        },
    };
    for (id, file) in &boards {
        // A board that can't be read has no health; its listing says why.
        let fresh = files
            .load(file)
            .ok()
            .and_then(|loaded| health_of(extras, &source, id, &loaded).ok().flatten());
        if fresh.is_none() {
            extras.forget(id);
        }
    }
    if only.is_none() {
        extras.retain(&boards.into_iter().map(|(id, _)| id).collect::<BTreeSet<_>>());
    }
    for (id, session) in extras.sessions().iter_mut() {
        // A session the disk no longer matches keeps its health until it is reloaded or saved.
        if let Some(health) = extras.health_at(id, session.revision) {
            session.health = health;
        }
    }
    publish_problems(workspace, extras);
}

/// How often a board file caught mid-write is read again.
const SETTLE_PAUSE: Duration = Duration::from_millis(25);

/// How long a board file that isn't a valid board must keep the same bytes before it counts as
/// its writer's final version (a writer that left it broken) rather than a write in progress.
const SETTLE_QUIET: Duration = Duration::from_millis(1000);

/// The longest a check waits for a board file to settle; after it, the bytes then are taken.
const SETTLE_LIMIT: Duration = Duration::from_secs(5);

/// Reloads each open board of a live project whose file changed on disk outside the app, and
/// tells the window that opened it with `board_reloaded` (runs on its own thread after
/// `boards_changed`).
///
/// - A file that is a valid board replaces the session (its health computed again), and the
///   event carries the board as the window now shows it.
/// - A file that is gone is reported once with no board; the session keeps the version last
///   loaded or saved, which the next save writes back.
/// - A file that isn't a valid board is left alone: the session keeps its version, and the next
///   save writes it over the file.
///
/// Writers that don't replace the file atomically (truncate, then write) let a check catch the
/// file empty or half written, so a changed file is read only once it is a valid board, or once
/// its bytes have stopped changing ([`SETTLE_QUIET`]). Checks run one at a time, so a window gets
/// the reloads in the order the file's versions settled.
pub(crate) fn check_disk(host: &HostState, projects: &Projects, workspace: &Workspace, extras: &BoardExtras) {
    let _serial = lock(&extras.disk_check);
    let Ok(project) = projects.handle(workspace.project()) else {
        return;
    };
    let Ok(source) = Source::live(workspace) else {
        return;
    };
    let files = Files::live(workspace);
    let open: Vec<(String, BoardFile, Revision, Option<Change>)> = extras
        .sessions()
        .iter()
        .filter_map(|(id, session)| Some((id.clone(), parse_id(id).ok()?, session.revision, session.skipped)))
        .collect();
    for (id, file, revision, skipped) in open {
        let Ok(now) = files.dirs.change(&file, revision) else {
            continue;
        };
        if now == Change::Unchanged || Some(now) == skipped {
            continue;
        }
        // Settled without the sessions lock: board commands mustn't wait for a slow writer.
        let Some(change) = settled_change(&files, &file, revision) else {
            continue;
        };
        let mut sessions = extras.sessions();
        // A board closed, saved, or reloaded meanwhile is compared again by the next check.
        let Some(session) = sessions
            .get_mut(&id)
            .filter(|session| session.revision == revision)
        else {
            continue;
        };
        let state = match change {
            Change::Unchanged => continue,
            Change::Missing => {
                session.skipped = Some(change);
                None
            }
            Change::Changed(_) => match load_session(&files, extras, &source, &id, &file) {
                Ok(reloaded) => {
                    let state = board_state(workspace.project(), &id, &file, &reloaded);
                    *session = reloaded;
                    Some(Box::new(state))
                }
                Err(_) => {
                    session.skipped = Some(change);
                    continue;
                }
            },
        };
        // Sent before the sessions lock is released, so a save of the board that waits for it
        // answers after the window has the reload.
        if let Some(window) = project.open_boards().window(&id) {
            host.deliver(
                &[window],
                &WorkspaceEvent::BoardReloaded {
                    project: workspace.project().clone(),
                    board: id,
                    state,
                },
            );
        }
    }
}

/// How a board file compares with `revision` once its writer is done: a changed file is read
/// again until it is a valid board, or its bytes stay the same for [`SETTLE_QUIET`], or
/// [`SETTLE_LIMIT`] passes. `None` when the file can't be examined.
fn settled_change(files: &Files<'_>, file: &BoardFile, revision: Revision) -> Option<Change> {
    let started = Instant::now();
    let mut quiet_since = started;
    let mut last: Option<Change> = None;
    loop {
        let change = files.dirs.change(file, revision).ok()?;
        if !matches!(change, Change::Changed(_)) {
            return Some(change);
        }
        // The bytes parsed are the bytes compared only if the file didn't change in between.
        if files
            .dirs
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
