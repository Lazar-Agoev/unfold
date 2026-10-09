//! Folder-relative paths.

use std::borrow::Borrow;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why a string is not a valid [`RepoPath`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The path is empty.
    #[error("path is empty")]
    Empty,
    /// The path contains a backslash; separators are `/`.
    #[error("path `{0}` contains a backslash")]
    Backslash(Box<str>),
    /// The path contains a NUL character.
    #[error("path contains a NUL character")]
    Nul,
    /// The path starts with a drive prefix such as `C:`.
    #[error("path `{0}` has a drive prefix")]
    Drive(Box<str>),
    /// The path is absolute, has an empty component (`a//b`, a trailing `/`), or a `.` or `..`
    /// component.
    #[error("path `{0}` has an empty, `.`, or `..` component")]
    Component(Box<str>),
}

/// A validated path relative to the open folder: nonempty, `/`-separated, case preserved, and
/// never escaping the root. It contains no empty, `.`, or `..` component, no backslash, no NUL,
/// and no drive or absolute prefix.
///
/// Cloning is cheap (a reference count). Paths order by their UTF-8 bytes, which is the order
/// files are processed and exported in.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(type = "string"))]
pub struct RepoPath(Arc<str>);

impl RepoPath {
    /// Validates `path`.
    pub fn new(path: &str) -> Result<Self, PathError> {
        validate(path)?;
        Ok(Self(Arc::from(path)))
    }

    /// The path as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The final component, such as `orders.ts`.
    pub fn file_name(&self) -> &str {
        self.0.rsplit_once('/').map_or(&self.0, |(_, name)| name)
    }

    /// The final component's extension without the dot, if it has one (`.bashrc` has none).
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name();
        match name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => Some(extension),
            _ => None,
        }
    }

    /// The containing folder's path, or `None` for a file at the root.
    pub fn parent(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(parent, _)| parent)
    }
}

fn validate(path: &str) -> Result<(), PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    if path.contains('\0') {
        return Err(PathError::Nul);
    }
    if path.contains('\\') {
        return Err(PathError::Backslash(path.into()));
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(PathError::Drive(path.into()));
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(PathError::Component(path.into()));
    }
    Ok(())
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RepoPath {
    type Err = PathError;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        Self::new(path)
    }
}

impl TryFrom<&str> for RepoPath {
    type Error = PathError;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for RepoPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Serialize for RepoPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Self::new(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_folder_relative_paths() {
        for path in [
            "a",
            "src/routes/orders.ts",
            "Ünïcode dir/File Name.PY",
            "a/.hidden",
            "a/b..c",
        ] {
            assert_eq!(RepoPath::new(path).unwrap().as_str(), path);
        }
    }

    #[test]
    fn rejects_paths_that_escape_or_are_malformed() {
        assert_eq!(RepoPath::new(""), Err(PathError::Empty));
        assert_eq!(RepoPath::new("a\0b"), Err(PathError::Nul));
        assert!(matches!(RepoPath::new("a\\b"), Err(PathError::Backslash(_))));
        assert!(matches!(RepoPath::new("C:/x"), Err(PathError::Drive(_))));
        for path in ["/abs", "a//b", "a/", "./a", "a/./b", "../a", "a/..", "."] {
            assert!(
                matches!(RepoPath::new(path), Err(PathError::Component(_))),
                "{path}"
            );
        }
    }

    #[test]
    fn orders_by_bytes_and_preserves_case() {
        let mut paths: Vec<RepoPath> = ["b.ts", "B.ts", "a/z.ts", "a.ts", "a-b.ts"]
            .iter()
            .map(|p| RepoPath::new(p).unwrap())
            .collect();
        paths.sort();
        let sorted: Vec<&str> = paths.iter().map(RepoPath::as_str).collect();
        assert_eq!(sorted, ["B.ts", "a-b.ts", "a.ts", "a/z.ts", "b.ts"]);
    }

    #[test]
    fn exposes_name_extension_and_parent() {
        let path = RepoPath::new("src/lib/validate.test.ts").unwrap();
        assert_eq!(path.file_name(), "validate.test.ts");
        assert_eq!(path.extension(), Some("ts"));
        assert_eq!(path.parent(), Some("src/lib"));
        let root = RepoPath::new(".bashrc").unwrap();
        assert_eq!(root.extension(), None);
        assert_eq!(root.parent(), None);
    }

    #[test]
    fn serde_validates() {
        let path: RepoPath = serde_json::from_str("\"src/a.ts\"").unwrap();
        assert_eq!(serde_json::to_string(&path).unwrap(), "\"src/a.ts\"");
        assert!(serde_json::from_str::<RepoPath>("\"../a.ts\"").is_err());
    }
}
