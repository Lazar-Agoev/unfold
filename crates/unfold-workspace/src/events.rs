//! Events to windows (architecture §12) and the sink they go through.
//!
//! Every folder event names its folder. The workspace hands each one to the [`EventSink`] with
//! the windows attached to that folder; the addon implements the sink and wraps each delivery as
//! `{"kind":"event","windows":[…],"event":{…}}` (brief §6). App-wide events
//! (`preferences_changed`) name no folder: the host sends them to every window, including
//! windows that show no folder, as `{"kind":"broadcast","event":{…}}`. Events serialize with a snake_case
//! `type` tag and camelCase fields, the shape the UI's `window.unfold.on(type, …)` receives.
//!
//! | Event | When |
//! | --- | --- |
//! | `folder_opened` | A window attached to a folder |
//! | `graph_status` | The status-bar state changed kind (not on every progress tick) |
//! | `compile_progress` | A compile or update advanced (throttled) |
//! | `graph_updated` | A commit changed the graph |
//! | `problems_changed` | The Problems list changed |
//! | `files_changed` | The watcher saw files appear, change, or disappear |
//! | `boards_changed` | Published boards or drafts changed on disk (outside the app, or by a board command) |
//! | `board_conflict` | A board open in a window changed on disk outside this app (only that window) |
//! | `config_changed` | `.unfold/config.toml` changed |
//! | `preferences_changed` | `set_preferences` saved the app preferences (every window) |

use serde::Serialize;
use ts_rs::TS;

use crate::state::Preferences;
use crate::status::GraphStatus;

/// A window's ID, chosen by the desktop shell.
pub type WindowId = u32;

/// Receives every event with the windows it is for. Called from workspace threads (the engine
/// thread, the watcher, command threads), so it must be cheap and must not block on the
/// workspace.
pub trait EventSink: Send + Sync {
    /// Delivers `event` to `windows` (never empty).
    fn deliver(&self, windows: &[WindowId], event: &WorkspaceEvent);
}

/// The most paths or keys one `graph_updated` or `files_changed` event lists. Past it the event
/// says everything changed (`everything` / `truncated`), and windows refresh what they show.
pub const MAX_LISTED: usize = 2_000;

/// One event. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum WorkspaceEvent {
    /// A window attached to the folder.
    FolderOpened {
        /// The folder's canonical path.
        folder: String,
        /// The folder's name.
        name: String,
        /// Whether the folder has a graph (or is being compiled).
        compiled: bool,
    },
    /// The graph status changed.
    GraphStatus {
        /// The folder.
        folder: String,
        /// The new status.
        status: GraphStatus,
    },
    /// A compile or update advanced.
    CompileProgress {
        /// The folder.
        folder: String,
        /// The pipeline stage (`snapshot`, `extract`, ...).
        stage: &'static str,
        /// Items finished.
        done: u64,
        /// Items in the stage.
        total: u64,
    },
    /// A commit changed the graph.
    GraphUpdated {
        /// The folder.
        folder: String,
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
        /// The folder.
        folder: String,
        /// How many problems there are now.
        count: u32,
    },
    /// Files appeared, changed, or disappeared on disk.
    FilesChanged {
        /// The folder.
        folder: String,
        /// Folder-relative paths, sorted.
        paths: Vec<String>,
        /// More paths changed than are listed.
        truncated: bool,
    },
    /// Boards changed on disk: outside the app (the watcher), or by a board command.
    BoardsChanged {
        /// The folder.
        folder: String,
        /// Published boards (folder-relative paths under `.unfold/boards/`).
        published: Vec<String>,
        /// Draft files (names under `local/drafts/`).
        drafts: Vec<String>,
    },
    /// A board open in a window changed on disk outside this app (an agent or a teammate wrote
    /// it, or it was deleted). Sent only to the window holding the board. A window without
    /// unsaved edits reloads it (`reload_board`); one with unsaved edits asks which version to
    /// keep (saving with `baseRevision` set to `diskRevision` keeps its own).
    BoardConflict {
        /// The folder.
        folder: String,
        /// The board's ID.
        board: String,
        /// The revision the app last loaded or saved.
        saved_revision: String,
        /// The revision on disk now, or `None` when the file is gone.
        disk_revision: Option<String>,
    },
    /// `.unfold/config.toml` changed.
    ConfigChanged {
        /// The folder.
        folder: String,
    },
    /// The app preferences were saved (by any window). Sent to every window, so each one applies
    /// the new theme and key bindings without asking for them again.
    PreferencesChanged {
        /// The preferences now saved.
        preferences: Preferences,
    },
}

impl WorkspaceEvent {
    /// The event's `type` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::FolderOpened { .. } => "folder_opened",
            Self::GraphStatus { .. } => "graph_status",
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

    /// The folder the event concerns, or `None` for an app-wide event.
    pub fn folder(&self) -> Option<&str> {
        match self {
            Self::FolderOpened { folder, .. }
            | Self::GraphStatus { folder, .. }
            | Self::CompileProgress { folder, .. }
            | Self::GraphUpdated { folder, .. }
            | Self::ProblemsChanged { folder, .. }
            | Self::FilesChanged { folder, .. }
            | Self::BoardsChanged { folder, .. }
            | Self::BoardConflict { folder, .. }
            | Self::ConfigChanged { folder } => Some(folder),
            Self::PreferencesChanged { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_with_a_type_tag_and_camel_case_fields() {
        let event = WorkspaceEvent::GraphUpdated {
            folder: "/repo".into(),
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
        assert_eq!(json["changedFiles"][0], "a.ts");
        assert_eq!(json["inputsChanged"], false);
        assert_eq!(event.kind(), "graph_updated");
        assert_eq!(event.folder(), Some("/repo"));
        let config = serde_json::to_value(WorkspaceEvent::ConfigChanged {
            folder: "/repo".into(),
        })
        .expect("serialize");
        assert_eq!(
            config,
            serde_json::json!({"type": "config_changed", "folder": "/repo"})
        );
        let preferences = WorkspaceEvent::PreferencesChanged {
            preferences: Preferences::default(),
        };
        assert_eq!(preferences.folder(), None);
        let json = serde_json::to_value(&preferences).expect("serialize");
        assert_eq!(json["type"], "preferences_changed");
        assert_eq!(json["preferences"]["theme"], "system");
        assert_eq!(json["preferences"]["keyBindings"], serde_json::json!({}));
    }
}
