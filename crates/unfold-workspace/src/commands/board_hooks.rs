//! What the rest of the workspace asks of the board commands: the one narrow interface between
//! the core (projects, liveness, events, Home) and the board sessions and health of
//! `commands::boards`.
//!
//! | Hook | Called by | When |
//! | --- | --- | --- |
//! | [`BoardExtras`] | [`super::ProjectExtras::boards`] | A live project's open board sessions, its boards' health cache, and the lock that orders disk checks; dropped when the project rests |
//! | [`released`] | `Host::detach_window` | A closing window's boards of a project were closed: forget their sessions |
//! | [`refresh_health`] | The coalesced health refresh (one thread per project) | After the project woke, and after `graph_updated` or `entrances_changed` of the project |
//! | [`boards_changed`] | The host, on `boards_changed` (its own thread) | Board files changed on disk: reload open sessions whose file changed (`board_reloaded`), refresh those boards' health |
//! | [`health_rows`] | `project_summary` | Home's Health line: the boards that need review |
//!
//! The core keeps the open boards ([`crate::open_boards::OpenBoards`]). Board health items reach the Problems list (not the row's problem count) through
//! [`crate::Workspace::set_board_problems`], which [`refresh_health`] calls.

use std::sync::Arc;

use super::ProjectExtras;
pub use super::boards::BoardExtras;
use super::boards::{check_disk, refresh};
use super::projects::BoardHealthCount;
use crate::host::HostState;
use crate::projects::ProjectHandle;
use crate::workspace::Workspace;

/// A closing window's boards `boards` of `project` were closed: their sessions go.
/// (Health doesn't change: it is kept for every board, open or not.)
pub(crate) fn released(_host: &Arc<HostState>, project: &ProjectHandle<'_>, boards: &[String]) {
    project.extras().boards.forget_sessions(boards);
}

/// Recomputes the health of every board of a live project that is no longer current (on the
/// project's health-refresh thread), then publishes the board problems with
/// `workspace.set_board_problems`.
pub(crate) fn refresh_health(workspace: &Workspace, extras: &ProjectExtras) {
    refresh(workspace, &extras.boards, None);
}

/// Boards `boards` of a live project changed on disk: reload each open board whose file changed
/// and tell the window that opened it (`board_reloaded`), and refresh those boards' health.
pub(crate) fn boards_changed(
    host: &Arc<HostState>,
    workspace: &Workspace,
    extras: &ProjectExtras,
    boards: &[String],
) {
    let Ok(projects) = host.projects() else {
        return;
    };
    check_disk(host, projects, workspace, &extras.boards);
    refresh(workspace, &extras.boards, Some(boards));
}

/// The boards of a live project whose health has items, most items first.
pub(crate) fn health_rows(extras: &ProjectExtras) -> Vec<BoardHealthCount> {
    extras.boards.health_rows()
}
