//! The in-memory view of the open folder that `oxc_resolver` resolves against.
//!
//! The view holds exactly what Unfold knows exists: the program's files, the unindexed files,
//! the context (declaration) files, and the configuration inputs, under a virtual root. Only
//! `package.json` contents can be read. Nothing outside the folder exists, `node_modules` is
//! never walked, and the resolver never touches the disk, so resolution is a pure function of the
//! fingerprinted inputs.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{FileMetadata, FileSystem, ResolveError};
use rustc_hash::{FxHashMap, FxHashSet};

/// The virtual folder root every path hangs from.
#[cfg(not(windows))]
const ROOT: &str = "/";
#[cfg(windows)]
const ROOT: &str = "C:\\";

/// The absolute virtual path of a folder-relative path (`""` is the root).
pub(crate) fn absolute(path: &str) -> PathBuf {
    let mut absolute = PathBuf::from(ROOT);
    absolute.extend(path.split('/').filter(|part| !part.is_empty()));
    absolute
}

/// The folder-relative path of an absolute virtual path, if it lies in the folder.
pub(crate) fn relative(path: &Path) -> Option<String> {
    let rest = path.strip_prefix(ROOT).ok()?;
    let mut parts = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The folder's files and folders, and the bytes of its `package.json` files.
#[derive(Debug, Default, Clone)]
pub(crate) struct Vfs {
    files: FxHashMap<PathBuf, Option<Arc<str>>>,
    dirs: FxHashSet<PathBuf>,
}

impl Vfs {
    /// Adds a file whose contents can't be read.
    pub(crate) fn add(&mut self, path: &str) {
        self.insert(path, None);
    }

    /// Adds a file with readable contents.
    pub(crate) fn add_with_text(&mut self, path: &str, text: Arc<str>) {
        self.insert(path, Some(text));
    }

    fn insert(&mut self, path: &str, text: Option<Arc<str>>) {
        let file = absolute(path);
        let mut dir = file.parent();
        while let Some(parent) = dir {
            if !self.dirs.insert(parent.to_path_buf()) {
                break;
            }
            dir = parent.parent();
        }
        self.files.insert(file, text);
    }
}

fn not_found() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

impl FileSystem for Vfs {
    fn new() -> Self {
        Self::default()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.read_to_string(path).map(String::into_bytes)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        match self.files.get(path) {
            Some(Some(text)) => Ok(text.to_string()),
            Some(None) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "contents not available",
            )),
            None => Err(not_found()),
        }
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        if self.files.contains_key(path) {
            Ok(FileMetadata::new(true, false, false))
        } else if self.dirs.contains(path) {
            Ok(FileMetadata::new(false, true, false))
        } else {
            Err(not_found())
        }
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.metadata(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a link", path.display()),
        )
        .into())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if self.files.contains_key(path) || self.dirs.contains(path) {
            Ok(path.to_path_buf())
        } else {
            Err(not_found())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_and_answers_like_a_file_system() {
        let mut vfs = Vfs::default();
        vfs.add("src/a.ts");
        vfs.add_with_text("package.json", "{}".into());
        assert_eq!(relative(&absolute("src/a.ts")).as_deref(), Some("src/a.ts"));
        assert_eq!(relative(&absolute("")).as_deref(), Some(""));
        assert!(vfs.metadata(&absolute("src")).is_ok_and(FileMetadata::is_dir));
        assert!(vfs.metadata(&absolute("")).is_ok_and(FileMetadata::is_dir));
        assert!(
            vfs.metadata(&absolute("src/a.ts"))
                .is_ok_and(FileMetadata::is_file)
        );
        assert!(vfs.metadata(&absolute("src/b.ts")).is_err());
        assert_eq!(
            vfs.read_to_string(&absolute("package.json")).ok().as_deref(),
            Some("{}")
        );
        assert!(vfs.read_to_string(&absolute("src/a.ts")).is_err());
        assert!(vfs.read_link(&absolute("src/a.ts")).is_err());
    }
}
