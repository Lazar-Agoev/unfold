//! Remembered state, in JSON files Rust owns (docs/architecture.md#storage). The UI never uses
//! browser storage; everything it remembers lives here.
//!
//! - **Per app** (the app-data folder the desktop shell passes at init): `settings.json`
//!   ([`AppSettings`]: the editor command), `window.json` (the window that comes back at launch:
//!   [`WindowState`]), and `workspace.json` (the projects, owned by
//!   [`crate::projects::Projects`]).
//! - **Per project:** `.unfold/local/state.json` ([`LocalState`]): the empty board folders.
//!
//! Every file is written atomically (a synced temporary file renamed over it), pretty-printed
//! with a final newline, and writes to one file are serialized. A file that is missing,
//! unreadable, or not valid JSON reads as the defaults: remembered state is a convenience, never
//! a reason to refuse to open.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::board::files::write_atomic;
use unfold_reader::board::{BoardError, is_folder_name};

use crate::error::{Result, WorkspaceError};
use crate::projects::ProjectId;

/// A window's position and size, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowBounds {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// The board the window shows, by project id and board id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShownBoard {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub board: String,
}

/// `window.json` in the app-data folder: the window as it should come back at launch. The main
/// process saves it; the window's renderer reports the board it shows and the Show all calls
/// switch to the main process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowState {
    /// Position and size.
    pub bounds: WindowBounds,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// The last board shown, or `None` for Home.
    pub board: Option<ShownBoard>,
    /// Whether Show all calls is on.
    pub show_all_calls: bool,
}

/// `.unfold/local/state.json`: what one project remembers on this machine.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LocalState {
    /// Board folders with no board in them, sorted.
    pub empty_folders: Vec<String>,
}

impl LocalState {
    /// The state in the file at `path` ([`read_json`]), without folder names a folder can't
    /// have (folders are one level deep).
    pub(crate) fn read(path: &Path) -> Self {
        let mut state: Self = read_json(path);
        state.empty_folders.retain(|folder| is_folder_name(folder));
        state
    }
}

/// `settings.json` in the app-data folder.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    /// The command that opens a file in the person's editor, such as `code -g {path}:{line}`;
    /// without one, files open in TextEdit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub editor_command: Option<String>,
}

/// The app-data folder: the settings and the remembered window. Cheap to share; every method
/// takes `&self`, and writes to one file are serialized.
#[derive(Debug)]
pub struct AppData {
    dir: PathBuf,
    settings: Mutex<()>,
    window: Mutex<()>,
}

impl AppData {
    /// The app data in `dir` (created when first written).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            settings: Mutex::new(()),
            window: Mutex::new(()),
        }
    }

    /// The app-data folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The settings (defaults if none are saved).
    pub fn settings(&self) -> AppSettings {
        read_json(&self.dir.join("settings.json"))
    }

    /// Saves the settings.
    pub fn set_settings(&self, settings: &AppSettings) -> Result<()> {
        let _guard = guard(&self.settings);
        write_json(&self.dir.join("settings.json"), settings, "settings.json")
    }

    /// The remembered window, if one was saved.
    pub fn window(&self) -> Option<WindowState> {
        read_json(&self.dir.join("window.json"))
    }

    /// Saves the remembered window.
    pub fn set_window(&self, window: &WindowState) -> Result<()> {
        let _guard = guard(&self.window);
        write_json(&self.dir.join("window.json"), window, "window.json")
    }
}

fn guard(mutex: &Mutex<()>) -> MutexGuard<'_, ()> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reads JSON, or the defaults when the file is missing or unusable.
pub(crate) fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes JSON atomically, pretty-printed with a final newline, creating the parent folder.
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T, what: &'static str) -> Result<()> {
    let mut text = serde_json::to_vec_pretty(value).map_err(|error| WorkspaceError::Encode {
        what,
        message: error.to_string(),
    })?;
    text.push(b'\n');
    write_atomically(path, &text)
}

/// Writes `bytes` to a synced temporary file beside `path` and renames it over `path` (creating
/// the parent folder), so readers see the old file or the new one, never a partial write. This
/// is the same helper board files use; every call gets its own temporary file, and callers that
/// need the last write to win serialize their writes (see `AppData`, `Projects`, and
/// `Workspace::update_local_state`).
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| WorkspaceError::io(parent, error))?;
    write_atomic(path, bytes).map_err(|error| {
        let source = match error {
            BoardError::Io { message, .. } => std::io::Error::other(message),
            other => std::io::Error::other(other.to_string()),
        };
        WorkspaceError::io(path, source)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_state_round_trips_and_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("local/state.json");
        assert_eq!(LocalState::read(&path), LocalState::default());
        let state = LocalState {
            empty_folders: vec!["Checkout".into(), "Refunds".into()],
        };
        write_json(&path, &state, "state.json").expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            text,
            "{\n  \"emptyFolders\": [\n    \"Checkout\",\n    \"Refunds\"\n  ]\n}\n"
        );
        assert_eq!(LocalState::read(&path), state);
        std::fs::write(
            &path,
            r#"{"emptyFolders": ["Checkout/Refunds", "..", "Refunds"]}"#,
        )
        .expect("write");
        assert_eq!(
            LocalState::read(&path).empty_folders,
            ["Refunds"],
            "nested and invalid names go"
        );
        for invalid in ["{ not json", r#"{"version": 1, "emptyFolders": ["Refunds"]}"#] {
            std::fs::write(&path, invalid).expect("write");
            assert_eq!(LocalState::read(&path), LocalState::default(), "{invalid}");
        }
    }

    #[test]
    fn settings_hold_only_the_editor_command() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = AppData::new(dir.path().join("Unfold"));
        assert_eq!(app.settings(), AppSettings::default());
        app.set_settings(&AppSettings::default()).expect("save");
        let text = std::fs::read_to_string(app.dir().join("settings.json")).expect("read");
        assert_eq!(text, "{}\n", "an unset command isn't written");
        let settings = AppSettings {
            editor_command: Some("code -g {path}:{line}".to_owned()),
        };
        app.set_settings(&settings).expect("save");
        assert_eq!(app.settings(), settings);
        std::fs::write(app.dir().join("settings.json"), r#"{"theme":"dark"}"#).expect("write");
        assert_eq!(
            app.settings(),
            AppSettings::default(),
            "a file that doesn't parse reads as defaults"
        );
    }

    #[test]
    fn the_window_is_remembered_once_saved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = AppData::new(dir.path().join("Unfold"));
        assert_eq!(app.window(), None);
        let window = WindowState {
            bounds: WindowBounds {
                x: 40,
                y: -20,
                width: 1280,
                height: 800,
            },
            maximized: false,
            board: Some(ShownBoard {
                project: ProjectId::new_unchecked("p-aaaaaaaaaa"),
                board: ".unfold/boards/orders.json".to_owned(),
            }),
            show_all_calls: true,
        };
        app.set_window(&window).expect("save");
        let text = std::fs::read_to_string(app.dir().join("window.json")).expect("read");
        assert!(text.contains("\"showAllCalls\": true"), "{text}");
        assert_eq!(app.window(), Some(window));
        std::fs::write(app.dir().join("window.json"), "{ nope").expect("write");
        assert_eq!(app.window(), None);
    }
}
