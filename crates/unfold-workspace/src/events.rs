//! Events to windows (docs/ipc.md#how-a-command-travels, docs/ipc.md#events) and the sink they go through.
//!
//! The window shows the whole workspace, so every event is for it: a project's workspace hands
//! each event to the [`EventSink`], and the host sends it as `{"kind":"broadcast","event":{…}}`,
//! which the engine host's router posts to every window port it has (a closing window's and a
//! reopened one's). The one exception is `board_reloaded`, which only the window that opened the
//! board receives (`{"kind":"event","windows":[w],"event":{…}}`). Project events name their project
//! ([`WorkspaceEvent::project`]). Events serialize with a snake_case `type` tag and camelCase
//! fields, the shape the UI's `window.unfold.on(type, …)` receives.
//!
//! | Event | When |
//! | --- | --- |
//! | `workspace_changed` | A project row was added, removed, expanded or collapsed, or renamed |
//! | `project_state` | A project's row state changed (live ↔ resting included; not on every progress tick) |
//! | `project_reset` | A project was removed: everything cached for it is stale |
//! | `compile_progress` | A compile or update advanced (throttled) |
//! | `graph_updated` | A commit changed the graph's files or inputs. Changed apps alone send only `entrances_changed` |
//! | `entrances_changed` | The apps and entrances changed: their tables, the states of their code, or their fixes. An edit of only the `[[app]]` tables sends this alone: it resolves the entrances again without compiling |
//! | `problems_changed` | The Problems list changed |
//! | `files_changed` | The watcher saw files appear, change, or disappear |
//! | `boards_changed` | Board files or folders changed |
//! | `board_reloaded` | An open board's file changed on disk outside the app: reloaded, deleted, or no longer a valid board (only the window that opened it) |
//! | `config_changed` | `unfold.toml` changed |

use serde::Serialize;

use crate::commands::boards::BoardState;
use crate::projects::{ProjectId, WorkspaceView};
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

/// One event. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum WorkspaceEvent {
    /// The workspace's rows changed. The UI replaces its `workspace` query data with this view
    /// (no refetch).
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
    /// The project was removed: everything cached for it is stale.
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
        /// Whether the configuration inputs or the evidence files changed (the settings file's
        /// record doesn't count).
        inputs_changed: bool,
        /// The lists were too long to send (more than [`MAX_LISTED`]) and are empty: treat
        /// everything as changed.
        everything: bool,
        /// Whether every discovered file is indexed.
        complete: bool,
    },
    /// The apps and entrances changed: their tables in `unfold.toml`, the states of their code,
    /// or their fixes. The UI refreshes `entrances` (and `project_summary`'s Health line) on it.
    EntrancesChanged {
        /// The project.
        project: ProjectId,
    },
    /// The Problems list changed.
    ProblemsChanged {
        /// The project.
        project: ProjectId,
        /// How many problems the list has now, board health included.
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
    /// Board files or folders changed: outside the app (the watcher), or by a board command.
    BoardsChanged {
        /// The project.
        project: ProjectId,
        /// The boards whose files changed, sorted, as `list_boards` lists them (a file whose name
        /// a board file can't have included).
        boards: Vec<String>,
        /// Whether the project's folders may have changed too (boards moved, folders created,
        /// renamed, or deleted).
        folders: bool,
    },
    /// An open board's file changed on disk outside this app (an agent or another program
    /// wrote it, or a `git pull` did). Sent only to the window that opened the board, which
    /// takes a valid board's version when nothing of its own is unsaved, and otherwise stops
    /// saving and asks the person which version to keep (docs/ui-shell.md#state-and-engine-data).
    BoardReloaded {
        /// The project.
        project: ProjectId,
        /// The board's id.
        board: String,
        /// What the file holds now.
        file: ReloadedFile,
    },
    /// `unfold.toml` changed.
    ConfigChanged {
        /// The project.
        project: ProjectId,
    },
}

/// What an open board's file holds after an outside change (`board_reloaded`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReloadedFile {
    /// A valid board, which the board's session took: the board as the window now shows it.
    Board {
        /// The board as reloaded.
        state: Box<BoardState>,
    },
    /// Nothing: the file was deleted. The session keeps the version last loaded or saved,
    /// which the next save writes back.
    Deleted,
    /// Something that isn't a valid board (a merge conflict, a broken edit). The session keeps
    /// its version, and saves are refused (`board_changed`) until the person keeps theirs or
    /// the file is a valid board again.
    Invalid {
        /// Why it isn't a valid board.
        message: String,
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
            Self::EntrancesChanged { .. } => "entrances_changed",
            Self::ProblemsChanged { .. } => "problems_changed",
            Self::FilesChanged { .. } => "files_changed",
            Self::BoardsChanged { .. } => "boards_changed",
            Self::BoardReloaded { .. } => "board_reloaded",
            Self::ConfigChanged { .. } => "config_changed",
        }
    }

    /// The project the event concerns, or `None` for a workspace-wide or app-wide event.
    pub fn project(&self) -> Option<&ProjectId> {
        match self {
            Self::ProjectState { project, .. }
            | Self::ProjectReset { project }
            | Self::CompileProgress { project, .. }
            | Self::GraphUpdated { project, .. }
            | Self::EntrancesChanged { project }
            | Self::ProblemsChanged { project, .. }
            | Self::FilesChanged { project, .. }
            | Self::BoardsChanged { project, .. }
            | Self::BoardReloaded { project, .. }
            | Self::ConfigChanged { project } => Some(project),
            Self::WorkspaceChanged { .. } => None,
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
            changed_keys: vec!["a.ts#f".into()],
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
        let invalid = serde_json::to_value(WorkspaceEvent::BoardReloaded {
            project: project.clone(),
            board: ".unfold/boards/a.json".into(),
            file: ReloadedFile::Invalid {
                message: "not valid JSON".into(),
            },
        })
        .expect("serialize");
        assert_eq!(
            invalid,
            serde_json::json!({
                "type": "board_reloaded", "project": "p-abcdefghij", "board": ".unfold/boards/a.json",
                "file": { "kind": "invalid", "message": "not valid JSON" }
            })
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
    }
}
