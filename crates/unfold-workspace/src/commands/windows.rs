//! Remembered windows (architecture §11.2): each window's tabs, layout, and open boards in the
//! folder's `state.json`, the saved windows the main process restores when a folder opens, and
//! window bounds.
//!
//! Window IDs change every run, so windows are remembered by a stable `key` the main process
//! assigns (and hands its window through `window_info`).

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::Layout;
use crate::state::{FolderState, WindowBounds, WindowState, read_json};

/// The arguments of a command about one remembered window.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WindowKeyArgs {
    /// The window's stable key.
    pub key: String,
}

/// `window_state`: what the window with `key` remembered (tabs, layout, open boards), if
/// anything.
pub fn window_state(ctx: &Ctx<'_>, args: WindowKeyArgs) -> CommandResult<Option<WindowState>> {
    let workspace = ctx.workspace()?;
    Ok(workspace
        .folder_state()
        .windows
        .into_iter()
        .find(|window| window.key == args.key))
}

/// The arguments of `save_window_state`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveWindowStateArgs {
    /// The window's state; `key` names it. Bounds already saved by `save_window_bounds` are
    /// kept when `bounds` is absent.
    pub state: WindowState,
}

/// The longest window key accepted.
const MAX_KEY: usize = 200;

fn check_key(key: &str) -> CommandResult<()> {
    if key.is_empty() || key.len() > MAX_KEY {
        return Err(CommandError::invalid_args(
            "a window key must have 1 to 200 characters",
        ));
    }
    Ok(())
}

/// `save_window_state`: remembers the calling window's tabs, layout, and open boards in
/// `state.json` (kept in memory until the first Compile creates `local/`).
pub fn save_window_state(ctx: &Ctx<'_>, args: SaveWindowStateArgs) -> CommandResult<()> {
    check_key(&args.state.key)?;
    let workspace = ctx.workspace()?;
    let mut state = args.state;
    workspace.update_folder_state(|folder| {
        if state.bounds.is_none()
            && let Some(saved) = folder.windows.iter().find(|window| window.key == state.key)
        {
            state.bounds = saved.bounds;
            state.maximized = saved.maximized;
        }
        folder.put_window(state);
    })?;
    Ok(())
}

/// `forget_window_state`: forgets a window the person closed, so it doesn't come back.
pub fn forget_window_state(ctx: &Ctx<'_>, args: WindowKeyArgs) -> CommandResult<()> {
    let workspace = ctx.workspace()?;
    workspace.update_folder_state(|folder| {
        folder.remove_window(&args.key);
    })?;
    Ok(())
}

/// The arguments of `folder_windows`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FolderWindowsArgs {
    /// The folder's absolute path (from the folder picker or the recent list).
    pub root: String,
}

/// A folder's remembered windows.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FolderWindows {
    /// The folder's canonical path.
    pub folder: String,
    /// Its windows, as they should come back.
    pub windows: Vec<WindowState>,
}

/// `folder_windows`: the saved windows of a folder, so the main process can restore them before
/// any window opens it. Main process only (`trusted`): it reads another folder's state.
pub fn folder_windows(ctx: &Ctx<'_>, args: FolderWindowsArgs) -> CommandResult<FolderWindows> {
    ctx.require_trusted("folder_windows")?;
    let root = std::fs::canonicalize(Path::new(&args.root))
        .map_err(|error| CommandError::new("not_a_folder", format!("{}: {error}", args.root)))?;
    if !root.is_dir() {
        return Err(CommandError::new(
            "not_a_folder",
            format!("{} is not a folder", args.root),
        ));
    }
    let folder = root.to_string_lossy().into_owned();
    // An open folder's state may not be written yet (before its first Compile).
    let open = ctx
        .registry()
        .workspaces()
        .into_iter()
        .find(|workspace| workspace.folder() == folder);
    let state: FolderState = match open {
        Some(workspace) => workspace.folder_state(),
        None => {
            let layout = Layout::discover(&root, &ctx.registry().config().fallback_cache_dir);
            read_json(&layout.state_file())
        }
    };
    Ok(FolderWindows {
        folder,
        windows: state.windows,
    })
}

/// The arguments of `save_window_bounds`, with the window's key.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WindowBoundsArgs {
    /// The window's stable key; absent for the welcome window.
    #[serde(default)]
    #[ts(optional)]
    pub key: Option<String>,
    /// The bounds.
    pub bounds: WindowBounds,
    /// Whether it is maximized.
    #[serde(default)]
    pub maximized: bool,
}

/// `save_window_bounds`: remembers a window's position and size: in its folder's `state.json`,
/// or, for the welcome window (no folder, no key), in `recent.json`.
pub fn save_window_bounds(ctx: &Ctx<'_>, args: WindowBoundsArgs) -> CommandResult<()> {
    match (&args.key, ctx.workspace()) {
        (Some(key), Ok(workspace)) => {
            check_key(key)?;
            workspace.update_folder_state(|folder| {
                let mut window = folder
                    .windows
                    .iter()
                    .find(|window| &window.key == key)
                    .cloned()
                    .unwrap_or_else(|| WindowState {
                        key: key.clone(),
                        ..WindowState::default()
                    });
                window.bounds = Some(args.bounds);
                window.maximized = args.maximized;
                folder.put_window(window);
            })?;
            Ok(())
        }
        (None, _) => {
            ctx.registry().app().set_welcome_bounds(Some(args.bounds))?;
            Ok(())
        }
        (Some(_), Err(_)) => Err(CommandError::new(
            "no_window_folder",
            "the window shows no folder; save the welcome window's bounds without a key",
        )),
    }
}
