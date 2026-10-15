//! Board folders (board folder metadata): one level deep, each a single name.
//!
//! A board sits in a folder through its `folder` field; the sidebar groups the boards by it. A
//! folder with no board in it is kept in `state.json`'s `emptyFolders` until a board goes into
//! it, and a folder a board leaves stays there, empty, until it is deleted. A folder is metadata
//! Rust owns: these commands work on any board, open or not, moving the file as it is now, and
//! an open board's session follows the move, so the window's next save keeps it. A file that
//! isn't a valid board is never moved: that would write over it.

use std::collections::BTreeMap;

use unfold_reader::board::files::{BoardFile, Loaded};
use unfold_reader::board::is_folder_name;

use super::files::Files;
use super::graphs::Source;
use super::lists::{label_of, summary};
use super::session::{BoardExtras, Session};
use super::types::{BoardSummary, RenameFolderArgs, RenamedFolder, SetBoardFolderArgs};
use super::{announce, parse_id};
use crate::commands::{ProjectPathArgs, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::workspace::Workspace;

/// A folder name a command names, checked (`invalid_folder`).
pub(super) fn folder_arg(name: String) -> CommandResult<String> {
    if is_folder_name(&name) {
        Ok(name)
    } else {
        Err(CommandError::new(
            "invalid_folder",
            "A folder's name is 1 to 64 characters, without “/”, “\\”, control characters, or \
             spaces at either end, and isn't “.” or “..”: folders hold boards, not other folders.",
        ))
    }
}

/// A board went into `folder`: it is no longer empty.
pub(super) fn folder_used(workspace: &Workspace, folder: &str) {
    if workspace
        .local_state()
        .empty_folders
        .iter()
        .any(|empty| empty == folder)
    {
        // Remembered state is a convenience: a failed write loses only an empty folder.
        let _ = workspace.update_local_state(|state| {
            state.empty_folders.retain(|empty| empty != folder);
        });
    }
}

/// A board left `folder`: it stays, empty, when no other board is in it.
pub(super) fn folder_left(files: &Files<'_>, workspace: &Workspace, folder: &str) {
    let boards = boards(files);
    if !exists(&boards, &workspace.local_state().empty_folders, folder) {
        let _ = workspace.update_local_state(|state| {
            state.empty_folders.push(folder.to_owned());
            state.empty_folders.sort();
            state.empty_folders.dedup();
        });
    }
}

/// Every board of the project that reads, by board id.
fn boards(files: &Files<'_>) -> BTreeMap<String, (BoardFile, Loaded)> {
    files
        .boards()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file| {
            let loaded = files.load(&file).ok()?;
            Some((file.relative(), (file, loaded)))
        })
        .collect()
}

/// Whether folder `name` exists: a board or an empty folder is in it.
fn exists(boards: &BTreeMap<String, (BoardFile, Loaded)>, empty: &[String], name: &str) -> bool {
    boards
        .values()
        .any(|(_, loaded)| loaded.board.folder.as_deref() == Some(name))
        || empty.iter().any(|folder| folder == name)
}

/// Moves board `id` (read as `loaded`) into `folder`, writing its file; an open session on the
/// same version follows. Whether it moved.
fn move_board(
    files: &Files<'_>,
    extras: &BoardExtras,
    sessions: &mut BTreeMap<String, Session>,
    id: &str,
    file: &BoardFile,
    loaded: Loaded,
    folder: Option<String>,
) -> CommandResult<bool> {
    if loaded.board.folder == folder {
        return Ok(false);
    }
    let mut board = loaded.board;
    board.folder = folder;
    let revision = files.save(file, &board)?;
    extras.rekey(id, loaded.revision, revision);
    if let Some(session) = sessions
        .get_mut(id)
        .filter(|session| session.revision == loaded.revision)
    {
        session.board.folder = board.folder;
        session.moved.push(session.revision);
        session.revision = revision;
        session.skipped = None;
    }
    Ok(true)
}

/// `set_board_folder`: moves a board into a folder (or to the top level), whether or not it is
/// open. An open board whose file is gone moves its session's version, which is what the
/// window's next save would write back; a file that isn't a valid board is refused.
pub fn set_board_folder(ctx: &Ctx<'_>, args: SetBoardFolderArgs) -> CommandResult<BoardSummary> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    let folder = args.folder.map(folder_arg).transpose()?;
    let workspace = project.wake()?;
    let extras = project.extras();
    let files = Files::live(&workspace);
    let source = Source::live(&workspace)?;
    let left = {
        let mut sessions = extras.boards.sessions();
        let loaded = match (files.load(&file), sessions.get(&args.id)) {
            (Ok(loaded), _) => loaded,
            (Err(error), Some(session)) if error.code == "not_found" => Loaded {
                board: session.board.clone(),
                revision: session.revision,
            },
            (Err(error), _) => return Err(error),
        };
        let previous = loaded.board.folder.clone();
        let moved = move_board(
            &files,
            &extras.boards,
            &mut sessions,
            &args.id,
            &file,
            loaded,
            folder.clone(),
        )?;
        moved.then_some(previous).flatten()
    };
    if let Some(folder) = &folder {
        folder_used(&workspace, folder);
    }
    if let Some(left) = &left {
        folder_left(&files, &workspace, left);
    }
    let loaded = files.load(&file);
    let label = label_of(Some(&(extras, source)), &args.id, &loaded);
    announce(&workspace, &mut vec![args.id.clone()], true);
    Ok(summary(
        project.id(),
        &file,
        files.modified_ms(&file),
        loaded,
        label,
    ))
}

/// `create_folder`: adds an empty folder (nothing when the folder exists).
pub fn create_folder(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    let name = folder_arg(args.path)?;
    let workspace = project.wake()?;
    let files = Files::live(&workspace);
    if exists(&boards(&files), &workspace.local_state().empty_folders, &name) {
        return Ok(());
    }
    workspace.update_local_state(|state| {
        state.empty_folders.push(name);
        state.empty_folders.sort();
    })?;
    announce(&workspace, &mut Vec::new(), true);
    Ok(())
}

/// `rename_folder`: moves every board in folder `from` to folder `to`, and renames it when it
/// is empty.
pub fn rename_folder(ctx: &Ctx<'_>, args: RenameFolderArgs) -> CommandResult<RenamedFolder> {
    let project = ctx.project(&args.project)?;
    let from = folder_arg(args.from)?;
    let to = folder_arg(args.to)?;
    let workspace = project.wake()?;
    let extras = project.extras();
    let files = Files::live(&workspace);
    let boards = boards(&files);
    let empty = workspace.local_state().empty_folders;
    if !exists(&boards, &empty, &from) {
        return Err(CommandError::not_found(format!("there is no folder `{from}`")));
    }
    if exists(&boards, &empty, &to) {
        return Err(CommandError::new(
            "folder_exists",
            format!("A folder named “{to}” is already there."),
        ));
    }
    let mut moved = Vec::new();
    {
        let mut sessions = extras.boards.sessions();
        for (id, (file, loaded)) in boards {
            if loaded.board.folder.as_deref() != Some(from.as_str()) {
                continue;
            }
            move_board(
                &files,
                &extras.boards,
                &mut sessions,
                &id,
                &file,
                loaded,
                Some(to.clone()),
            )?;
            moved.push(id);
        }
    }
    workspace.update_local_state(|state| {
        for folder in &mut state.empty_folders {
            if *folder == from {
                folder.clone_from(&to);
            }
        }
        state.empty_folders.sort();
        state.empty_folders.dedup();
    })?;
    let boards = count(moved.len());
    announce(&workspace, &mut moved, true);
    Ok(RenamedFolder { boards })
}

/// `delete_folder`: removes an empty folder (`folder_not_empty` while a board is in it).
pub fn delete_folder(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    let name = folder_arg(args.path)?;
    let workspace = project.wake()?;
    let files = Files::live(&workspace);
    if exists(&boards(&files), &[], &name) {
        return Err(CommandError::new(
            "folder_not_empty",
            format!("folder `{name}` has boards in it"),
        ));
    }
    if !exists(&BTreeMap::new(), &workspace.local_state().empty_folders, &name) {
        return Err(CommandError::not_found(format!("there is no folder `{name}`")));
    }
    workspace.update_local_state(|state| state.empty_folders.retain(|folder| *folder != name))?;
    announce(&workspace, &mut Vec::new(), true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_one_name() {
        assert_eq!(folder_arg("Checkout".into()).expect("valid"), "Checkout");
        for bad in ["", "Checkout/Refunds", "/x", "x/", "..", " x", "a\\b"] {
            assert_eq!(
                folder_arg(bad.into()).expect_err(bad).code,
                "invalid_folder",
                "{bad}"
            );
        }
        // A name's length is in characters: 33 emoji fit.
        assert!(folder_arg("📦".repeat(33)).is_ok());
        // The folder dialogs show the refusal as it is, under the name.
        let refused = folder_arg("Checkout/Refunds".into()).expect_err("a path");
        assert!(
            refused
                .message
                .starts_with("A folder's name is 1 to 64 characters"),
            "{}",
            refused.message
        );
    }
}
