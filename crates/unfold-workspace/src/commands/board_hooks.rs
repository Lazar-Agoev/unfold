//! What the rest of the workspace asks of the board commands: the one narrow interface between
//! the core (projects, liveness, events, Home) and the board sessions and health of
//! `commands::boards` (contract §3.5, §4.3, §4.6, §6).
//!
//! | Hook | Called by | When |
//! | --- | --- | --- |
//! | [`BoardExtras`] | [`super::ProjectExtras::boards`] | A live project's open board sessions, its boards' health cache, and the lock that orders disk checks; dropped when the project rests or switches worktree |
//! | [`released`] | `Host::detach_window` | A closing window's claims on a project's boards were released: forget their sessions |
//! | [`refresh_health`] | The coalesced health refresh (one thread per project) | After the project woke, after `graph_updated` of the project, and after `graph_updated` of a project its boards reference ([`dependents`]) |
//! | [`dependents`] | The host, on `graph_updated` of a project | Which other live projects' boards have cards from it |
//! | [`boards_changed`] | The host, on `boards_changed` (its own thread) | Board files changed on disk: check open sessions for `board_conflict`, refresh those boards' health |
//! | [`health_rows`] | `project_summary` | Home's Health line: the boards that need review |
//!
//! The core keeps the claims (with each open board's cross-project references, which keep those
//! projects live: [`crate::claims::BoardClaims::set_references`]), the recent boards
//! ([`crate::projects::Projects::record_recent_board`] and its siblings), and the view state
//! ([`crate::state::LocalState::put_board_view`] and its siblings); the board commands keep them
//! current. Board health items reach the Problems list (and the row's problem count) through
//! [`crate::Workspace::set_board_problems`], which [`refresh_health`] calls.

use std::sync::Arc;

use super::ProjectExtras;
pub use super::boards::BoardExtras;
use super::boards::{check_disk, refresh};
use super::projects::BoardHealthCount;
use crate::host::HostState;
use crate::projects::{ProjectHandle, ProjectId};
use crate::workspace::Workspace;

/// A closing window's claims on boards `boards` of `project` were released: their sessions go.
/// (Health doesn't change: it is kept for every board, open or not.)
pub(crate) fn released(_host: &Arc<HostState>, project: &ProjectHandle<'_>, boards: &[String]) {
    project.extras().boards.forget_sessions(boards);
}

/// Recomputes the health of every board of a live project that is no longer current (on the
/// project's health-refresh thread), then publishes the board problems with
/// `workspace.set_board_problems`.
pub(crate) fn refresh_health(host: &Arc<HostState>, workspace: &Workspace, extras: &ProjectExtras) {
    if let Ok(projects) = host.projects() {
        refresh(projects, workspace, &extras.boards, None);
    }
}

/// The live projects other than `project` whose boards have cards from `project`: their health
/// follows its graph.
pub(crate) fn dependents(host: &HostState, project: &ProjectId) -> Vec<ProjectId> {
    let Ok(projects) = host.projects() else {
        return Vec::new();
    };
    projects
        .ids()
        .into_iter()
        .filter(|id| id != project)
        .filter(|id| {
            projects
                .handle(id)
                .is_ok_and(|handle| handle.live().is_some() && handle.extras().boards.references(project))
        })
        .collect()
}

/// Boards `boards` of a live project changed on disk: tell each window holding one whose
/// session's working file changed (`board_conflict`), and refresh those boards' health.
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
    refresh(projects, workspace, &extras.boards, Some(boards));
}

/// The boards of a live project whose health has items, most items first.
pub(crate) fn health_rows(extras: &ProjectExtras) -> Vec<BoardHealthCount> {
    extras.boards.health_rows()
}
