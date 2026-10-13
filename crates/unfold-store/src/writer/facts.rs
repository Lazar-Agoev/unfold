//! The per-file facts cache (docs/engine.md#facts-cache-and-failures): serialized `FileFacts` keyed by path, content
//! hash, engine version, and settings hash.
//!
//! The store treats facts as opaque bytes. Each entry also records whether the bytes parsed
//! ([`FactsStatus`]); it is stored as a one-byte prefix of the `facts` blob, which keeps the
//! table exactly as the architecture specifies. A failed entry lets the engine skip re-parsing
//! a file whose broken bytes it has already seen.
//!
//! For each path the cache keeps at most two entries, both for the running engine and the
//! file's current settings:
//!
//! - the **current** entry, the one put last; and
//! - the **last good** entry, the most recent parsed one, when the current entry failed.
//!
//! Putting a parsed entry therefore deletes every other entry for its path (it is both current
//! and last good). Putting a failed entry deletes every other failed entry and every entry for
//! another engine or settings hash, and keeps the parsed one. With that invariant no
//! timestamps are needed.

use rusqlite::{OptionalExtension, Row};
use unfold_core::{ContentHash, RepoPath};

use super::Writer;
use crate::codec;
use crate::error::Result;

/// Whether an entry's bytes parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactsStatus {
    /// The facts come from bytes that parsed.
    Parsed,
    /// The bytes did not parse; the facts describe the failure.
    Failed,
}

impl FactsStatus {
    const fn tag(self) -> u8 {
        match self {
            Self::Parsed => 1,
            Self::Failed => 0,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Parsed),
            0 => Some(Self::Failed),
            _ => None,
        }
    }
}

/// An entry to put into the cache. The engine version is the store's.
#[derive(Debug, Clone, Copy)]
pub struct NewFacts<'a> {
    /// The file.
    pub path: &'a RepoPath,
    /// The SHA-256 of the bytes the facts came from.
    pub hash: ContentHash,
    /// The fingerprint of the file's language settings.
    pub settings_hash: ContentHash,
    /// Whether the bytes parsed.
    pub status: FactsStatus,
    /// The serialized facts.
    pub facts: &'a [u8],
}

/// A cached entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFacts {
    /// The file.
    pub path: RepoPath,
    /// The SHA-256 of the bytes the facts came from.
    pub hash: ContentHash,
    /// The fingerprint of the file's language settings.
    pub settings_hash: ContentHash,
    /// Whether the bytes parsed.
    pub status: FactsStatus,
    /// The serialized facts.
    pub facts: Vec<u8>,
}

const COLUMNS: &str = "ff.path, ff.hash, ff.settings_hash, ff.facts";

/// Decodes [`COLUMNS`].
fn decode(row: &Row<'_>) -> Result<CachedFacts> {
    let blob: &[u8] = row
        .get_ref(3)?
        .as_blob()
        .map_err(|_| codec::invalid("facts", "not a blob"))?;
    let (&tag, facts) = blob
        .split_first()
        .ok_or_else(|| codec::invalid("facts", "empty blob"))?;
    let status = FactsStatus::from_tag(tag).ok_or_else(|| codec::invalid("facts status", tag.to_string()))?;
    Ok(CachedFacts {
        path: codec::path(row, 0)?,
        hash: codec::hash(row, 1)?,
        settings_hash: codec::hash(row, 2)?,
        status,
        facts: facts.to_vec(),
    })
}

impl Writer {
    /// Puts `entries` in one transaction, pruning each path to its current and last good
    /// entries. Facts are independent of the graph, so they may be put before the graph that
    /// uses them commits (or even if that commit fails).
    pub fn put_facts(&mut self, entries: &[NewFacts<'_>]) -> Result<()> {
        self.recover()?;
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO file_facts (path, hash, engine, settings_hash, facts) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut prune_parsed = tx.prepare_cached(
                "DELETE FROM file_facts WHERE path = ?1 \
                 AND NOT (hash = ?2 AND engine = ?3 AND settings_hash = ?4)",
            )?;
            let mut prune_failed = tx.prepare_cached(
                "DELETE FROM file_facts WHERE path = ?1 \
                 AND NOT (hash = ?2 AND engine = ?3 AND settings_hash = ?4) \
                 AND (engine <> ?3 OR settings_hash <> ?4 OR substr(facts, 1, 1) = x'00')",
            )?;
            let mut blob = Vec::new();
            for entry in entries {
                blob.clear();
                blob.reserve(entry.facts.len() + 1);
                blob.push(entry.status.tag());
                blob.extend_from_slice(entry.facts);
                let key = (
                    entry.path.as_str(),
                    &entry.hash.as_bytes()[..],
                    &*self.engine_version,
                    &entry.settings_hash.as_bytes()[..],
                );
                insert.execute((key.0, key.1, key.2, key.3, &blob[..]))?;
                match entry.status {
                    FactsStatus::Parsed => prune_parsed.execute(key)?,
                    FactsStatus::Failed => prune_failed.execute(key)?,
                };
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The entry for exactly these bytes and settings, under the running engine.
    pub fn facts(
        &self,
        path: &RepoPath,
        hash: ContentHash,
        settings_hash: ContentHash,
    ) -> Result<Option<CachedFacts>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM file_facts ff \
             WHERE ff.path = ?1 AND ff.hash = ?2 AND ff.engine = ?3 AND ff.settings_hash = ?4"
        ))?;
        let mut rows = statement.query((
            path.as_str(),
            &hash.as_bytes()[..],
            &*self.engine_version,
            &settings_hash.as_bytes()[..],
        ))?;
        rows.next()?.map(decode).transpose()
    }

    /// The most recent parsed entry for `path` under the running engine and these settings:
    /// what a file that stopped parsing falls back to (docs/engine.md#facts-cache-and-failures).
    pub fn last_good_facts(
        &self,
        path: &RepoPath,
        settings_hash: ContentHash,
    ) -> Result<Option<CachedFacts>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM file_facts ff \
             WHERE ff.path = ?1 AND ff.engine = ?2 AND ff.settings_hash = ?3 \
             AND substr(ff.facts, 1, 1) = x'01'"
        ))?;
        let row = statement
            .query_row(
                (
                    path.as_str(),
                    &*self.engine_version,
                    &settings_hash.as_bytes()[..],
                ),
                |row| Ok(decode(row)),
            )
            .optional()?;
        row.transpose()
    }

    /// Every entry the stored graph's files were built from (hash equal to a file's facts
    /// hash) or last saw on disk (equal to its disk hash), under the running engine, in path
    /// order. On reopen the engine rebuilds its in-memory program from these without parsing.
    pub fn load_facts(&self) -> Result<Vec<CachedFacts>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM files f JOIN file_facts ff \
             ON ff.path = f.path AND ff.engine = ?1 AND (ff.hash = f.facts_hash OR ff.hash = f.disk_hash) \
             ORDER BY ff.path, ff.hash, ff.settings_hash"
        ))?;
        let mut rows = statement.query([&*self.engine_version])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(decode(row)?);
        }
        Ok(entries)
    }
}
