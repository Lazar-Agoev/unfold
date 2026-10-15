//! Workspace commands: the project rows the window shows, adding (from the main process
//! only), removing, ordering, expanding, and selecting projects.

use std::path::Path;

use serde::Deserialize;

use super::{NoArgs, ProjectArgs};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{AddedProject, ProjectId, WorkspaceView};

/// `workspace`: every project row and the default selection.
pub fn workspace(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<WorkspaceView> {
    Ok(ctx.projects().view())
}

/// The arguments of `add_project`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct AddProjectArgs {
    /// The folder's absolute path, from the Add Project dialog.
    pub path: String,
}

/// `add_project` (main process only): adds the folder as an expanded row at the end and
/// compiles it at once, or answers `exists` with the row that already has the folder. A folder
/// Unfold can't write `.unfold/` into (read-only, or on a network volume) is refused with
/// `folder_not_writable`.
pub fn add_project(ctx: &Ctx<'_>, args: AddProjectArgs) -> CommandResult<AddedProject> {
    ctx.require_trusted("add_project")?;
    let path = Path::new(&args.path);
    if !path.is_absolute() {
        return Err(CommandError::invalid_args("add_project takes an absolute path"));
    }
    Ok(ctx.projects().add(path)?)
}

/// `remove_project`: removes the project from the workspace (never touching its `.unfold/`).
/// Refused with `boards_open` while one of its boards is open.
pub fn remove_project(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<()> {
    Ok(ctx.projects().remove(&args.project)?)
}

/// The arguments of `set_project_expanded`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SetProjectExpandedArgs {
    /// The project.
    pub project: ProjectId,
    /// Whether its sidebar row is expanded.
    pub expanded: bool,
}

/// `set_project_expanded`: expands or collapses a project's sidebar row.
pub fn set_project_expanded(ctx: &Ctx<'_>, args: SetProjectExpandedArgs) -> CommandResult<()> {
    Ok(ctx.projects().set_expanded(&args.project, args.expanded)?)
}

/// The arguments of `select_project`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SelectProjectArgs {
    /// The project the calling window's Home shows, or `None` for none.
    pub project: Option<ProjectId>,
}

/// `select_project`: the calling window's Home selection. It keeps the project live (it wakes
/// before this returns, so the window's next query finds it live), and Home shows it when the
/// window opens again.
pub fn select_project(ctx: &Ctx<'_>, args: SelectProjectArgs) -> CommandResult<()> {
    Ok(ctx.projects().select(ctx.window, args.project.as_ref())?)
}
