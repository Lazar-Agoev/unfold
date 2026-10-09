//! What a command handler gets: the workspace's projects, the calling window, the request's
//! cancellation token and stream, and whether the main process sent it.

use std::sync::Arc;

use serde::Serialize;
use unfold_engine::CancelToken;

use crate::events::{WindowId, WorkspaceEvent};
use crate::host::HostState;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{ProjectHandle, ProjectId, Projects};
use crate::workspace::Workspace;

/// One running command. Cheap to pass by reference to every handler.
pub struct Ctx<'a> {
    pub(crate) host: &'a Arc<HostState>,
    pub(crate) projects: &'a Projects,
    /// The window the command runs for (`0` for the main process's own requests).
    pub window: WindowId,
    /// The request's ID, which `cancel` names.
    pub request: u64,
    /// Cancelled by `cancel(request)`.
    pub cancel: CancelToken,
    /// Whether the engine host marked the request as coming from the main process, which may
    /// name absolute paths the person chose in a dialog.
    pub trusted: bool,
}

impl Ctx<'_> {
    /// The workspace's projects, app data, and recent boards.
    pub fn projects(&self) -> &Projects {
        self.projects
    }

    /// Project `id`, without waking it (`unknown_project` when the workspace has no such
    /// project).
    pub fn project(&self, id: &ProjectId) -> CommandResult<ProjectHandle<'_>> {
        Ok(self.projects.handle(id)?)
    }

    /// Project `id`'s live workspace: wakes the project if it rests, and extends its lease.
    pub fn live(&self, id: &ProjectId) -> CommandResult<Arc<Workspace>> {
        Ok(self.project(id)?.wake()?)
    }

    /// Sends one stream message for this request.
    pub fn stream<T: Serialize>(&self, message: &T) {
        self.host.stream(self.window, self.request, message);
    }

    /// `Err(cancelled)` once the request was cancelled.
    pub fn check_cancelled(&self) -> CommandResult<()> {
        if self.cancel.is_cancelled() {
            Err(CommandError::cancelled())
        } else {
            Ok(())
        }
    }

    /// `Err(untrusted)` unless the main process sent the request.
    pub fn require_trusted(&self, what: &str) -> CommandResult<()> {
        if self.trusted {
            Ok(())
        } else {
            Err(CommandError::new(
                "untrusted",
                format!("{what} is accepted only from the main process"),
            ))
        }
    }

    /// Sends an event to some windows only (`board_conflict`, to the window holding the board).
    pub fn deliver(&self, windows: &[WindowId], event: &WorkspaceEvent) {
        self.host.deliver(windows, event);
    }

    /// Sends an event to every window.
    pub fn broadcast(&self, event: &WorkspaceEvent) {
        self.host.broadcast(event);
    }
}
