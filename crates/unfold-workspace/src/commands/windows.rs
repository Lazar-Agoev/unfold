//! The remembered window (desktop/README.md#transport-and-windows): `window.json` in the app-data
//! folder holds the one window's position and size and the last board it showed. The main process
//! reads it at launch (`window_state`) and saves it (`save_window_state`); both are trusted-only.
//! The window's renderer reports the board it shows to the main process, never to these commands.

use serde::Deserialize;
use ts_rs::TS;

use super::NoArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::state::WindowState;

/// `window_state`: the remembered window, or `None` when nothing is saved (main process only).
pub fn window_state(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<Option<WindowState>> {
    ctx.require_trusted("window_state")?;
    Ok(ctx.projects().app().window())
}

/// The arguments of `save_window_state`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveWindowStateArgs {
    /// The window's position and size, the board it shows, and the Show all calls switch.
    pub state: WindowState,
}

/// `save_window_state`: remembers the window's position and size, the board it shows, and the
/// Show all calls switch (main process only).
pub fn save_window_state(ctx: &Ctx<'_>, args: SaveWindowStateArgs) -> CommandResult<()> {
    ctx.require_trusted("save_window_state")?;
    ctx.projects().app().set_window(&args.state)?;
    Ok(())
}
