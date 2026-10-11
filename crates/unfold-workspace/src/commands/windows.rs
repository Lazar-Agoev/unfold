//! Remembered windows (desktop/README.md#transport-and-windows): `windows.json` in the app-data folder holds the
//! windows that come back at launch: each one's Home selection, board tabs, active tab, and
//! layout (which the UI saves), and its position and size (which the main process saves).
//!
//! Window ids change every run, so windows are remembered by a stable `key` the main process
//! assigns (and hands its window through `window_info`). The main process restores the saved
//! windows at launch (`saved_windows`), saves bounds as windows move (`save_window_bounds`), and
//! forgets a window the person closed (`forget_window`); those three are trusted-only.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::NoArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::state::{SavedWindow, WindowBounds, WindowTab};

/// The longest window key accepted.
const MAX_KEY: usize = 200;
/// The most tabs a window remembers.
pub const MAX_TABS: usize = 100;
/// The most bytes a window's layout may take, serialized.
pub const MAX_LAYOUT_BYTES: usize = 64 * 1024;

fn check_key(key: &str) -> CommandResult<()> {
    if key.is_empty() || key.chars().count() > MAX_KEY {
        return Err(CommandError::invalid_args(
            "a window key must have 1 to 200 characters",
        ));
    }
    Ok(())
}

/// The arguments of a command about one remembered window.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WindowKeyArgs {
    /// The window's stable key.
    pub key: String,
}

/// `window_state`: what the window with `key` remembered, if anything.
pub fn window_state(ctx: &Ctx<'_>, args: WindowKeyArgs) -> CommandResult<Option<SavedWindow>> {
    Ok(ctx
        .projects()
        .app()
        .windows()
        .windows
        .into_iter()
        .find(|window| window.key == args.key))
}

/// What the UI saves about its window.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowStateInput {
    /// The window's stable key.
    pub key: String,
    /// The project its Home shows.
    pub selected_project: Option<ProjectId>,
    /// Its board tabs, in order (at most 100).
    pub tabs: Vec<WindowTab>,
    /// The active tab's position in `tabs`, or `None` for Home.
    pub active_tab: Option<u32>,
    /// Its layout, as the UI describes it (at most 64 KiB serialized).
    pub layout: serde_json::Value,
}

/// The arguments of `save_window_state`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveWindowStateArgs {
    /// The window's state; `key` names it.
    pub state: WindowStateInput,
}

/// `save_window_state`: remembers the calling window's Home selection, tabs, active tab, and
/// layout, keeping its saved bounds.
pub fn save_window_state(ctx: &Ctx<'_>, args: SaveWindowStateArgs) -> CommandResult<()> {
    let state = args.state;
    check_key(&state.key)?;
    if state.tabs.len() > MAX_TABS {
        return Err(CommandError::invalid_args(format!(
            "a window remembers at most {MAX_TABS} tabs"
        )));
    }
    let layout_bytes = serde_json::to_vec(&state.layout).map_or(usize::MAX, |bytes| bytes.len());
    if layout_bytes > MAX_LAYOUT_BYTES {
        return Err(CommandError::invalid_args(
            "a window's layout must take at most 64 KiB",
        ));
    }
    ctx.projects().app().update_windows(|file| {
        let window = file.entry(&state.key);
        window.selected_project = state.selected_project;
        window.tabs = state.tabs;
        window.active_tab = state.active_tab;
        window.layout = state.layout;
    })?;
    Ok(())
}

/// The windows that come back at launch.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SavedWindows {
    /// Oldest first.
    pub windows: Vec<SavedWindow>,
}

/// `saved_windows`: every remembered window, for restoring them at launch (main process only).
pub fn saved_windows(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<SavedWindows> {
    ctx.require_trusted("saved_windows")?;
    Ok(SavedWindows {
        windows: ctx.projects().app().windows().windows,
    })
}

/// The arguments of `save_window_bounds`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WindowBoundsArgs {
    /// The window's stable key.
    pub key: String,
    /// Its position and size.
    pub bounds: WindowBounds,
    /// Whether it is maximized.
    pub maximized: bool,
}

/// `save_window_bounds`: remembers a window's position and size, remembering the window if it
/// wasn't yet (main process only).
pub fn save_window_bounds(ctx: &Ctx<'_>, args: WindowBoundsArgs) -> CommandResult<()> {
    ctx.require_trusted("save_window_bounds")?;
    check_key(&args.key)?;
    ctx.projects().app().update_windows(|file| {
        let window = file.entry(&args.key);
        window.bounds = Some(args.bounds);
        window.maximized = args.maximized;
    })?;
    Ok(())
}

/// `forget_window`: forgets a window the person closed, so it doesn't come back (main process
/// only).
pub fn forget_window(ctx: &Ctx<'_>, args: WindowKeyArgs) -> CommandResult<()> {
    ctx.require_trusted("forget_window")?;
    ctx.projects()
        .app()
        .update_windows(|file| file.windows.retain(|window| window.key != args.key))?;
    Ok(())
}
