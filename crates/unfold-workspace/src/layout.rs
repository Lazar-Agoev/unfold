//! The `.unfold/` layout of one folder (docs/architecture.md#storage–8.2, project storage).
//!
//! ```text
//! <folder>/
//! ├── unfold.toml        shared settings (written only when they differ from the defaults)
//! └── .unfold/
//!     ├── boards/        boards, saved as they are edited
//!     └── local/         this machine only; its .gitignore contains "*"
//!         ├── .gitignore
//!         ├── cache/     disposable: graph.sqlite and the lock
//!         └── state.json empty board folders
//! ```
//!
//! Opening and browsing a folder never creates `.unfold/`: [`Layout::prepare`] creates `local/`
//! on the first Compile or the first write of remembered state (and again if `local/cache/` was
//! deleted while the folder is open).
//!
//! Unfold needs to write `.unfold/` in the folder. [`check_writable`] refuses a folder that is
//! read-only, sits on a network volume (where SQLite's write-ahead log is unsafe), or whose
//! `.unfold/` goes through a symbolic link (writing through it could land outside the folder):
//! adding such a folder as a project is refused, and so is preparing it.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::error::{Result, WorkspaceError};
use crate::volume::is_network_volume;

/// The folder Unfold keeps everything about a repository in.
pub const UNFOLD_DIR: &str = ".unfold";
/// The shared settings file, relative to the folder.
pub const CONFIG_PATH: &str = "unfold.toml";
/// The boards folder, relative to the folder.
pub const BOARDS_PATH: &str = ".unfold/boards";
/// The machine-local folder, relative to the folder.
pub const LOCAL_PATH: &str = ".unfold/local";

/// The paths of one folder's `.unfold/` layout. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// The layout of the folder at canonical `root`. Writes nothing.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Whether the folder has been compiled on this machine: its `local/` exists.
    pub fn is_compiled(&self) -> bool {
        self.local().is_dir()
    }

    /// Creates `local/` with its `cache/` folder and the `.gitignore` that hides `local/` from
    /// Git. Refused with `folder_not_writable` for a folder Unfold can't write
    /// `.unfold/` into.
    pub fn prepare(&self) -> Result<()> {
        check_writable(&self.root)?;
        self.create_local().map_err(|error| {
            if is_read_only(&error) {
                WorkspaceError::FolderNotWritable {
                    path: self.root.clone(),
                    reason: Unwritable::ReadOnly,
                }
            } else {
                WorkspaceError::io(self.local(), error)
            }
        })
    }

    fn create_local(&self) -> std::io::Result<()> {
        let local = self.local();
        std::fs::create_dir_all(self.cache())?;
        // Checked again now that the folders exist, so a link raced in is never written through.
        if has_link(&self.root) {
            return Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "`.unfold/local` goes through a symbolic link",
            ));
        }
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

    /// The folder's canonical root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<folder>/.unfold/`.
    pub fn unfold_dir(&self) -> PathBuf {
        self.root.join(UNFOLD_DIR)
    }

    /// `<folder>/unfold.toml`.
    pub fn config(&self) -> PathBuf {
        self.root.join(CONFIG_PATH)
    }

    /// `<folder>/.unfold/boards/`.
    pub fn boards(&self) -> PathBuf {
        self.root.join(BOARDS_PATH)
    }

    /// `<folder>/.unfold/local/`.
    pub fn local(&self) -> PathBuf {
        self.root.join(LOCAL_PATH)
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

    /// `local/state.json`.
    pub fn state_file(&self) -> PathBuf {
        self.local().join("state.json")
    }
}

/// Why Unfold can't write `.unfold/` in a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unwritable {
    /// The folder, or its `.unfold/`, can't be written.
    ReadOnly,
    /// The folder is on a network volume.
    NetworkVolume,
    /// `.unfold/`, `local/`, or something Unfold writes in `local/` is a symbolic link.
    Linked,
}

impl std::fmt::Display for Unwritable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ReadOnly => "the folder is read-only",
            Self::NetworkVolume => "the folder is on a network volume",
            Self::Linked => "its `.unfold/` goes through a symbolic link",
        })
    }
}

/// Checks that Unfold can write `.unfold/` in the canonical folder `root`, without writing:
/// refused with `folder_not_writable` when the folder is on a network volume, when `.unfold/`
/// or `local/` in it is a symbolic link, or when the deepest of `local/`, `.unfold/`, and the
/// folder that exists can't be written.
pub fn check_writable(root: &Path) -> Result<()> {
    let refuse = |reason| {
        Err(WorkspaceError::FolderNotWritable {
            path: root.to_path_buf(),
            reason,
        })
    };
    if is_network_volume(root) {
        return refuse(Unwritable::NetworkVolume);
    }
    if has_link(root) {
        return refuse(Unwritable::Linked);
    }
    let nearest = [root.join(LOCAL_PATH), root.join(UNFOLD_DIR)]
        .into_iter()
        .find(|dir| dir.is_dir())
        .unwrap_or_else(|| root.to_path_buf());
    if !is_writable(&nearest) {
        return refuse(Unwritable::ReadOnly);
    }
    Ok(())
}

/// The in-folder paths Unfold creates or writes under `.unfold/local/`.
const LOCAL_PARTS: [&str; 5] = [
    UNFOLD_DIR,
    LOCAL_PATH,
    ".unfold/local/cache",
    ".unfold/local/state.json",
    ".unfold/local/.gitignore",
];

/// Whether `.unfold/`, `local/`, or anything Unfold writes directly in `local/` is a symbolic
/// link. A repository can commit such a link; writing through it could land outside the folder.
fn has_link(root: &Path) -> bool {
    LOCAL_PARTS.iter().any(|part| {
        std::fs::symlink_metadata(root.join(part)).is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}

/// Whether this process may create entries in the folder `dir`, asked of the operating system
/// (permissions, access control lists, read-only volumes) without writing.
fn is_writable(dir: &Path) -> bool {
    nix::unistd::access(dir, nix::unistd::AccessFlags::W_OK).is_ok()
}

/// Whether an I/O error means the location can't be written.
fn is_read_only(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical");
        let root = base.join("repo");
        std::fs::create_dir_all(&root).expect("mkdir");
        (dir, root)
    }

    fn reason(root: &Path) -> Option<Unwritable> {
        match check_writable(root) {
            Ok(()) => None,
            Err(WorkspaceError::FolderNotWritable { reason, .. }) => Some(reason),
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn a_new_layout_writes_nothing() {
        let (_dir, root) = dirs();
        let layout = Layout::new(&root);
        assert!(!layout.is_compiled());
        assert!(!root.join(UNFOLD_DIR).exists());
    }

    #[test]
    fn prepare_creates_the_local_folder() {
        let (_dir, root) = dirs();
        let layout = Layout::new(&root);
        layout.prepare().expect("prepare");
        assert!(layout.is_compiled());
        assert_eq!(
            std::fs::read_to_string(root.join(".unfold/local/.gitignore")).expect("ignore"),
            "*\n"
        );
        assert!(root.join(".unfold/local/cache").is_dir());
        layout.prepare().expect("prepare again");
    }

    #[test]
    fn a_read_only_folder_is_refused_before_anything_is_written() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, root) = dirs();
        assert_eq!(reason(&root), None, "a writable folder");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let checked = reason(&root);
        let prepared = Layout::new(&root).prepare();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        assert_eq!(checked, Some(Unwritable::ReadOnly));
        let error = prepared.expect_err("read-only");
        assert_eq!(error.code(), "folder_not_writable");
        assert!(
            error.to_string().contains("Unfold needs to write `.unfold/`"),
            "{error}"
        );
        assert!(!root.join(UNFOLD_DIR).exists());
    }

    #[test]
    fn links_in_the_local_folder_are_refused() {
        let (_dir, root) = dirs();
        let outside = root.parent().expect("parent").join("outside");
        std::fs::create_dir_all(&outside).expect("mkdir");
        std::fs::create_dir_all(root.join(UNFOLD_DIR)).expect("mkdir");
        std::os::unix::fs::symlink(&outside, root.join(LOCAL_PATH)).expect("link");
        assert_eq!(reason(&root), Some(Unwritable::Linked));
        assert!(Layout::new(&root).prepare().is_err());
        assert_eq!(
            std::fs::read_dir(&outside).expect("read").count(),
            0,
            "nothing written through the link"
        );
    }

    #[test]
    fn a_dangling_gitignore_link_is_never_followed() {
        let (_dir, root) = dirs();
        let target = root.parent().expect("parent").join("target-file");
        std::fs::create_dir_all(root.join(LOCAL_PATH)).expect("mkdir");
        std::os::unix::fs::symlink(&target, root.join(".unfold/local/.gitignore")).expect("link");
        assert!(Layout::new(&root).prepare().is_err());
        assert!(!target.exists(), "the link's target was created");
    }
}
