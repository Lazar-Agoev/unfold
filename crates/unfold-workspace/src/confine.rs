//! Path confinement (architecture §13): the one check every path a command names goes through.
//!
//! A window can never name a path outside its open folder. [`confine`] accepts only
//! folder-relative, `/`-separated paths with no empty, `.`, or `..` component, no backslash, no
//! NUL, and no absolute or drive prefix (the rules of [`RepoPath`]); the empty string names the
//! folder itself. It then resolves the path on disk: every existing component that is a symlink
//! must resolve to somewhere inside the folder, so a link to `/etc` or to a sibling folder is
//! rejected even though its name looks harmless. Components that don't exist yet are accepted
//! (they contain no links), so commands may name paths that are about to be created.

use std::path::{Path, PathBuf};

use unfold_core::RepoPath;

use crate::error::{Result, WorkspaceError};

/// A path a command named, checked to stay inside the open folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfinedPath {
    relative: Option<RepoPath>,
    absolute: PathBuf,
}

impl ConfinedPath {
    /// The folder-relative path, or `None` for the folder itself.
    pub fn relative(&self) -> Option<&RepoPath> {
        self.relative.as_ref()
    }

    /// The folder-relative path as text (empty for the folder itself).
    pub fn as_str(&self) -> &str {
        self.relative.as_ref().map_or("", RepoPath::as_str)
    }

    /// The path under the folder's canonical root. Symlinks in it resolve inside the folder.
    pub fn absolute(&self) -> &Path {
        &self.absolute
    }
}

/// Checks `path` against the folder at `root`, which must be canonical. See the
/// [module documentation](self).
pub fn confine(root: &Path, path: &str) -> Result<ConfinedPath> {
    let reject = |reason: &'static str| WorkspaceError::InvalidPath {
        path: path.to_owned(),
        reason,
    };
    if path.is_empty() {
        return Ok(ConfinedPath {
            relative: None,
            absolute: root.to_path_buf(),
        });
    }
    if path.contains('\0') {
        return Err(reject("it contains a NUL character"));
    }
    if path.starts_with('/') || path.starts_with('\\') || Path::new(path).is_absolute() {
        return Err(reject("it is absolute"));
    }
    if path.split('/').any(|part| part == "..") {
        return Err(reject("it has a `..` component"));
    }
    let relative = RepoPath::new(path).map_err(|_| reject("it is not a folder-relative path"))?;
    let mut current = root.to_path_buf();
    for part in relative.as_str().split('/') {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target =
                    std::fs::canonicalize(&current).map_err(|_| reject("it goes through a broken link"))?;
                if !target.starts_with(root) {
                    return Err(reject("it goes through a link that leaves the folder"));
                }
            }
            Ok(_) => {}
            // The rest doesn't exist, so it holds no links.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(reject("it can't be examined")),
        }
    }
    Ok(ConfinedPath {
        absolute: root.join(relative.as_str()),
        relative: Some(relative),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        std::fs::create_dir_all(root.join("src/lib")).expect("mkdir");
        std::fs::write(root.join("src/a.ts"), "x").expect("write");
        (dir, root)
    }

    #[test]
    fn accepts_paths_inside_the_folder() {
        let (_dir, root) = root();
        let found = confine(&root, "src/a.ts").expect("inside");
        assert_eq!(found.as_str(), "src/a.ts");
        assert_eq!(found.absolute(), root.join("src/a.ts"));
        assert_eq!(confine(&root, "").expect("root").relative(), None);
        assert!(confine(&root, "src/new/file.ts").is_ok());
    }

    #[test]
    fn rejects_escapes() {
        let (_dir, root) = root();
        for bad in [
            "../x",
            "src/../../x",
            "/etc/passwd",
            "\\x",
            "src//a.ts",
            "./src",
            "src/./a.ts",
            "a\0b",
            "src\\a.ts",
            "src/",
        ] {
            let error = confine(&root, bad).expect_err(bad);
            assert_eq!(error.code(), "invalid_path", "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_that_leave_the_folder() {
        let (_dir, root) = root();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "s").expect("write");
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).expect("link");
        std::os::unix::fs::symlink(root.join("src"), root.join("inner")).expect("link");
        std::os::unix::fs::symlink(root.join("missing"), root.join("broken")).expect("link");
        assert!(confine(&root, "escape").is_err());
        assert!(confine(&root, "escape/secret").is_err());
        assert!(confine(&root, "broken").is_err());
        assert!(confine(&root, "inner/a.ts").is_ok());
    }
}
