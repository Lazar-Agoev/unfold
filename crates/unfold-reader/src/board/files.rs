//! Board files of a project (docs/architecture.md#storage; unpublished edits): published boards
//! in `.unfold/boards/`, drafts and the changes copies of published boards in the drafts
//! directory (`.unfold/local/drafts/`, or its fallback location outside the folder), written
//! atomically, never overwriting a published file by accident, with change detection for edits
//! made outside the app.
//!
//! | File | Path | Holds |
//! | --- | --- | --- |
//! | Draft | `<drafts>/<name>.json` | A board |
//! | Published | `.unfold/boards/<name>.json` | A board |
//! | Changes copy | `<drafts>/<name>.changes.json` | The working version of published board `<name>`, with `publishes` and `baseRevision` |
//!
//! These are the only functions here that touch the disk; parsing, checking, and serializing
//! are pure ([`super::parse`], [`super::health`], [`super::serialize`]).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use unfold_core::ContentHash;
use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

use super::BoardError;
use super::model::Board;

/// Published boards, relative to the folder.
pub const BOARDS_DIR: &str = ".unfold/boards";
/// Drafts, relative to the folder (when `local/` is in the folder; board ids use it either way).
pub const DRAFTS_DIR: &str = ".unfold/local/drafts";
/// Every board file's extension.
pub const EXTENSION: &str = "json";
/// What a changes copy's stem adds to its published board's name. No draft name ends in it.
const CHANGES_SUFFIX: &str = ".changes";
/// The largest board file read. Boards hold references and short texts, so a bigger file is
/// not a board, and reading it whole would only waste memory.
pub const MAX_BOARD_BYTES: u64 = 16 * 1024 * 1024;
/// The longest file stem.
const MAX_NAME: usize = 100;

/// Where a board file lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// Committed with the code.
    Published,
    /// On this machine only: a draft, or a published board's changes copy.
    Draft,
}

impl Place {
    /// The place's directory, relative to the folder.
    const fn directory(self) -> &'static str {
        match self {
            Self::Published => BOARDS_DIR,
            Self::Draft => DRAFTS_DIR,
        }
    }
}

/// A board file: its place and its name (the file stem: a slug for a published board, an id
/// for a draft, `<published name>.changes` for a changes copy).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardFile {
    /// Where it lives.
    pub place: Place,
    /// The file stem.
    pub name: String,
}

impl BoardFile {
    /// A published board.
    pub fn published(name: &str) -> Result<Self, BoardError> {
        check_name(name)?;
        Ok(Self {
            place: Place::Published,
            name: name.to_owned(),
        })
    }

    /// A draft. Its name never ends in `.changes`, which marks changes copies (unpublished edits).
    pub fn draft(name: &str) -> Result<Self, BoardError> {
        check_name(name)?;
        if name.ends_with(CHANGES_SUFFIX) {
            return Err(BoardError::InvalidName {
                name: name.to_owned(),
            });
        }
        Ok(Self {
            place: Place::Draft,
            name: name.to_owned(),
        })
    }

    /// The changes copy of `published` (`None` when it is not a published board).
    pub fn changes_of(published: &Self) -> Option<Self> {
        (published.place == Place::Published).then(|| Self {
            place: Place::Draft,
            name: format!("{}{CHANGES_SUFFIX}", published.name),
        })
    }

    /// For a changes copy, the published board it holds changes for.
    pub fn publishes(&self) -> Option<Self> {
        if self.place != Place::Draft {
            return None;
        }
        let name = self.name.strip_suffix(CHANGES_SUFFIX)?;
        Self::published(name).ok()
    }

    /// The path relative to the folder, with `/` separators: the board's id for a draft or a
    /// published board.
    pub fn relative(&self) -> String {
        format!("{}/{}.{EXTENSION}", self.place.directory(), self.name)
    }

    fn file_name(&self) -> String {
        format!("{}.{EXTENSION}", self.name)
    }
}

/// Stems Windows reserves for devices (`con.json` cannot be created there), compared without
/// case on the part before the first `.`.
fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// A file name must be a plain stem: letters, digits, `.`, `_`, `-`, starting with a letter or
/// digit, at most 100 characters, and not a name Windows reserves. Paths, separators, and `..`
/// are refused.
fn check_name(name: &str) -> Result<(), BoardError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
        && !is_reserved(name);
    if valid {
        Ok(())
    } else {
        Err(BoardError::InvalidName {
            name: name.to_owned(),
        })
    }
}

/// The published file name for a title: `Order placement` → `order-placement`. Accents are
/// dropped (`Café` → `cafe`), every other run of non-alphanumeric characters becomes one `-`,
/// and the result has at most 80 characters; a title with nothing left becomes `board`, and a
/// name Windows reserves gets `-board` (`con-board`).
pub fn slug(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in title.nfkd().filter(|&character| !is_combining_mark(character)) {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(lower);
        } else {
            pending_dash = true;
        }
    }
    slug.truncate(80);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "board".to_owned()
    } else if is_reserved(slug) {
        format!("{slug}-board")
    } else {
        slug.to_owned()
    }
}

/// What a file's bytes were when the app last loaded or saved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
#[serde(transparent)]
pub struct Revision(pub ContentHash);

impl Revision {
    /// The revision of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(ContentHash::of(bytes))
    }
}

/// A board read from disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The board.
    pub board: Board,
    /// The bytes' revision.
    pub revision: Revision,
    /// For a changes copy, the published revision it started from (`baseRevision`).
    pub base: Option<Revision>,
}

/// What a file is now, compared with a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", content = "revision", rename_all = "camelCase")]
pub enum Change {
    /// The bytes are the ones last loaded or saved.
    Unchanged,
    /// Something else wrote the file.
    Changed(Revision),
    /// The file is gone.
    Missing,
}

/// Refuses a directory that is a symbolic link or not a directory, so board paths cannot
/// escape where they belong.
fn real_directory(path: &Path) -> Result<(), BoardError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(BoardError::Io {
            path: path.display().to_string(),
            message: "must be a real directory, not a link or a file".to_owned(),
        }),
        Err(error) => Err(io_error(path, &error)),
    }
}

fn io_error(path: &Path, error: &io::Error) -> BoardError {
    BoardError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

fn not_found(path: &Path) -> BoardError {
    io_error(path, &io::Error::from(io::ErrorKind::NotFound))
}

/// The board files of one project: its folder (`root`, for published boards) and its drafts
/// directory (`drafts`, from the workspace layout: `<root>/.unfold/local/drafts` or the fallback
/// location).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardDirs {
    root: PathBuf,
    drafts: PathBuf,
}

impl BoardDirs {
    /// The board files under folder `root` with drafts in `drafts`.
    pub fn new(root: &Path, drafts: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            drafts: drafts.to_path_buf(),
        }
    }

    /// The file on disk, unchecked: use it for messages and metadata (a board's `updatedAtMs`),
    /// and the other methods to read or write it.
    pub fn path(&self, file: &BoardFile) -> PathBuf {
        match file.place {
            Place::Published => self.root.join(file.relative()),
            Place::Draft => self.drafts.join(file.file_name()),
        }
    }

    /// The published directory once `.unfold/` and `boards/` are checked to be real
    /// directories, not links that could lead outside the folder; `None` when one is missing.
    fn published_directory(&self) -> Result<Option<PathBuf>, BoardError> {
        let mut path = self.root.clone();
        for part in [".unfold", "boards"] {
            path.push(part);
            match fs::symlink_metadata(&path) {
                Ok(_) => real_directory(&path)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(io_error(&path, &error)),
            }
        }
        Ok(Some(path))
    }

    /// The drafts directory once checked to be a real directory; `None` when it is missing.
    fn drafts_directory(&self) -> Result<Option<PathBuf>, BoardError> {
        match fs::symlink_metadata(&self.drafts) {
            Ok(_) => real_directory(&self.drafts).map(|()| Some(self.drafts.clone())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(&self.drafts, &error)),
        }
    }

    fn directory(&self, place: Place) -> Result<Option<PathBuf>, BoardError> {
        match place {
            Place::Published => self.published_directory(),
            Place::Draft => self.drafts_directory(),
        }
    }

    /// The file's checked path; `None` when its directory does not exist, so neither does it.
    fn resolve(&self, file: &BoardFile) -> Result<Option<PathBuf>, BoardError> {
        Ok(self
            .directory(file.place)?
            .map(|directory| directory.join(file.file_name())))
    }

    /// Creates the place's directory (real directories only) and returns it.
    fn prepare(&self, place: Place) -> Result<PathBuf, BoardError> {
        let create = |path: &Path| match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(io_error(path, &error)),
        };
        match place {
            Place::Published => {
                let mut path = self.root.clone();
                for part in [".unfold", "boards"] {
                    path.push(part);
                    create(&path)?;
                    real_directory(&path)?;
                }
                Ok(path)
            }
            Place::Draft => {
                fs::create_dir_all(&self.drafts).map_err(|error| io_error(&self.drafts, &error))?;
                real_directory(&self.drafts)?;
                Ok(self.drafts.clone())
            }
        }
    }

    /// Reads and parses a board file: a draft, a published board, or a changes copy (whose
    /// `publishes` must name its published board, and whose `baseRevision` becomes
    /// [`Loaded::base`]).
    pub fn load(&self, file: &BoardFile) -> Result<Loaded, BoardError> {
        let path = self.resolve(file)?.ok_or_else(|| not_found(&self.path(file)))?;
        let bytes = read_regular(&path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| BoardError::Syntax {
            line: 0,
            column: 0,
            message: "the file is not UTF-8".to_owned(),
        })?;
        let (board, base) = match file.publishes() {
            Some(published) => {
                let (board, base) = super::parse::parse_changes(text, &published.name)?;
                (board, Some(base))
            }
            None => (super::parse(text)?, None),
        };
        Ok(Loaded {
            board,
            revision: Revision::of(&bytes),
            base,
        })
    }

    /// Whether the file still has `revision`.
    pub fn change(&self, file: &BoardFile, revision: Revision) -> Result<Change, BoardError> {
        let Some(path) = self.resolve(file)? else {
            return Ok(Change::Missing);
        };
        if fs::symlink_metadata(&path).is_err_and(|error| error.kind() == io::ErrorKind::NotFound) {
            return Ok(Change::Missing);
        }
        let now = Revision::of(&read_regular(&path)?);
        Ok(if now == revision {
            Change::Unchanged
        } else {
            Change::Changed(now)
        })
    }

    /// Saves `text` (a serialized board, or for a changes copy [`super::serialize_changes`]) to
    /// a draft or a changes copy, replacing it atomically, and returns the new revision.
    /// Published files are written only by [`BoardDirs::create_published`] and
    /// [`BoardDirs::overwrite_published`].
    pub fn save_draft(&self, file: &BoardFile, text: &str) -> Result<Revision, BoardError> {
        if file.place != Place::Draft {
            return Err(BoardError::Io {
                path: file.relative(),
                message: "a published board is written only by Publish".to_owned(),
            });
        }
        let path = self.prepare(Place::Draft)?.join(file.file_name());
        replace(&path, text)
    }

    /// Creates published board `name` with `text`, never overwriting: if a file has that name,
    /// fails with [`BoardError::NameTaken`] carrying the path and a free alternative, so the UI
    /// can let the person edit the name (a draft's first Publish).
    pub fn create_published(&self, name: &str, text: &str) -> Result<(BoardFile, Revision), BoardError> {
        let file = BoardFile::published(name)?;
        let path = self.prepare(Place::Published)?.join(file.file_name());
        if create_atomic(&path, text.as_bytes())? {
            Ok((file, Revision::of(text.as_bytes())))
        } else {
            Err(BoardError::NameTaken {
                path: file.relative(),
                suggestion: self.free_name(name),
            })
        }
    }

    /// Replaces published board `file` with `text` atomically, creating it when it is gone
    /// (Publish of a changes copy, including "keep mine" after the published file was deleted).
    pub fn overwrite_published(&self, file: &BoardFile, text: &str) -> Result<Revision, BoardError> {
        if file.place != Place::Published {
            return Err(BoardError::Io {
                path: file.relative(),
                message: "not a published board".to_owned(),
            });
        }
        let path = self.prepare(Place::Published)?.join(file.file_name());
        replace(&path, text)
    }

    /// Deletes a board file (the app asks first). A file that is already gone is not an error.
    /// A link in the file's place is removed itself, never what it points at.
    pub fn remove(&self, file: &BoardFile) -> Result<(), BoardError> {
        let Some(path) = self.resolve(file)? else {
            return Ok(());
        };
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(&path, &error)),
        }
    }

    /// The board files with valid names in one place, sorted by name: published boards, or
    /// drafts (never changes copies; see [`BoardDirs::changed`]). Missing directories list
    /// nothing.
    pub fn list(&self, place: Place) -> Result<Vec<BoardFile>, BoardError> {
        let mut files: Vec<BoardFile> = self
            .stems(place)?
            .iter()
            .filter_map(|stem| match place {
                Place::Published => BoardFile::published(stem).ok(),
                Place::Draft => BoardFile::draft(stem).ok(),
            })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }

    /// The published boards that have a changes copy, sorted by name (whether or not their
    /// published file still exists).
    pub fn changed(&self) -> Result<Vec<BoardFile>, BoardError> {
        let mut files: Vec<BoardFile> = self
            .stems(Place::Draft)?
            .iter()
            .filter_map(|stem| {
                BoardFile {
                    place: Place::Draft,
                    name: stem.clone(),
                }
                .publishes()
            })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }

    /// The stems of the `.json` files in one place.
    fn stems(&self, place: Place) -> Result<Vec<String>, BoardError> {
        let Some(directory) = self.directory(place)? else {
            return Ok(Vec::new());
        };
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&directory, &error)),
        };
        let mut stems = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_error(&directory, &error))?;
            if let Some(stem) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
            {
                stems.push(stem.to_owned());
            }
        }
        Ok(stems)
    }

    /// A draft that does not exist yet: `base`, then `base-2`, `base-3`, … (never a changes
    /// copy's name).
    pub fn free_draft(&self, base: &str) -> Result<BoardFile, BoardError> {
        let base = base.strip_suffix(CHANGES_SUFFIX).unwrap_or(base);
        for number in 1..10_000u32 {
            let name = if number == 1 {
                base.to_owned()
            } else {
                suffixed(base, number)
            };
            let file = BoardFile::draft(&name)?;
            if fs::symlink_metadata(self.path(&file)).is_err() {
                return Ok(file);
            }
        }
        Err(BoardError::Io {
            path: self.drafts.display().to_string(),
            message: "no free draft name".to_owned(),
        })
    }

    /// The first published name from `name`, `name-2`, `name-3`, … that no file has; the base
    /// is shortened when a suffix would make the name too long. After 10,000 taken names,
    /// `name` itself (which the next create then refuses again).
    pub fn free_name(&self, name: &str) -> String {
        let free = |candidate: &str| {
            BoardFile::published(candidate).is_ok_and(|file| fs::symlink_metadata(self.path(&file)).is_err())
        };
        if free(name) {
            return name.to_owned();
        }
        (2..=10_000u32)
            .map(|number| suffixed(name, number))
            .find(|candidate| free(candidate))
            .unwrap_or_else(|| name.to_owned())
    }
}

/// `name-<number>`, with `name` shortened so the result fits a file stem.
fn suffixed(name: &str, number: u32) -> String {
    let suffix = format!("-{number}");
    let mut cut = MAX_NAME.saturating_sub(suffix.len()).min(name.len());
    while !name.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{suffix}", name[..cut].trim_end_matches(['-', '.']))
}

/// The bytes of the regular file at `path`, refusing links, directories, and files larger than
/// [`MAX_BOARD_BYTES`].
fn read_regular(path: &Path) -> Result<Vec<u8>, BoardError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(path, &error))?;
    if !metadata.is_file() {
        return Err(BoardError::Io {
            path: path.display().to_string(),
            message: "must be a regular file".to_owned(),
        });
    }
    if metadata.len() > MAX_BOARD_BYTES {
        return Err(BoardError::Io {
            path: path.display().to_string(),
            message: format!("is larger than {} MiB", MAX_BOARD_BYTES / (1024 * 1024)),
        });
    }
    fs::read(path).map_err(|error| io_error(path, &error))
}

/// Replaces the board file at `path` (never a link or a directory there) atomically.
fn replace(path: &Path, text: &str) -> Result<Revision, BoardError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(BoardError::Io {
            path: path.display().to_string(),
            message: "must be a regular file".to_owned(),
        });
    }
    write_atomic(path, text.as_bytes())?;
    Ok(Revision::of(text.as_bytes()))
}

static TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// A fresh temporary file beside `target`, created exclusively.
fn temporary(target: &Path) -> Result<(PathBuf, File), BoardError> {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("board");
    for _ in 0..16 {
        let counter = TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let path = directory.join(format!(".{stem}.{}-{counter}-{nanos}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error(&path, &error)),
        }
    }
    Err(BoardError::Io {
        path: directory.display().to_string(),
        message: "could not create a temporary file".to_owned(),
    })
}

/// Writes `bytes` to a synced temporary file beside `target` and returns its path.
fn write_temporary(target: &Path, bytes: &[u8]) -> Result<PathBuf, BoardError> {
    let (path, mut file) = temporary(target)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(error) = written {
        let _ = fs::remove_file(&path);
        return Err(io_error(&path, &error));
    }
    Ok(path)
}

/// Makes a rename durable by syncing the directory, where the platform supports it.
fn sync_directory(path: &Path) {
    if let Some(directory) = path.parent()
        && let Ok(handle) = File::open(directory)
    {
        // Directories cannot be opened for syncing on every platform; a failure here leaves
        // the rename done but possibly not yet durable, which is no worse than not trying.
        let _ = handle.sync_all();
    }
}

/// Replaces `target` with `bytes` atomically: write a temporary file beside it, sync it, and
/// rename it over the target.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), BoardError> {
    let temporary = write_temporary(target, bytes)?;
    if let Err(error) = fs::rename(&temporary, target) {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(target, &error));
    }
    sync_directory(target);
    Ok(())
}

/// Creates `target` with `bytes` atomically, never replacing an existing file: the synced
/// temporary file is hard-linked to the target, which fails if the target exists.
fn create_atomic(target: &Path, bytes: &[u8]) -> Result<bool, BoardError> {
    let temporary = write_temporary(target, bytes)?;
    let linked = fs::hard_link(&temporary, target);
    let result = match linked {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        // A file system without hard links: fall back to a checked rename. The check looks at
        // the target itself, so a dangling link there also counts as taken.
        Err(_) if fs::symlink_metadata(target).is_err() => fs::rename(&temporary, target)
            .map(|()| true)
            .map_err(|error| io_error(target, &error)),
        Err(_) => Ok(false),
    };
    let _ = fs::remove_file(&temporary);
    if result.as_ref().is_ok_and(|created| *created) {
        sync_directory(target);
    }
    result
}
