//! Opening a folder's graph database where its layout says, taking the folder lock, and moving
//! to the fallback location when the folder refuses.

use std::io::ErrorKind;

use unfold_store::{Store, StoreConfig, StoreError};

use crate::error::{Result, WorkspaceError};
use crate::layout::{FallbackReason, Layout, Location};
use crate::lock::{FolderLock, LockFailure};

/// Creates `local/` if needed, takes the lock in `local/cache/`, and opens `graph.sqlite`.
///
/// A folder that refuses the writes (read-only), or a database whose file system refuses
/// write-ahead logging, moves `local/` to the fallback location and tries there. Another copy
/// of Unfold holding the lock is [`WorkspaceError::OpenElsewhere`].
pub(crate) fn open_store(layout: &mut Layout, version: &str) -> Result<(Store, FolderLock)> {
    layout.prepare()?;
    let lock = match FolderLock::try_acquire(&layout.lock_file()) {
        Ok(lock) => lock,
        Err(LockFailure::Io(error))
            if layout.location() == Location::InFolder
                && matches!(
                    error.kind(),
                    ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
                ) =>
        {
            layout.use_fallback(FallbackReason::ReadOnly)?;
            FolderLock::acquire(&layout.lock_file(), layout.root())?
        }
        Err(LockFailure::Held) => {
            return Err(WorkspaceError::OpenElsewhere {
                path: layout.root().to_path_buf(),
            });
        }
        Err(LockFailure::Io(error)) => return Err(WorkspaceError::io(layout.lock_file(), error)),
    };
    let config = StoreConfig::new(version);
    match Store::open(&layout.database(), &config) {
        Ok(store) => Ok((store, lock)),
        Err(StoreError::WalUnavailable { .. }) if layout.location() == Location::InFolder => {
            drop(lock);
            layout.use_fallback(FallbackReason::WalUnavailable)?;
            let lock = FolderLock::acquire(&layout.lock_file(), layout.root())?;
            Ok((Store::open(&layout.database(), &config)?, lock))
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_locks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        let mut layout = Layout::discover(&root, &root.join("fallback"));
        let (store, _lock) = open_store(&mut layout, "v1").expect("open");
        assert_eq!(store.opened, unfold_store::Opened::Created);
        let mut again = Layout::discover(&root, &root.join("fallback"));
        let error = open_store(&mut again, "v1").expect_err("locked");
        assert_eq!(error.code(), "open_elsewhere");
    }
}
