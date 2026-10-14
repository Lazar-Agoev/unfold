//! Board files of a project (docs/architecture.md#storage): one file per board in
//! `.unfold/boards/`, written atomically as it is edited, created without ever overwriting
//! another board, never larger than a board file can be, with change detection for edits made
//! outside the app.
//!
//! These are the only functions here that touch the disk; parsing, checking, and serializing
//! are pure ([`super::parse`], [`super::health`], [`super::serialize`]).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use unfold_core::ContentHash;
use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

use super::BoardError;
use super::model::Board;

/// The boards folder, relative to the project's folder.
pub const BOARDS_DIR: &str = ".unfold/boards";
/// Every board file's extension.
pub const EXTENSION: &str = "json";
/// The largest board file read. Boards hold references and short texts, so a bigger file is
/// not a board, and reading it whole would only waste memory.
pub const MAX_BOARD_BYTES: u64 = 16 * 1024 * 1024;
/// The longest file stem.
pub(super) const MAX_NAME: usize = 100;

/// A board file, by its name (the file stem in [`BOARDS_DIR`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoardFile {
    name: String,
}

impl BoardFile {
    /// The board file named `name`, a plain stem ([`BoardError::InvalidName`] otherwise).
    pub fn new(name: &str) -> Result<Self, BoardError> {
        check_name(name)?;
        Ok(Self {
            name: name.to_owned(),
        })
    }

    /// The file stem.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The path relative to the project's folder, with `/` separators: the board's id.
    pub fn relative(&self) -> String {
        format!("{BOARDS_DIR}/{}.{EXTENSION}", self.name)
    }

    fn file_name(&self) -> String {
        format!("{}.{EXTENSION}", self.name)
    }
}

/// A file name must be a plain stem: letters, digits, `.`, `_`, `-`, starting with a letter or
/// digit, at most 100 characters. Paths, separators, and `..` are refused.
fn check_name(name: &str) -> Result<(), BoardError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(BoardError::InvalidName {
            name: name.to_owned(),
        })
    }
}

/// The file name for a title: `Order placement` → `order-placement`. Accents are dropped
/// (`Café` → `cafe`), every other run of non-alphanumeric characters becomes one `-`, and the
/// result has at most 80 characters; a title with nothing left becomes `board`.
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
    } else {
        slug.to_owned()
    }
}

/// What a file's bytes were when the app last loaded or saved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

/// What a file is now, compared with a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The board files of one project, under its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardDirs {
    root: PathBuf,
}

impl BoardDirs {
    /// The board files of the project in folder `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// The file on disk, unchecked: use it for messages and metadata (a board's last edit), and
    /// the other methods to read or write it.
    pub fn path(&self, file: &BoardFile) -> PathBuf {
        self.root.join(file.relative())
    }

    /// The boards directory once `.unfold/` and `boards/` are checked to be real directories,
    /// not links that could lead outside the folder; `None` when one is missing.
    fn directory(&self) -> Result<Option<PathBuf>, BoardError> {
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

    /// The file's checked path; `None` when the boards directory does not exist, so neither does
    /// the file.
    fn resolve(&self, file: &BoardFile) -> Result<Option<PathBuf>, BoardError> {
        Ok(self
            .directory()?
            .map(|directory| directory.join(file.file_name())))
    }

    /// Creates the boards directory (real directories only) and returns it.
    fn prepare(&self) -> Result<PathBuf, BoardError> {
        let mut path = self.root.clone();
        for part in [".unfold", "boards"] {
            path.push(part);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(&path, &error)),
            }
            real_directory(&path)?;
        }
        Ok(path)
    }

    /// Reads and parses a board file.
    pub fn load(&self, file: &BoardFile) -> Result<Loaded, BoardError> {
        let path = self.resolve(file)?.ok_or_else(|| not_found(&self.path(file)))?;
        let bytes = read_regular(&path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| BoardError::Syntax {
            line: 0,
            column: 0,
            message: "the file is not UTF-8".to_owned(),
        })?;
        Ok(Loaded {
            board: super::parse(text)?,
            revision: Revision::of(&bytes),
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

    /// Writes `text` (a serialized board) to board `file`, replacing whatever the file holds
    /// atomically (creating it when it is gone), and returns the new revision. A board too large
    /// to read again is refused ([`BoardError::TooLarge`]).
    pub fn save(&self, file: &BoardFile, text: &str) -> Result<Revision, BoardError> {
        fits(text)?;
        replace(&self.prepare()?.join(file.file_name()), text)
    }

    /// Creates board `file` with `text`, never overwriting: if a file has that name, fails with
    /// [`BoardError::NameTaken`] carrying the path and a free alternative. A board too large to
    /// read again is refused ([`BoardError::TooLarge`]).
    pub fn create(&self, file: &BoardFile, text: &str) -> Result<Revision, BoardError> {
        fits(text)?;
        let path = self.prepare()?.join(file.file_name());
        if create_atomic(&path, text.as_bytes())? {
            Ok(Revision::of(text.as_bytes()))
        } else {
            Err(BoardError::NameTaken {
                path: file.relative(),
                suggestion: self.free_name(&file.name),
            })
        }
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

    /// The stems of the files the boards directory lists ([`listed_stem`]), sorted. A missing
    /// directory lists nothing.
    fn stems(&self) -> Result<Vec<String>, BoardError> {
        let Some(directory) = self.directory()? else {
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
            if let Some(stem) = entry.file_name().to_str().and_then(listed_stem) {
                stems.push(stem.to_owned());
            }
        }
        stems.sort();
        Ok(stems)
    }

    /// The board files with valid names, sorted by name. A missing directory lists nothing.
    pub fn list(&self) -> Result<Vec<BoardFile>, BoardError> {
        Ok(self
            .stems()?
            .iter()
            .filter_map(|stem| BoardFile::new(stem).ok())
            .collect())
    }

    /// The listed files whose names a board file can't have (a Finder copy, `Order flow
    /// copy.json`), sorted: each one's stem and why ([`BoardError::InvalidName`]).
    pub fn misnamed(&self) -> Result<Vec<(String, BoardError)>, BoardError> {
        Ok(self
            .stems()?
            .into_iter()
            .filter_map(|stem| BoardFile::new(&stem).err().map(|error| (stem, error)))
            .collect())
    }

    /// The first name from `name`, `name-2`, `name-3`, … that no file has; the base is shortened
    /// when a suffix would make the name too long. After 10,000 taken names, `name` itself
    /// (which the next create then refuses again).
    pub fn free_name(&self, name: &str) -> String {
        let free = |candidate: &str| {
            BoardFile::new(candidate).is_ok_and(|file| fs::symlink_metadata(self.path(&file)).is_err())
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

/// The stem of `name`, a file name in the boards directory, when lists show the file: a `.json`
/// file that isn't hidden (a temporary file being written, or macOS's `._` files). Its stem may
/// still be a name a board file can't have ([`BoardDirs::misnamed`]).
pub fn listed_stem(name: &str) -> Option<&str> {
    name.strip_suffix(".json")
        .filter(|stem| !stem.is_empty() && !stem.starts_with('.') && !stem.contains('/'))
}

/// Refuses a serialized board larger than a board file can be: it couldn't be read again.
fn fits(text: &str) -> Result<(), BoardError> {
    let bytes = text.len() as u64;
    if bytes > MAX_BOARD_BYTES {
        Err(BoardError::TooLarge { bytes })
    } else {
        Ok(())
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

/// Creates `target` with `bytes`, never replacing an existing file; whether it did (`false`:
/// something has that name). The name is claimed by creating the target empty and exclusively,
/// which fails if anything is there (a dangling link included), and then the synced temporary
/// file is renamed over it. Never a hard link: FSEvents doesn't report the deletion of a file
/// that began as one, so a board deleted before its first save would go unnoticed.
fn create_atomic(target: &Path, bytes: &[u8]) -> Result<bool, BoardError> {
    let temporary = write_temporary(target, bytes)?;
    let claimed = OpenOptions::new().write(true).create_new(true).open(target);
    let result = match claimed {
        Ok(_) => fs::rename(&temporary, target).map(|()| true).map_err(|error| {
            // The empty claim is the app's own: it goes with the create that failed.
            let _ = fs::remove_file(target);
            io_error(target, &error)
        }),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(io_error(target, &error)),
    };
    if result.as_ref().is_ok_and(|created| *created) {
        sync_directory(target);
    } else {
        let _ = fs::remove_file(&temporary);
    }
    result
}
