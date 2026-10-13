//! Opening a folder's graph database: creating `local/` and taking the folder lock.

use unfold_store::{Store, StoreConfig};

use crate::error::Result;
use crate::layout::Layout;
use crate::lock::FolderLock;

/// Creates `local/` if needed, takes the lock in `local/cache/`, and opens `graph.sqlite`.
/// Another copy of Unfold holding the lock is `open_elsewhere`.
pub(crate) fn open_store(layout: &Layout, version: &str) -> Result<(Store, FolderLock)> {
    layout.prepare()?;
    let lock = FolderLock::acquire(&layout.lock_file(), layout.root())?;
    let store = Store::open(&layout.database(), &StoreConfig::new(version))?;
    Ok((store, lock))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_locks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        let layout = Layout::new(&root);
        let (store, _lock) = open_store(&layout, "v1").expect("open");
        assert_eq!(store.opened, unfold_store::Opened::Created);
        let error = open_store(&layout, "v1").expect_err("locked");
        assert_eq!(error.code(), "open_elsewhere");
    }
}
