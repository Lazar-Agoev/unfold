//! A project's board files by board id: reading them, and writing them as boards are edited.
//!
//! A board's **id** is its file's folder-relative path, `.unfold/boards/<name>.json`. Every write
//! goes through a live project's [`Workspace`], which notes it so the watcher doesn't report the
//! app's own writes as outside edits; commands report them with `boards_changed` themselves.

use std::path::Path;
use std::time::UNIX_EPOCH;

use unfold_reader::board::files::{BOARDS_DIR, BoardDirs, BoardFile, EXTENSION, Loaded, Revision, slug};
use unfold_reader::board::{Board, BoardError, serialize};

use crate::ipc::error::{CommandError, CommandResult};
use crate::workspace::Workspace;

/// The board file an id names.
pub(super) fn parse_id(id: &str) -> CommandResult<BoardFile> {
    let name = id
        .strip_prefix(BOARDS_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.strip_suffix(&format!(".{EXTENSION}")))
        .ok_or_else(|| {
            CommandError::invalid_args(format!("`{id}` is not a board id (`{BOARDS_DIR}/<name>.json`)"))
        })?;
    Ok(BoardFile::new(name)?)
}

/// One project's board files.
pub(super) struct Files<'a> {
    /// Reads, for every project.
    pub(super) dirs: BoardDirs,
    workspace: Option<&'a Workspace>,
}

impl<'a> Files<'a> {
    /// The board files of the project in folder `root`, read-only (a resting project).
    pub(super) fn reading(root: &Path) -> Self {
        Self {
            dirs: BoardDirs::new(root),
            workspace: None,
        }
    }

    /// The board files of live project `workspace`, which may also be written.
    pub(super) fn live(workspace: &'a Workspace) -> Self {
        Self {
            dirs: BoardDirs::new(workspace.root()),
            workspace: Some(workspace),
        }
    }

    fn workspace(&self) -> CommandResult<&'a Workspace> {
        self.workspace
            .ok_or_else(|| CommandError::internal("board files are written only in a live project"))
    }

    /// Notes that `file` is about to hold `text` (`None`: be deleted), so the watcher doesn't
    /// report the app's own write.
    fn note(&self, file: &BoardFile, text: Option<&str>) -> CommandResult<()> {
        self.workspace()?
            .note_own_write(&self.dirs.path(file), text.map(str::as_bytes));
        Ok(())
    }

    /// Whether `file` exists (as anything).
    fn exists(&self, file: &BoardFile) -> bool {
        std::fs::symlink_metadata(self.dirs.path(file)).is_ok()
    }

    /// Reads and parses a board file; `not_found` when it doesn't exist.
    pub(super) fn load(&self, file: &BoardFile) -> CommandResult<Loaded> {
        self.dirs.load(file).map_err(|error| match error {
            BoardError::Io { .. } if !self.exists(file) => {
                CommandError::not_found(format!("board `{}` does not exist", file.relative()))
            }
            other => other.into(),
        })
    }

    /// Every board of the project, by name.
    pub(super) fn boards(&self) -> CommandResult<Vec<BoardFile>> {
        Ok(self.dirs.list()?)
    }

    /// When `file` was last edited: its modification time, in milliseconds since the Unix epoch
    /// (0 when unknown).
    pub(super) fn modified_ms(&self, file: &BoardFile) -> i64 {
        std::fs::metadata(self.dirs.path(file))
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
    }

    /// Writes `board` over board `file`, whatever the file holds now, and returns the new
    /// revision.
    pub(super) fn save(&self, file: &BoardFile, board: &Board) -> CommandResult<Revision> {
        let text = serialize(board);
        self.note(file, Some(&text))?;
        Ok(self.dirs.save(file, &text)?)
    }

    /// Creates a new board file for `board`, named from its title (`order-placement.json`, or
    /// `order-placement-2.json` when that is taken), never overwriting one.
    pub(super) fn create(&self, board: &Board) -> CommandResult<(BoardFile, Revision)> {
        let file = BoardFile::new(&self.dirs.free_name(&slug(&board.title)))?;
        let text = serialize(board);
        self.note(&file, Some(&text))?;
        let revision = self.dirs.create(&file, &text)?;
        Ok((file, revision))
    }

    /// Deletes a board file; one already gone is fine.
    pub(super) fn remove(&self, file: &BoardFile) -> CommandResult<()> {
        self.note(file, None)?;
        Ok(self.dirs.remove(file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_relative_paths() {
        let file = parse_id(".unfold/boards/order-placement.json").expect("a board id");
        assert_eq!(file.name(), "order-placement");
        assert_eq!(file.relative(), ".unfold/boards/order-placement.json");
        for bad in [
            "order-placement",
            ".unfold/boards/../x.json",
            ".unfold/boards/a/b.json",
            ".unfold/boards/x.txt",
            ".unfold/local/x.json",
            "/etc/passwd",
        ] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
    }
}
