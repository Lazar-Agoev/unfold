//! Verified source (architecture §10.6, decision U-17).
//!
//! Before any stored range is shown, the file's bytes are read and hashed, and the hash is
//! compared with the hash the facts came from. Only when they match are those same bytes used,
//! decoded strictly as UTF-8, and sliced. Otherwise the reader reports that the source changed
//! or is unavailable, without text: old ranges are never applied to new bytes.

use std::path::Path;

use serde::Serialize;
use unfold_core::{ContentHash, RepoPath, Snapshot};

/// What the file check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// The bytes on disk are the ones the facts came from.
    Current(Snapshot),
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
        Ok(snapshot) if snapshot.hash() == expected => Verified::Current(snapshot),
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
        let Verified::Current(snapshot) = verify_file(dir.path(), &path, Some(hash)) else {
            panic!("expected current source");
        };
        assert_eq!(snapshot.text(), "let a = 1;\n");

        std::fs::write(dir.path().join("src/a.ts"), "let a = 2;\n").unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Changed);

        std::fs::write(dir.path().join("src/a.ts"), [0xff, 0xfe]).unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Changed);

        std::fs::remove_file(dir.path().join("src/a.ts")).unwrap();
        assert_eq!(verify_file(dir.path(), &path, Some(hash)), Verified::Missing);
        assert_eq!(verify_file(dir.path(), &path, None), Verified::Missing);
        assert_eq!(Verified::Missing.state(), SourceState::Missing);
    }
}
