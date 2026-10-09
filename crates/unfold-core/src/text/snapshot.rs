//! A file's bytes, read once, with their hash and strictly decoded text.

use std::sync::Arc;

use super::hash::ContentHash;
use super::lines::LineIndex;
use super::utf16::Utf16Index;
use crate::path::RepoPath;

/// Why bytes cannot become a [`Snapshot`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// The file is larger than byte offsets can address.
    #[error("{path}: file is {len} bytes, more than the 4 GiB limit")]
    TooLarge {
        /// The file.
        path: RepoPath,
        /// Its size in bytes.
        len: u64,
    },
    /// The bytes are not valid UTF-8.
    #[error("{path}: invalid UTF-8 at byte {offset}")]
    InvalidUtf8 {
        /// The file.
        path: RepoPath,
        /// The offset of the first invalid byte.
        offset: u32,
    },
}

/// One read of a file: its path, its SHA-256, and its text.
///
/// The text is the bytes decoded strictly as UTF-8, so the bytes and the text are the same
/// sequence: a byte-order mark stays as U+FEFF, and line endings and Unicode normalization are
/// preserved. The hash covers those original bytes. Cloning is cheap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    path: RepoPath,
    hash: ContentHash,
    text: Arc<str>,
}

impl Snapshot {
    /// Hashes and decodes `bytes`, read from `path`.
    pub fn new(path: RepoPath, bytes: Vec<u8>) -> Result<Self, SnapshotError> {
        if u32::try_from(bytes.len()).is_err() {
            let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            return Err(SnapshotError::TooLarge { path, len });
        }
        let hash = ContentHash::of(&bytes);
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Self {
                path,
                hash,
                text: Arc::from(text),
            }),
            Err(error) => {
                // The length fits in u32 (checked above), so every offset does too.
                let offset = u32::try_from(error.utf8_error().valid_up_to()).unwrap_or(u32::MAX);
                Err(SnapshotError::InvalidUtf8 { path, offset })
            }
        }
    }

    /// A snapshot of text already known to be valid, such as a test fixture.
    pub fn from_text(path: RepoPath, text: &str) -> Result<Self, SnapshotError> {
        Self::new(path, text.as_bytes().to_vec())
    }

    /// The file's folder-relative path.
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// The SHA-256 of the original bytes.
    pub fn hash(&self) -> ContentHash {
        self.hash
    }

    /// The decoded text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The original bytes (identical to the text's UTF-8 encoding).
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// The length in bytes. It always fits in `u32`.
    pub fn len(&self) -> u32 {
        u32::try_from(self.text.len()).unwrap_or(u32::MAX)
    }

    /// Whether the file is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Builds the text's line index.
    pub fn line_index(&self) -> LineIndex {
        LineIndex::new(&self.text)
    }

    /// Builds the text's UTF-8 to UTF-16 offset index.
    pub fn utf16_index(&self) -> Utf16Index {
        Utf16Index::new(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> RepoPath {
        RepoPath::new("src/a.ts").unwrap()
    }

    #[test]
    fn keeps_bom_and_line_endings() {
        let bytes = b"\xEF\xBB\xBFlet a;\r\n".to_vec();
        let snapshot = Snapshot::new(path(), bytes.clone()).unwrap();
        assert_eq!(snapshot.text(), "\u{feff}let a;\r\n");
        assert_eq!(snapshot.bytes(), bytes.as_slice());
        assert_eq!(snapshot.hash(), ContentHash::of(&bytes));
        assert_eq!(snapshot.len(), 11);
    }

    #[test]
    fn rejects_invalid_utf8_with_its_offset() {
        let error = Snapshot::new(path(), b"ok\xFFno".to_vec()).unwrap_err();
        assert_eq!(
            error,
            SnapshotError::InvalidUtf8 {
                path: path(),
                offset: 2
            }
        );
        assert_eq!(error.to_string(), "src/a.ts: invalid UTF-8 at byte 2");
        // A lone surrogate encoded in UTF-8 (CESU) is invalid too.
        assert!(Snapshot::new(path(), b"\xED\xA0\x80".to_vec()).is_err());
    }
}
