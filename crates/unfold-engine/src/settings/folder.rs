//! Folder paths in settings: a Python project root or source root, where `.` is the open folder.

use std::fmt;

use serde::{Serialize, Serializer};
use unfold_core::RepoPath;
use unfold_core::path::PathError;

/// A folder relative to the open folder. The folder itself is `.`; anything else is a
/// normalized [`RepoPath`] (no `.` or empty components, no trailing slash).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FolderPath(Option<RepoPath>);

/// Why a string is not a folder inside the open folder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderPathError {
    /// The path is empty.
    #[error("must not be empty; use \".\" for the folder itself")]
    Empty,
    /// The path is absolute.
    #[error("must be relative to the folder, not absolute")]
    Absolute,
    /// The path has a `..` component.
    #[error("must stay inside the folder (no `..`)")]
    Escapes,
    /// The path is otherwise malformed.
    #[error("{0}")]
    Invalid(PathError),
}

impl FolderPath {
    /// The open folder itself.
    pub const ROOT: FolderPath = FolderPath(None);

    /// Parses and normalizes a folder written in settings. `.` components and a trailing slash
    /// are dropped; `..`, absolute paths, empty components, backslashes, NUL, and drive prefixes
    /// are rejected.
    pub fn parse(text: &str) -> Result<Self, FolderPathError> {
        if text.is_empty() {
            return Err(FolderPathError::Empty);
        }
        if text.starts_with('/') {
            return Err(FolderPathError::Absolute);
        }
        let trimmed = text.strip_suffix('/').unwrap_or(text);
        let mut parts = Vec::new();
        for part in trimmed.split('/') {
            match part {
                "." => {}
                ".." => return Err(FolderPathError::Escapes),
                _ => parts.push(part),
            }
        }
        if parts.is_empty() {
            return Ok(Self::ROOT);
        }
        RepoPath::new(&parts.join("/"))
            .map(|path| Self(Some(path)))
            .map_err(FolderPathError::Invalid)
    }

    /// The folder as written canonically: `.` or a relative path.
    pub fn as_str(&self) -> &str {
        self.0.as_ref().map_or(".", RepoPath::as_str)
    }

    /// The number of components (`.` has none).
    pub fn depth(&self) -> usize {
        self.0.as_ref().map_or(0, |path| path.as_str().split('/').count())
    }

    /// Whether `path` lies inside this folder.
    pub fn contains(&self, path: &RepoPath) -> bool {
        match &self.0 {
            None => true,
            Some(folder) => path
                .as_str()
                .strip_prefix(folder.as_str())
                .is_some_and(|rest| rest.starts_with('/')),
        }
    }

    /// This folder followed by `child`, a folder relative to it.
    pub fn join(&self, child: &FolderPath) -> FolderPath {
        match (&self.0, &child.0) {
            (None, _) => child.clone(),
            (_, None) => self.clone(),
            (Some(parent), Some(child)) => Self(Some(
                RepoPath::new(&format!("{parent}/{child}"))
                    .expect("two valid paths joined by `/` form a valid path"),
            )),
        }
    }
}

impl fmt::Display for FolderPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for FolderPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_rejects_escapes() {
        for (text, expected) in [
            (".", "."),
            ("./", "."),
            ("src", "src"),
            ("src/", "src"),
            ("./a/./b", "a/b"),
        ] {
            assert_eq!(FolderPath::parse(text).unwrap().as_str(), expected, "{text}");
        }
        assert_eq!(FolderPath::parse(""), Err(FolderPathError::Empty));
        assert_eq!(FolderPath::parse("/abs"), Err(FolderPathError::Absolute));
        assert_eq!(FolderPath::parse("../out"), Err(FolderPathError::Escapes));
        assert_eq!(FolderPath::parse("a/../b"), Err(FolderPathError::Escapes));
        assert!(matches!(
            FolderPath::parse("a//b"),
            Err(FolderPathError::Invalid(_))
        ));
        assert!(matches!(
            FolderPath::parse("a\\b"),
            Err(FolderPathError::Invalid(_))
        ));
        assert!(matches!(
            FolderPath::parse("C:/x"),
            Err(FolderPathError::Invalid(_))
        ));
    }

    #[test]
    fn contains_and_joins() {
        let folder = FolderPath::parse("py-orders").unwrap();
        let inside = RepoPath::new("py-orders/src/a.py").unwrap();
        let sibling = RepoPath::new("py-orders-2/a.py").unwrap();
        assert!(folder.contains(&inside));
        assert!(!folder.contains(&sibling));
        assert!(FolderPath::ROOT.contains(&sibling));
        assert_eq!(
            folder.join(&FolderPath::parse("src").unwrap()).as_str(),
            "py-orders/src"
        );
        assert_eq!(folder.join(&FolderPath::ROOT).as_str(), "py-orders");
        assert_eq!(FolderPath::ROOT.join(&folder).as_str(), "py-orders");
        assert_eq!(folder.depth(), 1);
        assert_eq!(FolderPath::ROOT.depth(), 0);
    }
}
