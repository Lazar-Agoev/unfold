//! Detecting network volumes, where SQLite's write-ahead log is unsafe (docs/architecture.md#storage):
//! Unfold refuses to add a folder on one.
//!
//! The check reads the file-system type with `statfs` (through `nix`, so this crate stays free
//! of `unsafe`). Where the platform has no such call here (Windows, some Unix systems), the
//! folder counts as local; SQLite then still refuses write-ahead logging on file systems that
//! can't support it, which the store reports when the folder is first compiled.

use std::path::Path;

/// Whether `path` is on a network file system.
pub(crate) fn is_network_volume(path: &Path) -> bool {
    imp::is_network_volume(path)
}

#[cfg(any(target_vendor = "apple", target_os = "freebsd", target_os = "openbsd"))]
mod imp {
    use std::path::Path;

    /// File-system type names (`statfs.f_fstypename`) of network file systems.
    const NETWORK: &[&str] = &["nfs", "smbfs", "afpfs", "webdav", "cifs", "ftp", "9p"];

    pub(super) fn is_network_volume(path: &Path) -> bool {
        nix::sys::statfs::statfs(path).is_ok_and(|stat| NETWORK.contains(&stat.filesystem_type_name()))
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use std::path::Path;

    /// Magic numbers (`statfs.f_type`) of network file systems: NFS, SMB, SMB2, CIFS, NCP,
    /// AFS, Coda, and 9p.
    const NETWORK: &[u64] = &[
        0x6969,
        0x517b,
        0xfe53_4d42,
        0xff53_4d42,
        0x564c,
        0x5346_414f,
        0x7375_7245,
        0x0102_1997,
    ];

    pub(super) fn is_network_volume(path: &Path) -> bool {
        nix::sys::statfs::statfs(path).is_ok_and(|stat| {
            // `f_type` is signed on some targets; the magic numbers are 32-bit patterns.
            #[allow(clippy::cast_sign_loss)]
            let magic = (stat.filesystem_type().0 as u64) & 0xffff_ffff;
            NETWORK.contains(&magic)
        })
    }
}

#[cfg(not(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "linux",
    target_os = "android"
)))]
mod imp {
    use std::path::Path;

    pub(super) fn is_network_volume(_path: &Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn temporary_folders_are_local() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(!super::is_network_volume(dir.path()));
    }
}
