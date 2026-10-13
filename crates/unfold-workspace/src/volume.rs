//! Detecting network volumes, where SQLite's write-ahead log is unsafe (docs/architecture.md#storage):
//! Unfold refuses to add a folder on one.
//!
//! The check reads the file-system type name with `statfs` (through `nix`, so this crate stays
//! free of `unsafe`).

use std::path::Path;

/// File-system type names (`statfs.f_fstypename`) of network file systems.
const NETWORK: &[&str] = &["nfs", "smbfs", "afpfs", "webdav", "cifs", "ftp", "9p"];

/// Whether `path` is on a network file system.
pub(crate) fn is_network_volume(path: &Path) -> bool {
    nix::sys::statfs::statfs(path).is_ok_and(|stat| NETWORK.contains(&stat.filesystem_type_name()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn temporary_folders_are_local() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(!super::is_network_volume(dir.path()));
    }
}
