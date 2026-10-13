//! Board folders (board folder metadata).
//!
//! A board sits in a folder through its `folder` field; the sidebar builds the tree from the
//! boards. A folder with no board in or under it is kept in `state.json`'s `emptyFolders` until a
//! board goes into it, and a folder a board leaves stays there, empty, until it is deleted. A
//! folder is metadata Rust owns: these commands work on any board, open or not, and a published
//! board's move is an unpublished change (its changes copy). An open board's session follows the
//! move, so the window's next save doesn't conflict with it.

use std::collections::BTreeMap;

use unfold_reader::board::files::BoardFile;
use unfold_reader::board::is_folder_path;

use super::files::{Files, Working};
use super::graphs::Resolver;
use super::lists::summary;
use super::session::{BoardExtras, Session, health_of};
use super::types::{BoardSummary, RenameFolderArgs, RenamedFolder, SetBoardFolderArgs};
use super::{announce, parse_id};
use crate::commands::{ProjectPathArgs, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::workspace::Workspace;

/// A folder path a command names, checked (`invalid_folder`).
pub(super) fn folder_arg(path: String) -> CommandResult<String> {
    if is_folder_path(&path) {
        Ok(path)
    } else {
        Err(CommandError::new(
            "invalid_folder",
            format!(
                "`{path}` is not a folder path: 1 to 8 names joined by `/`, each 1 to 64 characters, \
                 not `.` or `..`, without `\\`, control characters, or surrounding spaces"
            ),
        ))
    }
}

/// Whether `folder` is folder `path` or inside it.
fn under(folder: &str, path: &str) -> bool {
    folder
        .strip_prefix(path)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// A board went into `folder`: it and the folders it is in are no longer empty.
pub(super) fn folder_used(workspace: &Workspace, folder: &str) {
    if workspace
        .local_state()
        .empty_folders
        .iter()
        .any(|empty| under(folder, empty))
    {
        // Remembered state is a convenience: a failed write loses only an empty folder.
        let _ = workspace.update_local_state(|state| {
            state.empty_folders.retain(|empty| !under(folder, empty));
        });
    }
}

/// A board left `folder`: it stays, empty, when nothing else is in or under it.
pub(super) fn folder_left(files: &Files<'_>, workspace: &Workspace, folder: &str) {
    let boards = working_versions(files);
    if !exists(&boards, &workspace.local_state().empty_folders, folder) {
        let _ = workspace.update_local_state(|state| {
            state.empty_folders.push(folder.to_owned());
            state.empty_folders.sort();
            state.empty_folders.dedup();
        });
    }
}

/// The working version of every board of the project that reads, by board id.
fn working_versions(files: &Files<'_>) -> BTreeMap<String, (BoardFile, Working)> {
    files
        .boards()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file| {
            let working = files.working(&file).ok()?;
            Some((file.relative(), (file, working)))
        })
        .collect()
}

/// Whether folder `path` exists: a board or an empty folder is in or under it.
fn exists(boards: &BTreeMap<String, (BoardFile, Working)>, empty: &[String], path: &str) -> bool {
    boards.values().any(|(_, working)| {
        working
            .loaded
            .board
            .folder
            .as_deref()
            .is_some_and(|folder| under(folder, path))
    }) || empty.iter().any(|folder| under(folder, path))
}

/// Moves board `id` (whose working version is `working`) into `folder`, writing its working
/// version; an open session on the same version follows. Whether it moved.
fn move_board(
    files: &Files<'_>,
    extras: &BoardExtras,
    sessions: &mut BTreeMap<String, Session>,
    id: &str,
    file: &BoardFile,
    working: Working,
    folder: Option<String>,
) -> CommandResult<bool> {
    if working.loaded.board.folder == folder {
        return Ok(false);
    }
    let mut board = working.loaded.board.clone();
    board.folder = folder;
    let written = files.write(file, board, working.base())?;
    if written.kind == working.kind {
        extras.rekey(id, working.loaded.revision, written.revision);
    }
    if let Some(session) = sessions
        .get_mut(id)
        .filter(|session| session.revision == working.loaded.revision && session.kind == working.kind)
    {
        let health = session.health.clone();
        session.wrote(written, health);
    }
    Ok(true)
}

/// `set_board_folder`: moves a board into a folder (or to the top level), whether or not it is
/// open.
pub fn set_board_folder(ctx: &Ctx<'_>, args: SetBoardFolderArgs) -> CommandResult<BoardSummary> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    let folder = args.folder.map(folder_arg).transpose()?;
    let workspace = project.wake()?;
    let extras = project.extras();
    let files = Files::live(&workspace);
    let mut resolver = Resolver::live(ctx.projects(), &workspace)?;
    let (left, working) = {
        let mut sessions = extras.boards.sessions();
        let working = files.working(&file)?;
        let previous = working.loaded.board.folder.clone();
        let moved = move_board(
            &files,
            &extras.boards,
            &mut sessions,
            &args.id,
            &file,
            working,
            folder.clone(),
        )?;
        (moved.then_some(previous).flatten(), files.working(&file))
    };
    if let Some(folder) = &folder {
        folder_used(&workspace, folder);
    }
    if let Some(left) = &left {
        folder_left(&files, &workspace, left);
    }
    let label = match &working {
        Ok(working) => health_of(&extras.boards, &mut resolver, &args.id, &working.loaded)?
            .and_then(|health| health.label()),
        Err(_) => None,
    };
    announce(&workspace, &mut vec![args.id.clone()], true);
    Ok(summary(project.id(), &files, &file, working, label))
}

/// `create_folder`: adds an empty folder (nothing when the folder exists).
pub fn create_folder(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    let path = folder_arg(args.path)?;
    let workspace = project.wake()?;
    let files = Files::live(&workspace);
    if exists(
        &working_versions(&files),
        &workspace.local_state().empty_folders,
        &path,
    ) {
        return Ok(());
    }
    workspace.update_local_state(|state| {
        state.empty_folders.push(path);
        state.empty_folders.sort();
    })?;
    announce(&workspace, &mut Vec::new(), true);
    Ok(())
}

/// `rename_folder`: moves every board at or under folder `from` to the same place under `to`
/// (published boards as unpublished changes), and the empty folders with them.
pub fn rename_folder(ctx: &Ctx<'_>, args: RenameFolderArgs) -> CommandResult<RenamedFolder> {
    let project = ctx.project(&args.project)?;
    let from = folder_arg(args.from)?;
    let to = folder_arg(args.to)?;
    let workspace = project.wake()?;
    let extras = project.extras();
    let files = Files::live(&workspace);
    let boards = working_versions(&files);
    let empty = workspace.local_state().empty_folders;
    if !exists(&boards, &empty, &from) {
        return Err(CommandError::not_found(format!("there is no folder `{from}`")));
    }
    if exists(&boards, &empty, &to) {
        return Err(CommandError::new(
            "folder_exists",
            format!("a folder `{to}` already exists"),
        ));
    }
    let renamed = |folder: &str| format!("{to}{}", &folder[from.len()..]);
    // A rename that would nest a folder too deep moves nothing.
    for folder in boards
        .values()
        .filter_map(|(_, working)| working.loaded.board.folder.as_deref())
        .chain(empty.iter().map(String::as_str))
        .filter(|folder| under(folder, &from))
    {
        folder_arg(renamed(folder))?;
    }
    let mut moved = Vec::new();
    {
        let mut sessions = extras.boards.sessions();
        for (id, (file, working)) in boards {
            let Some(folder) = working
                .loaded
                .board
                .folder
                .as_deref()
                .filter(|folder| under(folder, &from))
                .map(renamed)
            else {
                continue;
            };
            move_board(
                &files,
                &extras.boards,
                &mut sessions,
                &id,
                &file,
                working,
                Some(folder),
            )?;
            moved.push(id);
        }
    }
    workspace.update_local_state(|state| {
        for folder in &mut state.empty_folders {
            if under(folder, &from) {
                *folder = renamed(folder);
            }
        }
        state.empty_folders.sort();
        state.empty_folders.dedup();
    })?;
    let boards = count(moved.len());
    announce(&workspace, &mut moved, true);
    Ok(RenamedFolder { boards })
}

/// `delete_folder`: removes an empty folder and the empty folders under it
/// (`folder_not_empty` while a board is in or under it).
pub fn delete_folder(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    let path = folder_arg(args.path)?;
    let workspace = project.wake()?;
    let files = Files::live(&workspace);
    if exists(&working_versions(&files), &[], &path) {
        return Err(CommandError::new(
            "folder_not_empty",
            format!("folder `{path}` has boards in it"),
        ));
    }
    if !exists(&BTreeMap::new(), &workspace.local_state().empty_folders, &path) {
        return Err(CommandError::not_found(format!("there is no folder `{path}`")));
    }
    workspace.update_local_state(|state| state.empty_folders.retain(|folder| !under(folder, &path)))?;
    announce(&workspace, &mut Vec::new(), true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_contain_their_subfolders_only() {
        assert!(under("Checkout", "Checkout"));
        assert!(under("Checkout/Refunds", "Checkout"));
        assert!(!under("Checkouts", "Checkout"));
        assert!(!under("Checkout", "Checkout/Refunds"));
        assert_eq!(
            folder_arg("Checkout/Refunds".into()).expect("valid"),
            "Checkout/Refunds"
        );
        for bad in ["", "/x", "x/", "a/../b", " x", "a\\b"] {
            assert_eq!(
                folder_arg(bad.into()).expect_err(bad).code,
                "invalid_folder",
                "{bad}"
            );
        }
    }
}
