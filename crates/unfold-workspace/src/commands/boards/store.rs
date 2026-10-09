//! Board files on disk for one folder, by board ID, wherever `local/` lives.
//!
//! A board's **ID** is its folder-relative file path: `.unfold/boards/<name>.json` for a
//! published board and `.unfold/local/drafts/<name>.json` for a draft. The draft ID stays the
//! same when `local/` is in the fallback location (outside the folder); this module maps it to
//! the real file. Published files go through `unfold_reader::board::files` (atomic, never
//! overwriting on Publish or Rename); drafts are written the same way into the layout's
//! `drafts/` folder.
//!
//! Writes are deliberately **not** hidden from the watcher: an outside edit can land within
//! moments of the app's own save, and conflicts are decided by comparing revisions (the app's
//! own save leaves the file at the session's revision), never by who wrote last.

use std::path::PathBuf;

use unfold_reader::board::BoardError;
use unfold_reader::board::files::{
    self, BOARDS_DIR, BoardFile, Change, DRAFTS_DIR, EXTENSION, Loaded, Place, Revision,
};

use crate::ipc::error::{CommandError, CommandResult};
use crate::workspace::Workspace;

/// The ID of a board file.
pub fn board_id(file: &BoardFile) -> String {
    file.relative()
}

/// The board file an ID names.
pub fn parse_id(id: &str) -> CommandResult<BoardFile> {
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
            "`{id}` is not a board ID (`{BOARDS_DIR}/<name>.json` or `{DRAFTS_DIR}/<name>.json`)"
        )));
    };
    Ok(file?)
}

/// One folder's board files.
pub struct BoardStore<'a> {
    workspace: &'a Workspace,
    drafts: PathBuf,
}

impl<'a> BoardStore<'a> {
    /// The board files of `workspace`.
    pub fn new(workspace: &'a Workspace) -> Self {
        Self {
            drafts: workspace.layout().drafts(),
            workspace,
        }
    }

    /// The file on disk, unchecked: published files are touched only through
    /// `unfold_reader::board::files`, which refuses links on the way.
    pub fn path(&self, file: &BoardFile) -> PathBuf {
        match file.place {
            Place::Published => file.path(self.workspace.root()),
            Place::Draft => self.drafts.join(format!("{}.{EXTENSION}", file.name)),
        }
    }

    /// Reads and parses a board.
    pub fn load(&self, file: &BoardFile) -> CommandResult<Loaded> {
        let loaded = match file.place {
            Place::Published => files::load(self.workspace.root(), file),
            // The layout never puts drafts behind a link (see `Layout`).
            Place::Draft => files::load_path(&self.path(file)),
        };
        loaded.map_err(|error| match error {
            BoardError::Io { .. } if std::fs::symlink_metadata(self.path(file)).is_err() => {
                CommandError::not_found(format!("board `{}` does not exist", board_id(file)))
            }
            other => other.into(),
        })
    }

    /// Writes `text` over the board's file atomically and returns the new revision.
    pub fn save(&self, file: &BoardFile, text: &str) -> CommandResult<Revision> {
        let path = self.path(file);
        match file.place {
            Place::Published => Ok(files::save(self.workspace.root(), file, text)?),
            Place::Draft => {
                if !self.workspace.layout().is_compiled() {
                    return Err(crate::error::WorkspaceError::NotCompiled.into());
                }
                std::fs::create_dir_all(&self.drafts).map_err(|error| {
                    CommandError::new("io", format!("{}: {error}", self.drafts.display()))
                })?;
                files::write_atomic(&path, text.as_bytes())?;
                Ok(Revision::of(text.as_bytes()))
            }
        }
    }

    /// Creates published board `name` with `text`, never overwriting.
    pub fn create_published(&self, name: &str, text: &str) -> Result<(BoardFile, Revision), BoardError> {
        files::create_published(self.workspace.root(), name, text)
    }

    /// Renames a published board's file, never overwriting.
    pub fn rename_published(&self, from: &BoardFile, name: &str) -> Result<BoardFile, BoardError> {
        files::rename(self.workspace.root(), from, name)
    }

    /// Deletes a board's file; a file already gone is fine.
    pub fn remove(&self, file: &BoardFile) -> CommandResult<()> {
        if file.place == Place::Published {
            return Ok(files::remove(self.workspace.root(), file)?);
        }
        let path = self.path(file);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(CommandError::new("io", format!("{}: {error}", path.display()))),
        }
    }

    /// Whether the file still has `revision`.
    pub fn change(&self, file: &BoardFile, revision: Revision) -> CommandResult<Change> {
        Ok(match file.place {
            Place::Published => files::change(self.workspace.root(), file, revision)?,
            Place::Draft => files::change_path(&self.path(file), revision)?,
        })
    }

    /// The board files in one place, sorted by name.
    pub fn list(&self, place: Place) -> CommandResult<Vec<BoardFile>> {
        if place == Place::Published {
            return Ok(files::list(self.workspace.root(), place)?);
        }
        let entries = match std::fs::read_dir(&self.drafts) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(CommandError::new(
                    "io",
                    format!("{}: {error}", self.drafts.display()),
                ));
            }
        };
        let mut found: Vec<BoardFile> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let stem = name.to_str()?.strip_suffix(".json")?;
                BoardFile::draft(stem).ok()
            })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// A draft name that no draft has yet: `base`, then `base-2`, `base-3`, …
    pub fn free_draft(&self, base: &str) -> CommandResult<BoardFile> {
        for number in 1..10_000u32 {
            let name = if number == 1 {
                base.to_owned()
            } else {
                format!("{base}-{number}")
            };
            let file = BoardFile::draft(&name)?;
            if std::fs::symlink_metadata(self.path(&file)).is_err() {
                return Ok(file);
            }
        }
        Err(CommandError::internal("no free draft name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_relative_paths() {
        let published = parse_id(".unfold/boards/order-placement.json").expect("published");
        assert_eq!(published.place, Place::Published);
        assert_eq!(board_id(&published), ".unfold/boards/order-placement.json");
        let draft = parse_id(".unfold/local/drafts/draft-1.json").expect("draft");
        assert_eq!(draft.place, Place::Draft);
        for bad in [
            "order-placement",
            ".unfold/boards/../x.json",
            ".unfold/boards/a/b.json",
            ".unfold/boards/x.txt",
            "/etc/passwd",
        ] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
    }
}
