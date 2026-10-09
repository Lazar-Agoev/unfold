//! Events to windows (contract §5, architecture §12) and the sink they go through.
//!
//! Every window shows the whole workspace, so every event is for every window: a project's
//! workspace hands each event to the [`EventSink`], and the host sends it as
//! `{"kind":"broadcast","event":{…}}`, which the engine host's router posts to every window it
//! has. The one exception is `board_conflict`, which only the window holding the board receives
//! (`{"kind":"event","windows":[w],"event":{…}}`). Project events name their project
//! ([`WorkspaceEvent::project`]). Events serialize with a snake_case `type` tag and camelCase
//! fields, the shape the UI's `window.unfold.on(type, …)` receives.
//!
//! | Event | When |
//! | --- | --- |
//! | `workspace_changed` | A project row was added, removed, reordered, expanded or collapsed, renamed, or switched; recent boards changed |
//! | `project_state` | A project's row state changed (live ↔ resting included; not on every progress tick) |
//! | `project_reset` | A project's folder changed (worktree switch) or it was removed: everything cached for it is stale |
//! | `compile_progress` | A compile or update advanced (throttled) |
//! | `graph_updated` | A commit changed the graph |
//! | `problems_changed` | The Problems list changed |
//! | `files_changed` | The watcher saw files appear, change, or disappear |
//! | `boards_changed` | Board files, folders, or a board's publish state changed |
//! | `board_conflict` | The file an open board's session reads changed on disk outside the app (only the holder) |
//! | `config_changed` | `.unfold/config.toml` changed |
//! | `preferences_changed` | `set_preferences` saved the app preferences |

use serde::Serialize;
use ts_rs::TS;

use crate::projects::{ProjectId, WorkspaceView};
use crate::state::Preferences;
use crate::status::ProjectState;

/// A window's ID, chosen by the desktop shell.
pub type WindowId = u32;

/// Receives every event a project's workspace sends. Called from workspace threads (the engine
/// thread, the watcher, command threads), so it must be cheap and must not block on the
/// workspace.
pub trait EventSink: Send + Sync {
    /// Sends `event` to every window.
    fn emit(&self, event: &WorkspaceEvent);
}

/// The most paths or keys one `graph_updated` or `files_changed` event lists. Past it the event
/// says everything changed (`everything` / `truncated`), and windows refresh what they show.
pub const MAX_LISTED: usize = 2_000;

/// Which file an open board's session reads (contract §6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkingFile {
    /// A draft.
    Draft,
    /// A published board's unpublished-changes copy.
    Changes,
    /// A published board without a copy.
    Published,
}

/// One event. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum WorkspaceEvent {
    /// The workspace's rows or recent boards changed. The UI replaces its `workspace` query
    /// data with this view (no refetch).
    WorkspaceChanged {
        /// The workspace now.
        workspace: WorkspaceView,
    },
    /// A project's row state changed.
    ProjectState {
        /// The project.
        project: ProjectId,
        /// Its state now.
        state: ProjectState,
    },
    /// The project's folder changed (a worktree switch) or the project was removed: everything
    /// cached for it is stale.
    ProjectReset {
        /// The project.
        project: ProjectId,
    },
    /// A compile or update advanced.
    CompileProgress {
        /// The project.
        project: ProjectId,
        /// The pipeline stage (`snapshot`, `extract`, ...).
        stage: &'static str,
        /// Items finished.
        done: u64,
        /// Items in the stage.
        total: u64,
    },
    /// A commit changed the graph.
    GraphUpdated {
        /// The project.
        project: ProjectId,
        /// Files added or changed, in path order.
        changed_files: Vec<String>,
        /// Files no longer indexed, in path order.
        removed_files: Vec<String>,
        /// Block keys added, removed, or changed, sorted.
        changed_keys: Vec<String>,
        /// Entrance keys added, removed, or changed, sorted.
        changed_entrances: Vec<String>,
        /// Whether the configuration inputs changed.
        inputs_changed: bool,
        /// The lists were too long to send (more than [`MAX_LISTED`]) and are empty: treat
        /// everything as changed.
        everything: bool,
        /// Whether every discovered file is indexed.
        complete: bool,
    },
    /// The Problems list changed.
    ProblemsChanged {
        /// The project.
        project: ProjectId,
        /// How many problems there are now.
        count: u32,
    },
    /// Files appeared, changed, or disappeared on disk.
    FilesChanged {
        /// The project.
        project: ProjectId,
        /// Folder-relative paths, sorted.
        paths: Vec<String>,
        /// More paths changed than are listed.
        truncated: bool,
    },
    /// Board files, folders, or a board's publish state changed: outside the app (the
    /// watcher), or by a board command. A changes copy's change is reported under its
    /// published board's id.
    BoardsChanged {
        /// The project.
        project: ProjectId,
        /// The boards whose files changed, sorted.
        boards: Vec<String>,
        /// Whether the project's folders may have changed too (boards moved, folders created,
        /// renamed, or deleted).
        folders: bool,
    },
    /// The file an open board's session reads changed on disk outside this app (an agent or a
    /// teammate wrote it, or it was deleted). Sent only to the window holding the board. A
    /// window without unsaved edits reloads it (`reload_board`); one with unsaved edits asks
    /// which version to keep (`save_board` with `force` keeps its own).
    BoardConflict {
        /// The project.
        project: ProjectId,
        /// The board's id.
        board: String,
        /// Which file changed.
        file: WorkingFile,
        /// The revision the app last loaded or saved.
        saved_revision: String,
        /// The revision on disk now, or `None` when the file is gone.
        disk_revision: Option<String>,
    },
    /// `.unfold/config.toml` changed.
    ConfigChanged {
        /// The project.
        project: ProjectId,
    },
    /// The app preferences were saved (by any window), so each window applies the new theme and
    /// key bindings without asking for them again.
    PreferencesChanged {
        /// The preferences now saved.
        preferences: Preferences,
    },
}

impl WorkspaceEvent {
    /// The event's `type` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::WorkspaceChanged { .. } => "workspace_changed",
            Self::ProjectState { .. } => "project_state",
            Self::ProjectReset { .. } => "project_reset",
            Self::CompileProgress { .. } => "compile_progress",
            Self::GraphUpdated { .. } => "graph_updated",
            Self::ProblemsChanged { .. } => "problems_changed",
            Self::FilesChanged { .. } => "files_changed",
            Self::BoardsChanged { .. } => "boards_changed",
            Self::BoardConflict { .. } => "board_conflict",
            Self::ConfigChanged { .. } => "config_changed",
            Self::PreferencesChanged { .. } => "preferences_changed",
        }
    }

    /// The project the event concerns, or `None` for a workspace-wide or app-wide event.
    pub fn project(&self) -> Option<&ProjectId> {
        match self {
            Self::ProjectState { project, .. }
            | Self::ProjectReset { project }
            | Self::CompileProgress { project, .. }
            | Self::GraphUpdated { project, .. }
            | Self::ProblemsChanged { project, .. }
            | Self::FilesChanged { project, .. }
            | Self::BoardsChanged { project, .. }
            | Self::BoardConflict { project, .. }
            | Self::ConfigChanged { project } => Some(project),
            Self::WorkspaceChanged { .. } | Self::PreferencesChanged { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_with_a_type_tag_and_camel_case_fields() {
        let project = ProjectId::new_unchecked("p-abcdefghij");
        let event = WorkspaceEvent::GraphUpdated {
            project: project.clone(),
            changed_files: vec!["a.ts".into()],
            removed_files: Vec::new(),
            changed_keys: vec!["a.ts#function:f".into()],
            changed_entrances: Vec::new(),
            inputs_changed: false,
            everything: false,
            complete: true,
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "graph_updated");
        assert_eq!(json["project"], "p-abcdefghij");
        assert_eq!(json["changedFiles"][0], "a.ts");
        assert_eq!(json["inputsChanged"], false);
        assert_eq!(event.kind(), "graph_updated");
        assert_eq!(event.project(), Some(&project));
        let config = serde_json::to_value(WorkspaceEvent::ConfigChanged {
            project: project.clone(),
        })
        .expect("serialize");
        assert_eq!(
            config,
            serde_json::json!({"type": "config_changed", "project": "p-abcdefghij"})
        );
        let state = serde_json::to_value(WorkspaceEvent::ProjectState {
            project,
            state: ProjectState::Problems { count: 3 },
        })
        .expect("serialize");
        assert_eq!(
            state["state"],
            serde_json::json!({"kind": "problems", "count": 3})
        );
        let preferences = WorkspaceEvent::PreferencesChanged {
            preferences: Preferences::default(),
        };
        assert_eq!(preferences.project(), None);
        let json = serde_json::to_value(&preferences).expect("serialize");
        assert_eq!(json["type"], "preferences_changed");
        assert_eq!(json["preferences"]["theme"], "system");
        assert_eq!(json["preferences"]["keyBindings"], serde_json::json!({}));
    }
}
