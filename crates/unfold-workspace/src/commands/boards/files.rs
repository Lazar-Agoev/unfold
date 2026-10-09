//! A project's board files by board id (contract §1.3, §6.1, §6.6): which file a board's
//! working version is in, reading it, and writing it the way the table of §6.6 says.
//!
//! A board's **id** is its public file's folder-relative path: `.unfold/local/drafts/<name>.json`
//! for a draft (the same id when `local/` is in the fallback location) and
//! `.unfold/boards/<name>.json` for a published board, with or without unpublished changes. A
//! published board's **working file** is its changes copy (`drafts/<name>.changes.json`) when one
//! exists, else the published file itself.
//!
//! Every write goes through a live project's [`Workspace`], which notes it so the watcher doesn't
//! report the app's own writes as outside edits; commands report them with `boards_changed`
//! themselves.

use std::time::UNIX_EPOCH;

use unfold_reader::board::files::{
    BOARDS_DIR, BoardDirs, BoardFile, DRAFTS_DIR, EXTENSION, Loaded, Place, Revision,
};
use unfold_reader::board::{Board, BoardError, changes, serialize, serialize_changes};

use super::types::PublishedState;
use crate::error::WorkspaceError;
use crate::events::WorkingFile;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::Layout;
use crate::workspace::Workspace;

/// The board file an id names.
pub(super) fn parse_id(id: &str) -> CommandResult<BoardFile> {
    let stem = |prefix: &str| {
        id.strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|rest| rest.strip_suffix(&format!(".{EXTENSION}")))
    };
    let file = if let Some(name) = stem(BOARDS_DIR) {
        BoardFile::published(name)
    } else if let Some(name) = stem(DRAFTS_DIR) {
        BoardFile::draft(name)
    } else {
        return Err(CommandError::invalid_args(format!(
            "`{id}` is not a board id (`{BOARDS_DIR}/<name>.json` or `{DRAFTS_DIR}/<name>.json`)"
        )));
    };
    Ok(file?)
}

/// The file board `file`'s working version is in when it is `working`.
pub(super) fn working_file(file: &BoardFile, working: WorkingFile) -> BoardFile {
    match working {
        WorkingFile::Changes => BoardFile::changes_of(file).unwrap_or_else(|| file.clone()),
        WorkingFile::Draft | WorkingFile::Published => file.clone(),
    }
}

/// A board's working version as read from disk.
#[derive(Debug, Clone)]
pub(super) struct Working {
    /// Which file it is in.
    pub(super) kind: WorkingFile,
    /// The board, its file's revision, and (for a changes copy) the published revision it
    /// started from.
    pub(super) loaded: Loaded,
}

impl Working {
    /// The published revision a write of this working version starts a changes copy from: the
    /// copy's own base, or the published revision read (`None` for a draft).
    pub(super) fn base(&self) -> Option<Revision> {
        match self.kind {
            WorkingFile::Changes => self.loaded.base,
            WorkingFile::Published => Some(self.loaded.revision),
            WorkingFile::Draft => None,
        }
    }
}

/// A working version just written.
#[derive(Debug, Clone)]
pub(super) struct Written {
    /// Which file it is in now.
    pub(super) kind: WorkingFile,
    /// The board as the file holds it: the version written, or the published one when the
    /// working version became equal to it.
    pub(super) board: Board,
    /// The file's revision.
    pub(super) revision: Revision,
    /// For a changes copy, the published revision it started from.
    pub(super) base: Option<Revision>,
    /// For a published board, its publish state now.
    pub(super) published: Option<PublishedState>,
}

/// One project's board files.
pub(super) struct Files<'a> {
    /// Reads, for every project.
    pub(super) dirs: BoardDirs,
    workspace: Option<&'a Workspace>,
}

impl<'a> Files<'a> {
    /// The board files of a project laid out as `layout` says, read-only (a resting project).
    pub(super) fn reading(root: &std::path::Path, layout: &Layout) -> Self {
        Self {
            dirs: BoardDirs::new(root, &layout.drafts()),
            workspace: None,
        }
    }

    /// The board files of live project `workspace`, which may also be written.
    pub(super) fn live(workspace: &'a Workspace) -> Self {
        Self {
            dirs: BoardDirs::new(workspace.root(), &workspace.layout().drafts()),
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
    pub(super) fn exists(&self, file: &BoardFile) -> bool {
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

    /// Board `file`'s working version (its id's file: a draft or a published board).
    pub(super) fn working(&self, file: &BoardFile) -> CommandResult<Working> {
        if let Some(copy) = BoardFile::changes_of(file)
            && self.exists(&copy)
        {
            return Ok(Working {
                kind: WorkingFile::Changes,
                loaded: self.load(&copy)?,
            });
        }
        let kind = match file.place {
            Place::Published => WorkingFile::Published,
            Place::Draft => WorkingFile::Draft,
        };
        Ok(Working {
            kind,
            loaded: self.load(file)?,
        })
    }

    /// Every board of the project by its own file: published boards (those with only a changes
    /// copy left included), then drafts, each by name.
    pub(super) fn boards(&self) -> CommandResult<Vec<BoardFile>> {
        let mut published = self.dirs.list(Place::Published)?;
        published.extend(self.dirs.changed()?);
        published.sort_by(|a, b| a.name.cmp(&b.name));
        published.dedup();
        published.extend(self.dirs.list(Place::Draft)?);
        Ok(published)
    }

    /// When `file` was last written, in milliseconds since the Unix epoch (0 when unknown).
    pub(super) fn modified_ms(&self, file: &BoardFile) -> i64 {
        std::fs::metadata(self.dirs.path(file))
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
    }

    /// The publish state of published board `file` whose working version is `working`, in
    /// `kind` (with base `base` for a changes copy). `None` for a draft.
    pub(super) fn published_state(
        &self,
        file: &BoardFile,
        working: &Board,
        kind: WorkingFile,
        base: Option<Revision>,
    ) -> Option<PublishedState> {
        if file.place != Place::Published {
            return None;
        }
        let published = self.dirs.load(file).ok();
        let changed = kind == WorkingFile::Changes;
        Some(PublishedState {
            revision: published
                .as_ref()
                .map(|published| published.revision.0.to_string()),
            changes: if changed {
                unpublished(working, published.as_ref())
            } else {
                0
            },
            base_changed: changed && published.map(|published| published.revision) != base,
        })
    }

    /// Fails with `not_compiled` until the first Compile created the drafts' `local/` folder.
    fn drafts_writable(&self) -> CommandResult<()> {
        if self.workspace()?.layout().is_compiled() {
            Ok(())
        } else {
            Err(WorkspaceError::NotCompiled.into())
        }
    }

    /// Writes a draft or a changes copy.
    pub(super) fn save_draft(&self, file: &BoardFile, text: &str) -> CommandResult<Revision> {
        self.drafts_writable()?;
        self.note(file, Some(text))?;
        Ok(self.dirs.save_draft(file, text)?)
    }

    /// Creates published board `name`, never overwriting one.
    pub(super) fn create_published(
        &self,
        name: &str,
        text: &str,
    ) -> Result<(BoardFile, Revision), BoardError> {
        if let (Ok(file), Some(workspace)) = (BoardFile::published(name), self.workspace) {
            workspace.note_own_write(&self.dirs.path(&file), Some(text.as_bytes()));
        }
        self.dirs.create_published(name, text)
    }

    /// Replaces (or recreates) published board `file`.
    pub(super) fn overwrite_published(&self, file: &BoardFile, text: &str) -> CommandResult<Revision> {
        self.note(file, Some(text))?;
        Ok(self.dirs.overwrite_published(file, text)?)
    }

    /// Deletes a board file; one already gone is fine.
    pub(super) fn remove(&self, file: &BoardFile) -> CommandResult<()> {
        self.note(file, None)?;
        Ok(self.dirs.remove(file)?)
    }

    /// A draft name no draft has: `board-<ms>`, then `-2`, `-3`, … (decision D-6).
    pub(super) fn new_draft(&self) -> CommandResult<BoardFile> {
        Ok(self
            .dirs
            .free_draft(&format!("board-{}", crate::commit::now_ms()))?)
    }

    /// Writes `board` as board `file`'s working version (contract §6.6): the draft; or for a
    /// published board its changes copy, started from published revision `base` when the copy
    /// is new, and deleted (the board back on its published file) when the working version
    /// equals the published one.
    pub(super) fn write(
        &self,
        file: &BoardFile,
        board: Board,
        base: Option<Revision>,
    ) -> CommandResult<Written> {
        let Some(copy) = BoardFile::changes_of(file) else {
            let revision = self.save_draft(file, &serialize(&board))?;
            return Ok(Written {
                kind: WorkingFile::Draft,
                board,
                revision,
                base: None,
                published: None,
            });
        };
        let published = self.dirs.load(file).ok();
        let count = unpublished(&board, published.as_ref());
        if count == 0
            && let Some(published) = published
        {
            self.remove(&copy)?;
            return Ok(Written {
                kind: WorkingFile::Published,
                published: Some(PublishedState::current(published.revision)),
                board: published.board,
                revision: published.revision,
                base: None,
            });
        }
        let now = published.as_ref().map(|published| published.revision);
        let base = base.or(now).ok_or_else(|| {
            CommandError::internal("a changes copy needs the published revision it starts from")
        })?;
        let revision = self.save_draft(&copy, &serialize_changes(&board, &file.name, base))?;
        Ok(Written {
            kind: WorkingFile::Changes,
            board,
            revision,
            base: Some(base),
            published: Some(PublishedState {
                revision: now.map(|revision| revision.0.to_string()),
                changes: count,
                base_changed: now != Some(base),
            }),
        })
    }
}

/// The unpublished changes of `working` over the published version, or for a published file
/// deleted on disk over an empty board (every part of the working version is a change).
fn unpublished(working: &Board, published: Option<&Loaded>) -> u32 {
    match published {
        Some(published) => changes(working, &published.board),
        None => changes(working, &Board::with_pages("", None, Vec::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_relative_paths() {
        let published = parse_id(".unfold/boards/order-placement.json").expect("published");
        assert_eq!(published.place, Place::Published);
        assert_eq!(published.relative(), ".unfold/boards/order-placement.json");
        let draft = parse_id(".unfold/local/drafts/draft-1.json").expect("draft");
        assert_eq!(draft.place, Place::Draft);
        assert_eq!(
            working_file(&published, WorkingFile::Changes).relative(),
            ".unfold/local/drafts/order-placement.changes.json"
        );
        for bad in [
            "order-placement",
            ".unfold/boards/../x.json",
            ".unfold/boards/a/b.json",
            ".unfold/boards/x.txt",
            ".unfold/local/drafts/x.changes.json",
            "/etc/passwd",
        ] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
    }
}
