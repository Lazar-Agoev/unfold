//! What a command handler gets: the calling window, its workspace, the request's cancellation
//! token and stream, and whether the main process sent it.

use std::sync::Arc;

use serde::Serialize;
use unfold_engine::CancelToken;

use crate::commands::FolderExtras;
use crate::events::WindowId;
use crate::host::HostState;
use crate::ipc::error::{CommandError, CommandResult};
use crate::registry::Registry;
use crate::workspace::Workspace;

/// One running command. Cheap to pass by reference to every handler.
pub struct Ctx<'a> {
    pub(crate) host: &'a Arc<HostState>,
    pub(crate) registry: &'a Registry,
    /// The window the command runs for.
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
    /// The registry: app data and every open folder.
    pub fn registry(&self) -> &Registry {
        self.registry
    }

    /// The workspace of the calling window's folder.
    pub fn workspace(&self) -> CommandResult<Arc<Workspace>> {
        Ok(self.registry.workspace(self.window)?)
    }

    /// The command-side state of a folder (board sessions, the file list).
    pub fn extras(&self, workspace: &Workspace) -> Arc<FolderExtras> {
        self.host.extras(workspace.folder())
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

    /// Sends an event to some windows, as the workspace does.
    pub fn deliver(&self, windows: &[WindowId], event: &crate::events::WorkspaceEvent) {
        self.host.deliver(windows, event);
    }

    /// Sends an app-wide event to every window, including windows that show no folder.
    pub fn broadcast(&self, event: &crate::events::WorkspaceEvent) {
        self.host.broadcast(event);
    }
}
