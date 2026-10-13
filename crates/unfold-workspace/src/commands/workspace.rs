//! Workspace commands: the project rows every window shows, adding (from the
//! main process only, or a folder "Opened before"), removing, ordering, expanding, and selecting
//! projects, and the branch and worktree switcher (worktree metadata).

use std::path::Path;

use serde::Deserialize;
use ts_rs::TS;

use super::{NoArgs, ProjectArgs};
use crate::git::{self, GitInfo};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{AddedProject, ProjectId, ProjectRow, WorkspaceView};

/// `workspace`: every project row (reading each repository's `HEAD`), the default selection,
/// recent boards, and the folders "Opened before".
pub fn workspace(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<WorkspaceView> {
    Ok(ctx.projects().view())
}

/// The arguments of `add_project` and `readd_project`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AddProjectArgs {
    /// The folder's absolute path (from a dialog, a drop, or the command line; for
    /// `readd_project`, a path "Opened before" lists).
    pub path: String,
}

/// `add_project` (main process only): adds the folder as a resting, expanded row at the end, or
/// answers `exists` with the row that has it (the same folder, or another worktree of its
/// repository). Writes nothing in the folder.
pub fn add_project(ctx: &Ctx<'_>, args: AddProjectArgs) -> CommandResult<AddedProject> {
    ctx.require_trusted("add_project")?;
    let path = Path::new(&args.path);
    if !path.is_absolute() {
        return Err(CommandError::invalid_args("add_project takes an absolute path"));
    }
    Ok(ctx.projects().add(path)?)
}

/// `readd_project`: adds back a folder "Opened before" lists (and only such a folder: a window
/// can't name other paths outside its projects), exactly like `add_project`.
pub fn readd_project(ctx: &Ctx<'_>, args: AddProjectArgs) -> CommandResult<AddedProject> {
    if !ctx.projects().was_removed(&args.path) {
        return Err(CommandError::not_found(format!(
            "`{}` is not a folder removed from the workspace",
            args.path
        )));
    }
    Ok(ctx.projects().add(Path::new(&args.path))?)
}

/// `remove_project`: removes the project from the workspace (never touching its `.unfold/`).
/// Refused with `boards_open` while one of its boards is open.
pub fn remove_project(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<()> {
    Ok(ctx.projects().remove(&args.project)?)
}

/// The arguments of `reorder_projects`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReorderProjectsArgs {
    /// Every project id, once, in the new sidebar order.
    pub order: Vec<ProjectId>,
}

/// `reorder_projects`: puts the rows in the given order.
pub fn reorder_projects(ctx: &Ctx<'_>, args: ReorderProjectsArgs) -> CommandResult<WorkspaceView> {
    ctx.projects()
        .reorder(&args.order)?
        .ok_or_else(|| CommandError::invalid_args("the order must list every project once"))
}

/// The arguments of `set_project_expanded`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SelectProjectArgs {
    /// The project the calling window's Home shows, or `None` for none.
    pub project: Option<ProjectId>,
}

/// `select_project`: the calling window's Home selection. It keeps the project live (it wakes
/// before this returns, so the window's next query finds it live) and becomes the default for
/// new windows.
pub fn select_project(ctx: &Ctx<'_>, args: SelectProjectArgs) -> CommandResult<()> {
    Ok(ctx.projects().select(ctx.window, args.project.as_ref())?)
}

/// `git_info`: the branch and worktrees of the project's repository, read from Git's files, or
/// `None` outside a repository. Never wakes the project.
pub fn git_info(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<Option<GitInfo>> {
    Ok(git::info(ctx.project(&args.project)?.root()))
}

/// The arguments of `switch_worktree`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SwitchWorktreeArgs {
    /// The project.
    pub project: ProjectId,
    /// An available worktree's `path`, exactly as `git_info` lists it.
    pub path: String,
}

/// `switch_worktree`: points the project at another worktree of its repository (its graph,
/// boards, drafts, and `state.json` become that folder's). Refused with `not_a_worktree` for a
/// path `git_info` doesn't list as available, and with `boards_open` while one of its boards is
/// open.
pub fn switch_worktree(ctx: &Ctx<'_>, args: SwitchWorktreeArgs) -> CommandResult<ProjectRow> {
    Ok(ctx.projects().switch_worktree(&args.project, &args.path)?)
}
