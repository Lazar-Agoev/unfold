//! Remembered state, in JSON files Rust owns (architecture §8.1–8.2, decisions
//! U-23, U-46). The UI never uses browser storage; everything it remembers lives here.
//!
//! - **Per app** (the app-data folder the desktop shell passes at init): `preferences.json`
//!   (theme, editor command, background-update default, thread limit, key bindings),
//!   `windows.json` (the windows that come back at launch: [`WindowsFile`]), and
//!   `workspace.json` (the projects and recent boards, owned by [`crate::projects::Projects`]).
//! - **Per project:** `.unfold/local/state.json` ([`LocalState`]): whether background updates
//!   are on, the empty board folders, and each board's view state. Before the first Compile
//!   nothing may be written into the folder, so changes are kept in memory and written when
//!   `local/` is created.
//!
//! Every file is written atomically (a synced temporary file renamed over it), pretty-printed
//! with a final newline, and writes to one file are serialized. A file that is missing,
//! unreadable, or not valid JSON reads as the defaults: remembered state is a convenience, never
//! a reason to refuse to open.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;
use unfold_reader::board::files::write_atomic;
use unfold_reader::board::{BoardError, View};

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

/// One board tab of a window. Home is implicit and always first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowTab {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub board: String,
    /// The page the tab shows, or `None` for the board's first page.
    pub page: Option<String>,
}

/// One window as it should come back at launch (a `windows.json` entry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SavedWindow {
    /// The stable key the main process assigns (window ids change every run).
    pub key: String,
    /// Position and size, once the main process saved them.
    pub bounds: Option<WindowBounds>,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// The project the window's Home shows.
    pub selected_project: Option<ProjectId>,
    /// The board tabs, in order.
    pub tabs: Vec<WindowTab>,
    /// The active tab's position in `tabs`, or `None` for Home.
    pub active_tab: Option<u32>,
    /// What the UI keeps about the window's layout (sidebar width and mode, peek pin, Home's
    /// view, …), as the UI describes it.
    pub layout: serde_json::Value,
}

impl SavedWindow {
    /// A window known only by its key.
    pub(crate) fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            bounds: None,
            maximized: false,
            selected_project: None,
            tabs: Vec::new(),
            active_tab: None,
            layout: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// `windows.json` in the app-data folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowsFile {
    /// The format version, `1`.
    pub version: u32,
    /// The windows, oldest first.
    pub windows: Vec<SavedWindow>,
}

impl Default for WindowsFile {
    fn default() -> Self {
        Self {
            version: 1,
            windows: Vec::new(),
        }
    }
}

/// The most windows `windows.json` keeps; past it the oldest is dropped.
pub const MAX_WINDOWS: usize = 16;

impl WindowsFile {
    /// The window with `key`, created (and the oldest dropped past [`MAX_WINDOWS`]) when absent.
    pub(crate) fn entry(&mut self, key: &str) -> &mut SavedWindow {
        let index = match self.windows.iter().position(|window| window.key == key) {
            Some(index) => index,
            None => {
                self.windows.push(SavedWindow::new(key));
                let excess = self.windows.len().saturating_sub(MAX_WINDOWS);
                self.windows.drain(..excess);
                self.windows.len() - 1
            }
        };
        &mut self.windows[index]
    }
}

/// One page's camera and lens in a board's view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageView {
    /// The camera's x.
    pub x: f64,
    /// The camera's y.
    pub y: f64,
    /// The zoom.
    pub zoom: f64,
    /// The page's lens, or `None` for its default view.
    #[serde(default, deserialize_with = "lens")]
    pub lens: Option<View>,
}

/// A board's view state (`state.json` `boards[id]`): its last active page and each
/// page's camera and lens.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoardView {
    /// The last active page.
    pub page: Option<String>,
    /// Each page's camera and lens, by page id.
    pub pages: BTreeMap<String, PageView>,
}

/// The most boards `state.json` keeps view state for; the least recently written go first.
pub const MAX_BOARD_VIEWS: usize = 500;
/// The most pages one board's view state keeps.
pub const MAX_PAGE_VIEWS: usize = 200;

/// `.unfold/local/state.json`: what one project remembers on this machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalState {
    /// The format version, `1`.
    pub version: u32,
    /// Whether background updates are on for the project.
    pub background_updates: bool,
    /// Board folders with no board in them, sorted (decision U-47).
    pub empty_folders: Vec<String>,
    /// View state by board id, least recently written first (the file keeps that order).
    pub boards: IndexMap<String, BoardView>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: 1,
            background_updates: true,
            empty_folders: Vec::new(),
            boards: IndexMap::new(),
        }
    }
}

impl LocalState {
    /// The view state of board `id`.
    pub fn board_view(&self, id: &str) -> Option<&BoardView> {
        self.boards.get(id)
    }

    /// Replaces board `id`'s view state, making it the most recently written. Past
    /// [`MAX_PAGE_VIEWS`] pages, pages other than the active one are dropped (by page id); past
    /// [`MAX_BOARD_VIEWS`] boards, the least recently written are.
    pub fn put_board_view(&mut self, id: &str, mut view: BoardView) {
        while view.pages.len() > MAX_PAGE_VIEWS {
            let Some(drop) = view
                .pages
                .keys()
                .rev()
                .find(|page| view.page.as_ref() != Some(*page))
                .cloned()
            else {
                break;
            };
            view.pages.remove(&drop);
        }
        self.boards.shift_remove(id);
        self.boards.insert(id.to_owned(), view);
        let excess = self.boards.len().saturating_sub(MAX_BOARD_VIEWS);
        self.boards.drain(..excess);
    }

    /// Moves board `from`'s view state to `to` (the board's id changed: first Publish, Move back
    /// to drafts), replacing any state `to` had. Whether there was state to move.
    pub fn move_board_view(&mut self, from: &str, to: &str) -> bool {
        if from == to {
            return self.boards.contains_key(from);
        }
        let Some(index) = self.boards.get_index_of(from) else {
            return false;
        };
        let replaced = self.boards.get_index_of(to);
        let Some(view) = self.boards.shift_remove(from) else {
            return false;
        };
        self.boards.shift_remove(to);
        // The moved state keeps its place in the write order.
        let position = index - usize::from(replaced.is_some_and(|replaced| replaced < index));
        let (moved, _) = self.boards.insert_full(to.to_owned(), view);
        self.boards.move_index(moved, position);
        true
    }

    /// Forgets board `id`'s view state (the board was deleted); whether it had any.
    pub fn remove_board_view(&mut self, id: &str) -> bool {
        self.boards.shift_remove(id).is_some()
    }
}

/// Reads a lens (`"code"`, `"flow"`, or `null`).
fn lens<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<View>, D::Error> {
    match Option::<String>::deserialize(deserializer)?.as_deref() {
        None => Ok(None),
        Some("code") => Ok(Some(View::Code)),
        Some("flow") => Ok(Some(View::Flow)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "unknown lens `{other}`: expected `code`, `flow`, or null"
        ))),
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
    /// Whether background updates start on for newly compiled projects.
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

/// The app-data folder: preferences and remembered windows. Cheap to share; every method takes
/// `&self`, and writes to one file are serialized.
#[derive(Debug)]
pub struct AppData {
    dir: PathBuf,
    preferences: Mutex<()>,
    windows: Mutex<()>,
}

impl AppData {
    /// The app data in `dir` (created when first written).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            preferences: Mutex::new(()),
            windows: Mutex::new(()),
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
        let _guard = guard(&self.preferences);
        write_json(
            &self.dir.join("preferences.json"),
            preferences,
            "preferences.json",
        )
    }

    /// The remembered windows.
    pub fn windows(&self) -> WindowsFile {
        read_json(&self.dir.join("windows.json"))
    }

    /// Changes the remembered windows and saves them; returns what `change` returned.
    pub fn update_windows<T>(&self, change: impl FnOnce(&mut WindowsFile) -> T) -> Result<T> {
        let _guard = guard(&self.windows);
        let path = self.dir.join("windows.json");
        let mut windows: WindowsFile = read_json(&path);
        let result = change(&mut windows);
        write_json(&path, &windows, "windows.json")?;
        Ok(result)
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

    fn view(page: &str) -> BoardView {
        BoardView {
            page: Some(page.to_owned()),
            pages: BTreeMap::from([(
                page.to_owned(),
                PageView {
                    x: -120.0,
                    y: 40.0,
                    zoom: 0.85,
                    lens: Some(View::Flow),
                },
            )]),
        }
    }

    #[test]
    fn local_state_round_trips_in_write_order_and_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("local/state.json");
        assert_eq!(read_json::<LocalState>(&path), LocalState::default());
        let mut state = LocalState {
            background_updates: false,
            empty_folders: vec!["Checkout/Refunds".into()],
            ..LocalState::default()
        };
        state.put_board_view(".unfold/boards/z.json", view("page-1"));
        state.put_board_view(".unfold/boards/a.json", view("page-2"));
        write_json(&path, &state, "state.json").expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"backgroundUpdates\": false"));
        assert!(text.contains("\"lens\": \"flow\""));
        assert!(text.find("z.json") < text.find("a.json"), "{text}");
        assert!(text.ends_with("}\n"));
        assert_eq!(read_json::<LocalState>(&path), state);
        std::fs::write(&path, "{ not json").expect("write");
        assert_eq!(read_json::<LocalState>(&path), LocalState::default());
    }

    #[test]
    fn board_views_move_with_their_board_and_keep_the_newest() {
        let mut state = LocalState::default();
        state.put_board_view("draft", view("page-1"));
        state.put_board_view("other", view("page-1"));
        assert!(state.move_board_view("draft", "published"));
        assert!(!state.move_board_view("draft", "published"));
        assert_eq!(state.board_view("published"), Some(&view("page-1")));
        assert!(state.board_view("draft").is_none());
        assert!(state.remove_board_view("published"));
        assert!(!state.remove_board_view("published"));
        for index in 0..MAX_BOARD_VIEWS + 3 {
            state.put_board_view(&format!("b{index}"), view("page-1"));
        }
        assert_eq!(state.boards.len(), MAX_BOARD_VIEWS);
        assert!(state.board_view("other").is_none(), "the oldest went first");
        let mut big = view("page-0");
        for index in 1..MAX_PAGE_VIEWS + 5 {
            big.pages
                .insert(format!("page-{index}"), view("x").pages["x"].clone());
        }
        state.put_board_view("big", big);
        let kept = state.board_view("big").expect("big");
        assert_eq!(kept.pages.len(), MAX_PAGE_VIEWS);
        assert!(kept.pages.contains_key("page-0"), "the active page stays");
    }

    #[test]
    fn windows_are_capped_oldest_first() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = AppData::new(dir.path().join("Unfold"));
        assert!(app.windows().windows.is_empty());
        for index in 0..MAX_WINDOWS + 2 {
            app.update_windows(|file| file.entry(&format!("w{index}")).maximized = true)
                .expect("save");
        }
        let windows = app.windows().windows;
        assert_eq!(windows.len(), MAX_WINDOWS);
        assert_eq!(windows[0].key, "w2");
        assert!(windows.iter().all(|window| window.maximized));
    }

    #[test]
    fn the_update_check_is_on_unless_turned_off() {
        assert!(Preferences::default().updates.enabled);
        let older: Preferences =
            serde_json::from_str(r#"{"theme":"dark"}"#).expect("parse older preferences");
        assert!(older.updates.enabled);
        let off: Preferences =
            serde_json::from_str(r#"{"updates":{"enabled":false}}"#).expect("parse preferences");
        assert!(!off.updates.enabled);
    }

    #[test]
    fn preferences_round_trip() {
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
    }
}
