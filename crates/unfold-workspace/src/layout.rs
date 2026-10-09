//! The `.unfold/` layout of one folder (architecture §8.1–8.2, decision U-18) and its fallback
//! location.
//!
//! ```text
//! <folder>/.unfold/
//! ├── config.toml        shared settings (written only when they differ from the defaults)
//! ├── boards/            published boards
//! └── local/             this machine only; its .gitignore contains "*"
//!     ├── .gitignore
//!     ├── cache/         disposable: graph.sqlite and the lock
//!     ├── drafts/        unpublished boards
//!     └── state.json     windows and the background-update switch
//! ```
//!
//! [`Layout::discover`] decides where `local/` is **without writing anything**: opening and
//! browsing a folder never creates `.unfold/`. [`Layout::prepare`] creates `local/` on the first
//! Compile (and again if `local/cache/` was deleted while the folder is open).
//!
//! When the folder is read-only (detected by attempting the write), or sits on a network volume
//! where SQLite's write-ahead log is unsafe, `local/` moves to
//! `<fallbackCacheDir>/<hash of the canonical path>/` and the status bar says so. Shared files
//! (`config.toml`, `boards/`) stay in the folder either way.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{Result, WorkspaceError};
use crate::volume::is_network_volume;

/// The folder Unfold keeps everything about a repository in.
pub const UNFOLD_DIR: &str = ".unfold";
/// The shared settings file, relative to the folder.
pub const CONFIG_PATH: &str = ".unfold/config.toml";
/// The published boards folder, relative to the folder.
pub const BOARDS_PATH: &str = ".unfold/boards";
/// The machine-local folder, relative to the folder (when not in the fallback location).
pub const LOCAL_PATH: &str = ".unfold/local";

/// Why `local/` lives in the fallback location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    /// The folder can't be written, or `.unfold/` or `local/` in it is a symbolic link, which
    /// Unfold never writes through (it could lead outside the folder).
    ReadOnly,
    /// The folder is on a network volume.
    NetworkVolume,
    /// SQLite refused write-ahead logging in the folder.
    WalUnavailable,
}

/// Where `local/` lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// `<folder>/.unfold/local/`.
    InFolder,
    /// `<fallbackCacheDir>/<hash>/`.
    Fallback(FallbackReason),
}

/// What the fallback location records about the folder it belongs to.
#[derive(Debug, Serialize, Deserialize)]
struct Origin {
    path: String,
    reason: FallbackReason,
}

const ORIGIN_FILE: &str = "origin.json";

/// The paths of one folder's `.unfold/` layout. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
    fallback_local: PathBuf,
    location: Location,
}

impl Layout {
    /// Decides where `local/` is for the folder at canonical `root`, without writing.
    pub fn discover(root: &Path, fallback_cache_dir: &Path) -> Self {
        Self::discover_with(root, fallback_cache_dir, is_network_volume(root))
    }

    pub(crate) fn discover_with(root: &Path, fallback_cache_dir: &Path, network: bool) -> Self {
        let fallback_local = fallback_cache_dir.join(folder_hash(root));
        let in_folder = root.join(LOCAL_PATH);
        let linked = has_link(root);
        let location = if in_folder.is_dir() && !network && !linked {
            Location::InFolder
        } else if fallback_local.is_dir() {
            Location::Fallback(read_origin(&fallback_local).unwrap_or(if network {
                FallbackReason::NetworkVolume
            } else {
                FallbackReason::ReadOnly
            }))
        } else if network {
            Location::Fallback(FallbackReason::NetworkVolume)
        } else if linked {
            Location::Fallback(FallbackReason::ReadOnly)
        } else {
            Location::InFolder
        };
        Self {
            root: root.to_path_buf(),
            fallback_local,
            location,
        }
    }

    /// Whether the folder has been compiled on this machine: its `local/` exists.
    pub fn is_compiled(&self) -> bool {
        self.local().is_dir()
    }

    /// Creates `local/` with its `cache/` and `drafts/` folders (and, in the folder, the
    /// `.gitignore` that hides `local/` from Git). A folder that refuses the write moves
    /// `local/` to the fallback location.
    pub fn prepare(&mut self) -> Result<()> {
        if self.location == Location::InFolder {
            match self.create_in_folder() {
                Ok(()) => return Ok(()),
                Err(error) if is_read_only(&error) => {
                    self.location = Location::Fallback(FallbackReason::ReadOnly);
                }
                Err(error) => return Err(WorkspaceError::io(self.local(), error)),
            }
        }
        let Location::Fallback(reason) = self.location else {
            return Ok(());
        };
        self.create_fallback(reason)
    }

    /// Moves `local/` to the fallback location for `reason` and creates it there.
    pub fn use_fallback(&mut self, reason: FallbackReason) -> Result<()> {
        self.location = Location::Fallback(reason);
        self.create_fallback(reason)
    }

    fn create_in_folder(&self) -> std::io::Result<()> {
        let refuse_links = || {
            if has_link(&self.root) {
                Err(std::io::Error::new(
                    ErrorKind::PermissionDenied,
                    "`.unfold/local` goes through a symbolic link",
                ))
            } else {
                Ok(())
            }
        };
        refuse_links()?;
        let local = self.local();
        std::fs::create_dir_all(self.cache())?;
        std::fs::create_dir_all(self.drafts())?;
        // Checked again now that the folders exist, so a link raced in is never written through.
        refuse_links()?;
        // `create_new` refuses an existing file and never follows a (dangling) link.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local.join(".gitignore"))
        {
            Ok(mut ignore) => ignore.write_all(b"*\n")?,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        // Creating folders that already exist proves nothing; write to be sure.
        let probe = self.cache().join(format!(".probe-{}", std::process::id()));
        std::fs::write(&probe, b"")?;
        std::fs::remove_file(&probe)
    }

    fn create_fallback(&self, reason: FallbackReason) -> Result<()> {
        let local = self.local();
        for dir in [self.cache(), self.drafts()] {
            std::fs::create_dir_all(&dir).map_err(|error| WorkspaceError::io(&dir, error))?;
        }
        let origin = Origin {
            path: self.root.to_string_lossy().into_owned(),
            reason,
        };
        let text = serde_json::to_string_pretty(&origin).map_err(|error| WorkspaceError::Encode {
            what: "origin.json",
            message: error.to_string(),
        })?;
        let path = local.join(ORIGIN_FILE);
        std::fs::write(&path, text).map_err(|error| WorkspaceError::io(path, error))
    }

    /// The folder's canonical root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where `local/` lives.
    pub fn location(&self) -> Location {
        self.location
    }

    /// `<folder>/.unfold/`.
    pub fn unfold_dir(&self) -> PathBuf {
        self.root.join(UNFOLD_DIR)
    }

    /// `<folder>/.unfold/config.toml`.
    pub fn config(&self) -> PathBuf {
        self.root.join(CONFIG_PATH)
    }

    /// `<folder>/.unfold/boards/`.
    pub fn boards(&self) -> PathBuf {
        self.root.join(BOARDS_PATH)
    }

    /// `local/`, in the folder or in the fallback location.
    pub fn local(&self) -> PathBuf {
        match self.location {
            Location::InFolder => self.root.join(LOCAL_PATH),
            Location::Fallback(_) => self.fallback_local.clone(),
        }
    }

    /// `local/cache/`.
    pub fn cache(&self) -> PathBuf {
        self.local().join("cache")
    }

    /// `local/cache/graph.sqlite`.
    pub fn database(&self) -> PathBuf {
        self.cache().join("graph.sqlite")
    }

    /// `local/cache/lock`.
    pub fn lock_file(&self) -> PathBuf {
        self.cache().join("lock")
    }

    /// `local/drafts/`.
    pub fn drafts(&self) -> PathBuf {
        self.local().join("drafts")
    }

    /// `local/state.json`.
    pub fn state_file(&self) -> PathBuf {
        self.local().join("state.json")
    }
}

/// The in-folder paths Unfold creates or writes under `.unfold/local/`.
const LOCAL_PARTS: [&str; 6] = [
    UNFOLD_DIR,
    LOCAL_PATH,
    ".unfold/local/cache",
    ".unfold/local/drafts",
    ".unfold/local/state.json",
    ".unfold/local/.gitignore",
];

/// Whether `.unfold/`, `local/`, or anything Unfold writes directly in `local/` is a symbolic
/// link. A repository can commit such a link; writing through it could land outside the folder,
/// so the folder then keeps `local/` in the fallback location instead.
fn has_link(root: &Path) -> bool {
    LOCAL_PARTS.iter().any(|part| {
        std::fs::symlink_metadata(root.join(part)).is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}

/// Whether an I/O error means the location can't be written.
fn is_read_only(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
    )
}

fn read_origin(local: &Path) -> Option<FallbackReason> {
    let text = std::fs::read_to_string(local.join(ORIGIN_FILE)).ok()?;
    serde_json::from_str::<Origin>(&text)
        .ok()
        .map(|origin| origin.reason)
}

/// The fallback folder name for `root`: 32 hex digits of the SHA-256 of its canonical path.
fn folder_hash(root: &Path) -> String {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    digest
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut text, byte| {
            // Writing to a String can't fail.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical");
        let root = base.join("repo");
        let fallback = base.join("fallback");
        std::fs::create_dir_all(&root).expect("mkdir");
        (dir, root, fallback)
    }

    #[test]
    fn discovering_writes_nothing() {
        let (_dir, root, fallback) = dirs();
        let layout = Layout::discover(&root, &fallback);
        assert_eq!(layout.location(), Location::InFolder);
        assert!(!layout.is_compiled());
        assert!(!root.join(UNFOLD_DIR).exists());
        assert!(!fallback.exists());
    }

    #[test]
    fn prepare_creates_the_local_folder() {
        let (_dir, root, fallback) = dirs();
        let mut layout = Layout::discover(&root, &fallback);
        layout.prepare().expect("prepare");
        assert!(layout.is_compiled());
        assert_eq!(
            std::fs::read_to_string(root.join(".unfold/local/.gitignore")).expect("ignore"),
            "*\n"
        );
        assert!(root.join(".unfold/local/cache").is_dir());
        assert!(root.join(".unfold/local/drafts").is_dir());
        assert_eq!(Layout::discover(&root, &fallback), layout);
    }

    #[test]
    fn network_volumes_use_the_fallback() {
        let (_dir, root, fallback) = dirs();
        let mut layout = Layout::discover_with(&root, &fallback, true);
        assert_eq!(
            layout.location(),
            Location::Fallback(FallbackReason::NetworkVolume)
        );
        layout.prepare().expect("prepare");
        assert!(layout.local().starts_with(&fallback));
        assert!(!root.join(UNFOLD_DIR).exists());
        let again = Layout::discover_with(&root, &fallback, false);
        assert_eq!(
            again.location(),
            Location::Fallback(FallbackReason::NetworkVolume)
        );
        assert!(again.is_compiled());
    }

    #[cfg(unix)]
    #[test]
    fn links_in_the_local_folder_move_it_to_the_fallback() {
        let (_dir, root, fallback) = dirs();
        let outside = root.parent().expect("parent").join("outside");
        std::fs::create_dir_all(&outside).expect("mkdir");
        std::fs::create_dir_all(root.join(UNFOLD_DIR)).expect("mkdir");
        std::os::unix::fs::symlink(&outside, root.join(LOCAL_PATH)).expect("link");
        let mut layout = Layout::discover(&root, &fallback);
        assert_eq!(layout.location(), Location::Fallback(FallbackReason::ReadOnly));
        layout.prepare().expect("prepare");
        assert!(layout.local().starts_with(&fallback));
        assert_eq!(
            std::fs::read_dir(&outside).expect("read").count(),
            0,
            "nothing written through the link"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_gitignore_link_is_never_followed() {
        let (_dir, root, fallback) = dirs();
        let target = root.parent().expect("parent").join("target-file");
        std::fs::create_dir_all(root.join(LOCAL_PATH)).expect("mkdir");
        std::os::unix::fs::symlink(&target, root.join(".unfold/local/.gitignore")).expect("link");
        let mut layout = Layout::discover(&root, &fallback);
        layout.prepare().expect("prepare");
        assert!(!target.exists(), "the link's target was created");
        assert!(layout.local().starts_with(&fallback));
    }

    #[test]
    fn hashes_differ_by_path() {
        assert_ne!(folder_hash(Path::new("/a")), folder_hash(Path::new("/b")));
        assert_eq!(folder_hash(Path::new("/a")).len(), 32);
    }
}
