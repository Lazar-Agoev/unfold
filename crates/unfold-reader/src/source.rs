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
//!
//! A [`SourceCache`] keeps the sources verified most recently, by path and hash, so a drawing
//! keeps its words while a file changes on disk and the compile hasn't caught up, or while the
//! file doesn't parse (its last good facts keep the hash they came from). A source is kept only
//! after it verified, and found only by the hash the facts came from: its text is exactly the
//! facts' text.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use serde::Serialize;
use unfold_core::text::OffsetError;
use unfold_core::{
    ByteRange, ContentHash, LineBreaks, LineIndex, RepoPath, Snapshot, Utf16Index, text::Utf16Range,
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
    fn new(snapshot: Snapshot, breaks: LineBreaks) -> Self {
        Self {
            lines: snapshot.line_index(breaks),
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

/// How many bytes of text a [`SourceCache`] keeps before it forgets the sources used least
/// recently.
const KEPT_BYTES: usize = 32 << 20;

/// The sources verified most recently, by path and hash, within [`KEPT_BYTES`] of text.
#[derive(Debug)]
pub struct SourceCache {
    /// How many bytes of text it keeps.
    budget: usize,
    kept: Mutex<Kept>,
}

#[derive(Debug, Default)]
struct Kept {
    /// Each source with when it was last used.
    sources: FxHashMap<(RepoPath, ContentHash), (Arc<Source>, u64)>,
    bytes: usize,
    clock: u64,
}

/// A file checked against its facts' hash, with the text to show for it.
#[derive(Debug, Clone)]
pub struct Checked {
    /// Whether the bytes on disk are the facts'.
    pub state: SourceState,
    /// The facts' text: the file's when current, else the one kept from when it last verified.
    pub source: Option<Arc<Source>>,
}

impl SourceCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            budget: KEPT_BYTES,
            kept: Mutex::default(),
        }
    }

    /// Verifies `path` under `root` against `expected` ([`verify_file`]) and keeps its source
    /// when it is current. When the file changed or is gone, the source kept for `expected` is
    /// the text, if it is still kept.
    pub fn check(
        &self,
        root: &Path,
        path: &RepoPath,
        expected: Option<ContentHash>,
        breaks: LineBreaks,
    ) -> Checked {
        let verified = verify_file(root, path, expected, breaks);
        let state = verified.state();
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        let source = match (verified, expected) {
            (Verified::Current(source), _) => Some(kept.keep(source, self.budget)),
            (Verified::Changed | Verified::Missing, Some(hash)) => kept.get(path, hash),
            (Verified::Changed | Verified::Missing, None) => None,
        };
        Checked { state, source }
    }
}

impl Kept {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn get(&mut self, path: &RepoPath, hash: ContentHash) -> Option<Arc<Source>> {
        let now = self.tick();
        let (source, used) = self.sources.get_mut(&(path.clone(), hash))?;
        *used = now;
        Some(Arc::clone(source))
    }

    /// Keeps `source`, forgetting the sources used least recently past `budget` bytes (never
    /// the one just kept).
    fn keep(&mut self, source: Source, budget: usize) -> Arc<Source> {
        let now = self.tick();
        let key = (source.path().clone(), source.hash());
        if let Some((kept, used)) = self.sources.get_mut(&key) {
            *used = now;
            return Arc::clone(kept);
        }
        let source = Arc::new(source);
        self.bytes += source.text().len();
        self.sources.insert(key.clone(), (Arc::clone(&source), now));
        while self.bytes > budget {
            let Some(oldest) = self
                .sources
                .iter()
                .filter(|(other, _)| **other != key)
                .min_by_key(|(_, (_, used))| *used)
                .map(|(other, _)| other.clone())
            else {
                break;
            };
            if let Some((forgotten, _)) = self.sources.remove(&oldest) {
                self.bytes -= forgotten.text().len();
            }
        }
        source
    }
}

/// Reads `path` under `root` and verifies it against `expected`, the hash its facts came from
/// (`None` for a failed file, which has no facts). Its lines end at `breaks`: a source's
/// language's, [`LineBreaks::Newlines`] for any other file.
pub fn verify_file(
    root: &Path,
    path: &RepoPath,
    expected: Option<ContentHash>,
    breaks: LineBreaks,
) -> Verified {
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
        Ok(snapshot) if snapshot.hash() == expected => Verified::Current(Source::new(snapshot, breaks)),
        _ => Verified::Changed,
    }
}

impl Default for SourceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_keeps_the_text_a_file_had_when_it_verified() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepoPath::new("a.py").unwrap();
        let write = |text: &str| std::fs::write(dir.path().join("a.py"), text).unwrap();
        let first = ContentHash::of(b"a = 1\n");
        let cache = SourceCache {
            budget: 10,
            kept: Mutex::default(),
        };
        let check = |hash| cache.check(dir.path(), &path, Some(hash), LineBreaks::Newlines);
        let text = |checked: &Checked| checked.source.as_ref().map(|source| source.text().to_owned());

        // Nothing was verified yet, so a changed file has no text.
        write("a = 2\n");
        let unseen = check(first);
        assert_eq!((unseen.state, text(&unseen)), (SourceState::Changed, None));

        write("a = 1\n");
        let current = check(first);
        assert_eq!(current.state, SourceState::Current);
        write("a = 2\n");
        let changed = check(first);
        assert_eq!(
            (changed.state, text(&changed).as_deref()),
            (SourceState::Changed, Some("a = 1\n"))
        );
        std::fs::remove_file(dir.path().join("a.py")).unwrap();
        let missing = check(first);
        assert_eq!(
            (missing.state, text(&missing).as_deref()),
            (SourceState::Missing, Some("a = 1\n"))
        );

        // Past the budget, the source used least recently is forgotten.
        write("a = 2\n");
        let second = ContentHash::of(b"a = 2\n");
        assert_eq!(check(second).state, SourceState::Current);
        write("a = 3\n");
        assert_eq!(text(&check(second)).as_deref(), Some("a = 2\n"));
        assert_eq!(text(&check(first)), None);
    }

    #[test]
    fn verifies_matching_changed_and_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = RepoPath::new("src/a.ts").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.ts"), "let a = 1;\n").unwrap();
        let hash = ContentHash::of(b"let a = 1;\n");
        let Verified::Current(source) = verify_file(dir.path(), &path, Some(hash), LineBreaks::EcmaScript)
        else {
            panic!("expected current source");
        };
        assert_eq!(source.text(), "let a = 1;\n");

        std::fs::write(dir.path().join("src/a.ts"), "let a = 2;\n").unwrap();
        assert_eq!(
            verify_file(dir.path(), &path, Some(hash), LineBreaks::EcmaScript),
            Verified::Changed
        );

        std::fs::write(dir.path().join("src/a.ts"), [0xff, 0xfe]).unwrap();
        assert_eq!(
            verify_file(dir.path(), &path, Some(hash), LineBreaks::EcmaScript),
            Verified::Changed
        );

        std::fs::remove_file(dir.path().join("src/a.ts")).unwrap();
        assert_eq!(
            verify_file(dir.path(), &path, Some(hash), LineBreaks::EcmaScript),
            Verified::Missing
        );
        assert_eq!(
            verify_file(dir.path(), &path, None, LineBreaks::EcmaScript),
            Verified::Missing
        );
        assert_eq!(Verified::Missing.state(), SourceState::Missing);
        assert!(Verified::Changed.source().is_none());
    }

    #[test]
    fn ranges_convert_between_bytes_and_utf16() {
        let dir = tempfile::tempdir().unwrap();
        let text = "a = \"é😀\"\nb = 1\n";
        std::fs::write(dir.path().join("m.py"), text).unwrap();
        let path = RepoPath::new("m.py").unwrap();
        let Verified::Current(source) = verify_file(
            dir.path(),
            &path,
            Some(ContentHash::of(text.as_bytes())),
            LineBreaks::Newlines,
        ) else {
            panic!("expected current source");
        };
        // `b` starts after `a = "é😀"\n`: 13 bytes, but 10 UTF-16 units (é is 1, 😀 is 2).
        let b = ByteRange { start: 13, end: 14 };
        assert_eq!(source.slice(b).unwrap(), "b");
        assert_eq!(source.to_utf16(b).unwrap(), Utf16Range { start: 10, end: 11 });
        assert_eq!(source.to_utf8(Utf16Range { start: 10, end: 11 }).unwrap(), b);
        // A range inside 😀, or past the end, never fits.
        let inside = ByteRange { start: 7, end: 8 };
        assert!(matches!(
            source.slice(inside),
            Err(ReaderError::InvalidRange { .. })
        ));
        assert!(source.to_utf16(inside).is_err());
        assert!(source.slice(ByteRange { start: 0, end: 99 }).is_err());
    }
}
