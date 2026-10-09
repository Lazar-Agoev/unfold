//! The per-folder lock (architecture §8.7, decision U-37): `local/cache/lock` keeps a second
//! copy of Unfold (another version, a development build) from opening a folder this copy has
//! open. Windows within one copy share the folder's workspace and never contend for it.
//!
//! The lock is an advisory, exclusive operating-system file lock (`flock` on Unix,
//! `LockFileEx` on Windows) on an open handle, so it disappears with the process that held it:
//! a crash never leaves a stale lock behind. The file holds the owner's process ID for
//! diagnostics only.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{Result, WorkspaceError};

/// A held folder lock; dropping it releases the lock.
#[derive(Debug)]
pub struct FolderLock {
    file: File,
    path: PathBuf,
}

/// Why the lock couldn't be taken.
#[derive(Debug)]
pub(crate) enum LockFailure {
    /// Another process holds it.
    Held,
    /// The lock file couldn't be created or locked.
    Io(std::io::Error),
}

impl FolderLock {
    /// Takes the lock at `path` without waiting. The folder containing it must exist.
    pub(crate) fn try_acquire(path: &Path) -> Result<Self, LockFailure> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(LockFailure::Io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(LockFailure::Held),
            Err(TryLockError::Error(error)) => return Err(LockFailure::Io(error)),
        }
        // The process ID is informational; failing to record it doesn't weaken the lock.
        let _ = file
            .set_len(0)
            .and_then(|()| writeln!(file, "{}", std::process::id()));
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Takes the lock for the folder at `root`, mapping a held lock to
    /// [`WorkspaceError::OpenElsewhere`].
    pub(crate) fn acquire(path: &Path, root: &Path) -> Result<Self> {
        Self::try_acquire(path).map_err(|failure| match failure {
            LockFailure::Held => WorkspaceError::OpenElsewhere {
                path: root.to_path_buf(),
            },
            LockFailure::Io(error) => WorkspaceError::io(path, error),
        })
    }

    /// The lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FolderLock {
    fn drop(&mut self) {
        // Closing the handle releases the lock too; unlocking first makes it immediate.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_is_refused_until_the_first_lets_go() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("lock");
        let first = FolderLock::acquire(&path, dir.path()).expect("first");
        let second = FolderLock::acquire(&path, dir.path()).expect_err("held");
        assert_eq!(second.code(), "open_elsewhere");
        drop(first);
        assert!(FolderLock::acquire(&path, dir.path()).is_ok());
    }
}
