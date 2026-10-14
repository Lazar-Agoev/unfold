//! Verified source (`docs/architecture.md`, "Guarantees": source is verified against its hash
//! before it is shown or signed).
//!
//! Before any stored range is shown, the file's bytes are read and hashed, and the hash is
//! compared with the hash the facts came from. Only when they match are those same bytes used,
//! decoded strictly as UTF-8, and sliced. Otherwise the reader reports that the source changed
//! or is unavailable, without text: old ranges are never applied to new bytes.
//!
//! Stored ranges are UTF-8 byte ranges (the engine's unit); what is rendered uses UTF-16
//! offsets (the UI's unit). [`Source`] converts between them for one verified file.

use std::path::Path;

use serde::Serialize;
use unfold_core::text::OffsetError;
use unfold_core::{
    ByteRange, ContentHash, LineIndex, LineSpan, RepoPath, Snapshot, Utf16Index, text::Utf16Range,
};

use crate::error::{ReaderError, Result};

/// A file's text, verified against the hash its facts came from, with its line and UTF-16
/// indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    snapshot: Snapshot,
    lines: LineIndex,
    utf16: Utf16Index,
}

impl Source {
    fn new(snapshot: Snapshot) -> Self {
        Self {
            lines: snapshot.line_index(),
            utf16: snapshot.utf16_index(),
            snapshot,
        }
    }

    /// The file's path.
    pub fn path(&self) -> &RepoPath {
        self.snapshot.path()
    }

    /// The SHA-256 of the bytes.
    pub fn hash(&self) -> ContentHash {
        self.snapshot.hash()
    }

    /// The whole text.
    pub fn text(&self) -> &str {
        self.snapshot.text()
    }

    /// The bytes of `range`, when it fits the text.
    pub fn bytes(&self, range: ByteRange) -> Option<&[u8]> {
        self.snapshot.bytes().get(range.as_usize())
    }

    /// The line index.
    pub fn lines(&self) -> &LineIndex {
        &self.lines
    }

    /// The text of stored range `range`. A range that doesn't fit the verified text (or splits
    /// a character) means the store and the bytes it hashed disagree.
    pub fn slice(&self, range: ByteRange) -> Result<&str> {
        self.text()
            .get(range.as_usize())
            .ok_or_else(|| self.invalid(range))
    }

    /// The lines stored range `range` starts and ends on.
    pub fn line_span(&self, range: ByteRange) -> Result<LineSpan> {
        if range.start > range.end || range.end > self.snapshot.len() {
            return Err(self.invalid(range));
        }
        Ok(self.lines.span(range))
    }

    /// Stored range `range` in UTF-16 code units, for rendering.
    pub fn to_utf16(&self, range: ByteRange) -> Result<Utf16Range> {
        self.utf16.range_to_utf16(range).map_err(|_| self.invalid(range))
    }

    /// A rendered UTF-16 range as a byte range of the text.
    pub fn to_utf8(&self, range: Utf16Range) -> Result<ByteRange, OffsetError> {
        self.utf16.range_to_utf8(range)
    }

    fn invalid(&self, range: ByteRange) -> ReaderError {
        ReaderError::InvalidRange {
            path: self.path().as_str().into(),
            start: range.start,
            end: range.end,
        }
    }
}

/// What the file check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// The bytes on disk are the ones the facts came from.
    Current(Source),
    /// The bytes differ from the facts' (the file is pending or shows its last good version).
    Changed,
    /// The file is missing or unreadable, or there are no facts to compare with.
    Missing,
}

impl Verified {
    /// The state without the text.
    pub fn state(&self) -> SourceState {
        match self {
            Self::Current(_) => SourceState::Current,
            Self::Changed => SourceState::Changed,
            Self::Missing => SourceState::Missing,
        }
    }

    /// The verified source, when current.
    pub fn source(&self) -> Option<&Source> {
        match self {
            Self::Current(source) => Some(source),
            Self::Changed | Self::Missing => None,
        }
    }

    /// The verified source, when current, taken.
    pub fn into_source(self) -> Option<Source> {
        match self {
            Self::Current(source) => Some(source),
            Self::Changed | Self::Missing => None,
        }
    }
}

/// The freshness of a file's source, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum SourceState {
    /// The source matches the facts.
    Current,
    /// The source changed since the facts were taken.
    Changed,
    /// The source is missing or unreadable.
    Missing,
}

/// Reads `path` under `root` and verifies it against `expected`, the hash its facts came from
/// (`None` for a failed file, which has no facts).
pub fn verify_file(root: &Path, path: &RepoPath, expected: Option<ContentHash>) -> Verified {
    let Some(expected) = expected else {
        return Verified::Missing;
    };
    let Ok(bytes) = std::fs::read(root.join(path.as_str())) else {
        return Verified::Missing;
    };
    // `Snapshot::new` hashes the bytes and then decodes those same bytes; the text is used only
    // after the hash matches. Bytes that are not UTF-8 (or too large) cannot be the bytes the
    // facts came from, which decoded, so they count as changed.
    match Snapshot::new(path.clone(), bytes) {
        Ok(snapshot) if snapshot.hash() == expected => Verified::Current(Source::new(snapshot)),
        _ => Verified::Changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_matching_changed_and_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepoPath::new("src/a.ts").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.ts"), "let a = 1;\n").unwrap();
        let hash = ContentHash::of(b"let a = 1;\n");
        let Verified::Current(source) = verify_file(dir.path(), &path, Some(hash)) else {
            panic!("expected current source");
        };
        assert_eq!(source.text(), "let a = 1;\n");

        std::fs::write(dir.path().join("src/a.ts"), "let a = 2;\n").unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Changed);

        std::fs::write(dir.path().join("src/a.ts"), [0xff, 0xfe]).unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Changed);

        std::fs::remove_file(dir.path().join("src/a.ts")).unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Missing);
        assert_eq!(verify_file(dir.path(), &path, None), Verified::Missing);
        assert_eq!(Verified::Missing.state(), SourceState::Missing);
        assert!(Verified::Changed.source().is_none());
    }

    #[test]
    fn ranges_convert_between_bytes_and_utf16() {
        let dir = tempfile::tempdir().unwrap();
        let text = "a = \"é😀\"\nb = 1\n";
        std::fs::write(dir.path().join("m.py"), text).unwrap();
        let path = RepoPath::new("m.py").unwrap();
        let Verified::Current(source) =
            verify_file(dir.path(), &path, Some(ContentHash::of(text.as_bytes())))
        else {
            panic!("expected current source");
        };
        // `b` starts after `a = "é😀"\n`: 13 bytes, but 10 UTF-16 units (é is 1, 😀 is 2).
        let b = ByteRange { start: 13, end: 14 };
        assert_eq!(source.slice(b).unwrap(), "b");
        assert_eq!(source.to_utf16(b).unwrap(), Utf16Range { start: 10, end: 11 });
        assert_eq!(source.to_utf8(Utf16Range { start: 10, end: 11 }).unwrap(), b);
        assert_eq!(source.line_span(b).unwrap(), LineSpan { start: 2, end: 2 });
        // A range inside 😀, or past the end, never fits.
        let inside = ByteRange { start: 7, end: 8 };
        assert!(matches!(
            source.slice(inside),
            Err(ReaderError::InvalidRange { .. })
        ));
        assert!(source.to_utf16(inside).is_err());
        assert!(source.line_span(ByteRange { start: 0, end: 99 }).is_err());
    }
}
