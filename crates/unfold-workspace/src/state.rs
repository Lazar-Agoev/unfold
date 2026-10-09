//! Remembered state, in JSON files Rust owns (architecture §8.1–8.2, §11.2, decision U-23). The
//! UI never uses browser storage; everything it remembers lives here.
//!
//! - **Per folder:** `.unfold/local/state.json` holds each window's position, size, tabs,
//!   layout, and open boards, and whether background updates are on. Before the first Compile
//!   nothing may be written into the folder, so changes are kept in memory and written when
//!   `local/` is created.
//! - **Per app:** `preferences.json` (theme, editor command, background-update default, thread
//!   limit, key bindings) and `recent.json` (recent folders and the welcome window's bounds)
//!   in the app-data folder the desktop shell passes at init.
//!
//! Every file is written atomically (a temporary file renamed over it). A file that is missing,
//! unreadable, or not valid JSON reads as the defaults: remembered state is a convenience, never
//! a reason to refuse to open.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::board::BoardError;
use unfold_reader::board::files::write_atomic;

use crate::error::{Result, WorkspaceError};

/// A window's position and size, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
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

/// One window of a folder, as it should come back.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    /// A stable key the shell assigns, so a window's state survives restarts (window IDs
    /// don't).
    pub key: String,
    /// Position and size.
    pub bounds: Option<WindowBounds>,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// The open tabs, in order, as the UI describes them.
    pub tabs: Vec<serde_json::Value>,
    /// The active tab's position in `tabs`.
    pub active_tab: Option<u32>,
    /// Sidebar, inspector, panel, and lens layout, as the UI describes it.
    pub layout: serde_json::Value,
    /// The boards open in the window.
    pub open_boards: Vec<String>,
}

/// `.unfold/local/state.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct FolderState {
    /// The format version, `1`.
    pub version: u32,
    /// Whether background updates are on for this folder.
    pub background_updates: bool,
    /// The folder's windows.
    pub windows: Vec<WindowState>,
}

impl Default for FolderState {
    fn default() -> Self {
        Self {
            version: 1,
            background_updates: true,
            windows: Vec::new(),
        }
    }
}

impl FolderState {
    /// Replaces the window with `window.key`, or appends it.
    pub fn put_window(&mut self, window: WindowState) {
        match self
            .windows
            .iter_mut()
            .find(|existing| existing.key == window.key)
        {
            Some(existing) => *existing = window,
            None => self.windows.push(window),
        }
    }

    /// Removes the window with `key`; whether it was there.
    pub fn remove_window(&mut self, key: &str) -> bool {
        let before = self.windows.len();
        self.windows.retain(|window| window.key != key);
        self.windows.len() != before
    }
}

/// The light or dark theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// `preferences.json` in the app-data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// The theme.
    pub theme: Theme,
    /// The command that opens a file in the person's editor, such as `code -g {path}:{line}`.
    pub editor_command: Option<String>,
    /// Whether background updates start on for newly compiled folders.
    pub background_updates: bool,
    /// The most threads a compile uses; `None` means every core but one.
    pub thread_limit: Option<u32>,
    /// Key binding overrides, by command.
    pub key_bindings: BTreeMap<String, String>,
    /// The update check.
    pub updates: UpdatePreferences,
}

/// The update check (architecture §14): the app's only network use, which can be turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdatePreferences {
    /// Whether the app checks GitHub Releases for a new version of itself.
    pub enabled: bool,
}

impl Default for UpdatePreferences {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            editor_command: None,
            background_updates: true,
            thread_limit: None,
            key_bindings: BTreeMap::new(),
            updates: UpdatePreferences::default(),
        }
    }
}

/// One recent folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecentFolder {
    /// The canonical path.
    pub path: String,
    /// The folder's name.
    pub name: String,
    /// When it was last opened, in milliseconds since the Unix epoch.
    pub opened_at_ms: i64,
}

/// `recent.json` in the app-data folder.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct RecentState {
    /// Recent folders, most recent first.
    pub folders: Vec<RecentFolder>,
    /// The welcome window's bounds.
    pub welcome_bounds: Option<WindowBounds>,
}

/// How many recent folders are kept.
pub const MAX_RECENT: usize = 20;

/// App-wide preferences and recent folders. Cheap to share; every method takes `&self`.
#[derive(Debug)]
pub struct AppData {
    dir: PathBuf,
    write: Mutex<()>,
}

impl AppData {
    /// The app data in `dir` (created when first written).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            write: Mutex::new(()),
        }
    }

    /// The app-data folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The preferences (defaults if none are saved).
    pub fn preferences(&self) -> Preferences {
        read_json(&self.dir.join("preferences.json"))
    }

    /// Saves the preferences.
    pub fn set_preferences(&self, preferences: &Preferences) -> Result<()> {
        let _guard = self.lock();
        write_json(
            &self.dir.join("preferences.json"),
            preferences,
            "preferences.json",
        )
    }

    /// Recent folders and the welcome window's bounds.
    pub fn recent(&self) -> RecentState {
        read_json(&self.dir.join("recent.json"))
    }

    /// Moves `path` to the top of the recent folders.
    pub fn add_recent(&self, path: &str, name: &str, opened_at_ms: i64) -> Result<RecentState> {
        self.update_recent(|recent| {
            recent.folders.retain(|folder| folder.path != path);
            recent.folders.insert(
                0,
                RecentFolder {
                    path: path.to_owned(),
                    name: name.to_owned(),
                    opened_at_ms,
                },
            );
            recent.folders.truncate(MAX_RECENT);
        })
    }

    /// Removes `path` from the recent folders.
    pub fn remove_recent(&self, path: &str) -> Result<RecentState> {
        self.update_recent(|recent| recent.folders.retain(|folder| folder.path != path))
    }

    /// Remembers the welcome window's bounds.
    pub fn set_welcome_bounds(&self, bounds: Option<WindowBounds>) -> Result<RecentState> {
        self.update_recent(|recent| recent.welcome_bounds = bounds)
    }

    fn update_recent(&self, change: impl FnOnce(&mut RecentState)) -> Result<RecentState> {
        let _guard = self.lock();
        let path = self.dir.join("recent.json");
        let mut recent: RecentState = read_json(&path);
        change(&mut recent);
        write_json(&path, &recent, "recent.json")?;
        Ok(recent)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.write.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads JSON, or the defaults when the file is missing or unusable.
pub(crate) fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes JSON atomically, creating the parent folder.
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
/// need the last write to win serialize their writes (see `AppData` and
/// `Workspace::update_folder_state`).
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
    fn folder_state_round_trips_and_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("local/state.json");
        assert_eq!(read_json::<FolderState>(&path), FolderState::default());
        let mut state = FolderState {
            background_updates: false,
            ..FolderState::default()
        };
        state.put_window(WindowState {
            key: "main".into(),
            bounds: Some(WindowBounds {
                x: 10,
                y: 20,
                width: 800,
                height: 600,
            }),
            tabs: vec![serde_json::json!({"kind": "board", "id": "b1"})],
            open_boards: vec!["b1".into()],
            ..WindowState::default()
        });
        write_json(&path, &state, "state.json").expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"backgroundUpdates\": false"));
        assert!(text.contains("\"openBoards\""));
        assert_eq!(read_json::<FolderState>(&path), state);
        std::fs::write(&path, "{ not json").expect("write");
        assert_eq!(read_json::<FolderState>(&path), FolderState::default());
        assert!(state.remove_window("main"));
        assert!(!state.remove_window("main"));
    }

    #[test]
    fn the_update_check_is_on_unless_turned_off() {
        assert!(Preferences::default().updates.enabled);
        // A preferences file from before the setting existed keeps the default.
        let older: Preferences =
            serde_json::from_str(r#"{"theme":"dark"}"#).expect("parse older preferences");
        assert!(older.updates.enabled);
        let off: Preferences =
            serde_json::from_str(r#"{"updates":{"enabled":false}}"#).expect("parse preferences");
        assert!(!off.updates.enabled);
    }

    #[test]
    fn preferences_and_recent_folders() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = AppData::new(dir.path().join("Unfold"));
        assert_eq!(app.preferences(), Preferences::default());
        let preferences = Preferences {
            theme: Theme::Dark,
            thread_limit: Some(4),
            ..Preferences::default()
        };
        app.set_preferences(&preferences).expect("save");
        assert_eq!(app.preferences(), preferences);
        for index in 0..25 {
            app.add_recent(&format!("/repo{index}"), "repo", index)
                .expect("add");
        }
        let recent = app.add_recent("/repo3", "repo", 99).expect("add");
        assert_eq!(recent.folders.len(), MAX_RECENT);
        assert_eq!(recent.folders[0].path, "/repo3");
        assert_eq!(
            recent
                .folders
                .iter()
                .filter(|folder| folder.path == "/repo3")
                .count(),
            1
        );
        let recent = app.remove_recent("/repo3").expect("remove");
        assert!(recent.folders.iter().all(|folder| folder.path != "/repo3"));
        assert_eq!(app.recent(), recent);
    }
}
