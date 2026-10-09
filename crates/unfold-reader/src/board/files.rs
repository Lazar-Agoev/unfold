//! Board files in a workspace (architecture §8.1, §10.5): drafts in `.unfold/local/drafts/`,
//! published boards in `.unfold/boards/`, written atomically, never overwriting a published
//! file, with change detection for edits made outside the app.
//!
//! These are the only functions here that touch the disk; parsing, checking, and serializing
//! are pure ([`super::parse`], [`super::check`], [`super::serialize`]).

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
/// Drafts, relative to the folder.
pub const DRAFTS_DIR: &str = ".unfold/local/drafts";
/// Every board file's extension.
pub const EXTENSION: &str = "json";
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
    /// On this machine only.
    Draft,
}

impl Place {
    /// The directories from the folder down to the place's directory.
    const fn parts(self) -> &'static [&'static str] {
        match self {
            Self::Published => &[".unfold", "boards"],
            Self::Draft => &[".unfold", "local", "drafts"],
        }
    }

    /// The place's directory, relative to the folder.
    const fn directory(self) -> &'static str {
        match self {
            Self::Published => BOARDS_DIR,
            Self::Draft => DRAFTS_DIR,
        }
    }
}

/// A board file: its place and its name (the file stem: a slug for a published board, an id
/// for a draft).
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
        Self::new(Place::Published, name)
    }

    /// A draft.
    pub fn draft(id: &str) -> Result<Self, BoardError> {
        Self::new(Place::Draft, id)
    }

    fn new(place: Place, name: &str) -> Result<Self, BoardError> {
        check_name(name)?;
        Ok(Self {
            place,
            name: name.to_owned(),
        })
    }

    /// The path relative to the folder, with `/` separators.
    pub fn relative(&self) -> String {
        format!("{}/{}.{EXTENSION}", self.place.directory(), self.name)
    }

    /// The path under `root`, unchecked: use it for messages, and [`BoardFile::resolve`] before
    /// touching the file.
    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(self.relative())
    }

    /// The path under `root` once every directory on the way (`.unfold/`, `boards/` or
    /// `local/drafts/`) is checked to be a real directory, not a link that could lead outside the
    /// folder. `None` when one of them does not exist, so neither does the file.
    pub fn resolve(&self, root: &Path) -> Result<Option<PathBuf>, BoardError> {
        Ok(place_directory(root, self.place)?
            .map(|directory| directory.join(format!("{}.{EXTENSION}", self.name))))
    }
}

/// The directory of `place` under `root`, once every directory on the way is checked to be a
/// real directory; `None` when one of them does not exist.
fn place_directory(root: &Path, place: Place) -> Result<Option<PathBuf>, BoardError> {
    let mut path = root.to_path_buf();
    for part in place.parts() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(_) => real_directory(&path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&path, &error)),
        }
    }
    Ok(Some(path))
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
/// escape the folder.
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

/// Creates the file's directory (and `.unfold/local/.gitignore` for drafts) as real
/// directories inside `root`.
fn prepare_directory(root: &Path, file: &BoardFile) -> Result<PathBuf, BoardError> {
    let mut path = root.to_path_buf();
    for part in file.place.parts() {
        path.push(part);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(&path, &error)),
        }
        real_directory(&path)?;
        if *part == "local" {
            let ignore = path.join(".gitignore");
            if !ignore.exists() {
                write_atomic(&ignore, b"*\n")?;
            }
        }
    }
    Ok(path)
}

fn not_found(path: &Path) -> BoardError {
    io_error(path, &io::Error::from(io::ErrorKind::NotFound))
}

/// Reads and parses a board file.
pub fn load(root: &Path, file: &BoardFile) -> Result<Loaded, BoardError> {
    match file.resolve(root)? {
        Some(path) => load_path(&path),
        None => Err(not_found(&file.path(root))),
    }
}

/// Reads and parses the board file at `path`, which must be a regular file (not a link) of at
/// most [`MAX_BOARD_BYTES`]. The caller vouches for the directories on the way.
pub fn load_path(path: &Path) -> Result<Loaded, BoardError> {
    let bytes = read_regular(path)?;
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

/// Whether the file still has `revision`.
pub fn change(root: &Path, file: &BoardFile, revision: Revision) -> Result<Change, BoardError> {
    let Some(path) = file.resolve(root)? else {
        return Ok(Change::Missing);
    };
    change_path(&path, revision)
}

/// Whether the board file at `path` still has `revision`. The caller vouches for the
/// directories on the way.
pub fn change_path(path: &Path, revision: Revision) -> Result<Change, BoardError> {
    if fs::symlink_metadata(path).is_err_and(|error| error.kind() == io::ErrorKind::NotFound) {
        return Ok(Change::Missing);
    }
    let now = Revision::of(&read_regular(path)?);
    Ok(if now == revision {
        Change::Unchanged
    } else {
        Change::Changed(now)
    })
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

/// Saves `text` (a serialized board) to `file`, replacing it atomically, and returns the new
/// revision.
pub fn save(root: &Path, file: &BoardFile, text: &str) -> Result<Revision, BoardError> {
    prepare_directory(root, file)?;
    let path = file.path(root);
    if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(BoardError::Io {
            path: path.display().to_string(),
            message: "must be a regular file".to_owned(),
        });
    }
    write_atomic(&path, text.as_bytes())?;
    Ok(Revision::of(text.as_bytes()))
}

/// The first published name from `name`, `name-2`, `name-3`, … that no file has; the base is
/// shortened when a suffix would make the name too long. After 10,000 taken names, `name`
/// itself (which the next create then refuses again).
pub fn free_name(root: &Path, name: &str) -> String {
    let free = |candidate: &str| {
        BoardFile::published(candidate).is_ok_and(|file| fs::symlink_metadata(file.path(root)).is_err())
    };
    if free(name) {
        return name.to_owned();
    }
    (2..=10_000u32)
        .map(|number| {
            let suffix = format!("-{number}");
            let mut cut = MAX_NAME.saturating_sub(suffix.len()).min(name.len());
            while !name.is_char_boundary(cut) {
                cut -= 1;
            }
            format!("{}{suffix}", name[..cut].trim_end_matches(['-', '.']))
        })
        .find(|candidate| free(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/// Creates the published file `name` with `text`, never overwriting: if a file has that name,
/// fails with [`BoardError::NameTaken`] carrying the path and a free alternative, so the UI can
/// let the person edit the name.
pub fn create_published(root: &Path, name: &str, text: &str) -> Result<(BoardFile, Revision), BoardError> {
    let file = BoardFile::published(name)?;
    prepare_directory(root, &file)?;
    if create_atomic(&file.path(root), text.as_bytes())? {
        Ok((file, Revision::of(text.as_bytes())))
    } else {
        Err(BoardError::NameTaken {
            path: file.relative(),
            suggestion: free_name(root, name),
        })
    }
}

/// Publishes draft `draft` as `name` (normally [`slug`] of its title): writes `text` (the
/// validated, serialized board) to `.unfold/boards/<name>.json` without overwriting, then
/// removes the draft.
pub fn publish(
    root: &Path,
    draft: &BoardFile,
    name: &str,
    text: &str,
) -> Result<(BoardFile, Revision), BoardError> {
    let created = create_published(root, name, text)?;
    remove(root, draft)?;
    Ok(created)
}

/// Moves published board `published` back to draft `draft`: writes `text` as the draft, then
/// removes the published file.
pub fn unpublish(
    root: &Path,
    published: &BoardFile,
    draft: &BoardFile,
    text: &str,
) -> Result<Revision, BoardError> {
    if fs::symlink_metadata(draft.path(root)).is_ok() {
        return Err(BoardError::NameTaken {
            path: draft.relative(),
            suggestion: draft.name.clone(),
        });
    }
    let revision = save(root, draft, text)?;
    remove(root, published)?;
    Ok(revision)
}

/// Renames a published board's file to `name`, never overwriting another board.
pub fn rename(root: &Path, from: &BoardFile, name: &str) -> Result<BoardFile, BoardError> {
    let source = from.resolve(root)?.ok_or_else(|| not_found(&from.path(root)))?;
    let bytes = read_regular(&source)?;
    let text = String::from_utf8(bytes).map_err(|_| BoardError::Syntax {
        line: 0,
        column: 0,
        message: "the file is not UTF-8".to_owned(),
    })?;
    let (file, _) = create_published(root, name, &text)?;
    remove(root, from)?;
    Ok(file)
}

/// Deletes a board file (the app asks first). A file that is already gone is not an error. A
/// link in the file's place is removed itself, never what it points at.
pub fn remove(root: &Path, file: &BoardFile) -> Result<(), BoardError> {
    let Some(path) = file.resolve(root)? else {
        return Ok(());
    };
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(&path, &error)),
    }
}

/// The board files in one place, sorted by name. Missing directories list nothing.
pub fn list(root: &Path, place: Place) -> Result<Vec<BoardFile>, BoardError> {
    let Some(directory) = place_directory(root, place)? else {
        return Ok(Vec::new());
    };
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(&directory, &error)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_error(&directory, &error))?;
        let name = entry.file_name();
        let Some(stem) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|stem| check_name(stem).is_ok())
        else {
            continue;
        };
        files.push(BoardFile {
            place,
            name: stem.to_owned(),
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}
