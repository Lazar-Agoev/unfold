//! The per-file facts cache contract (docs/engine.md#facts-cache-and-failures) and an in-memory implementation.
//!
//! Entries are serialized [`FileFacts`] (or a serialized failure) keyed by path, content hash,
//! engine version, and the file's settings hash. For each path a cache keeps at most two
//! entries: the **current** one (put last) and the **last good** one (the most recent parsed
//! entry, when the current one failed). Putting a parsed entry drops every other entry for its
//! path; putting a failed entry drops other failed entries and entries for another engine or
//! settings hash, and keeps the parsed one. The store's `file_facts` table follows the same
//! rules, so the workspace can back this trait with it.
//!
//! The engine treats a cache as an accelerator: a read error or an entry that doesn't decode is
//! a miss, and so is one whose payload its language can't read ([`decode_facts`]). A failed put
//! is reported as a problem, never as a failed update.

use std::sync::Mutex;

use rustc_hash::FxHashMap;
use unfold_core::{ContentHash, RepoPath};

use crate::facts::{FactsError, FileFacts};
use crate::language::{Language, SourceFailure};
use crate::settings::SettingsHash;

/// The key of one cache entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FactsKey<'a> {
    /// The file.
    pub path: &'a RepoPath,
    /// The SHA-256 of the bytes the entry came from.
    pub hash: ContentHash,
    /// The engine that produced it ([`crate::ENGINE_VERSION`]).
    pub engine: &'static str,
    /// The fingerprint of the settings that shaped it ([`crate::Language::settings_hash`]).
    pub settings: SettingsHash,
}

/// Whether an entry's bytes parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheStatus {
    /// The entry holds [`FileFacts::to_bytes`].
    Parsed,
    /// The bytes didn't parse; the entry holds the failure ([`encode_failure`]).
    Failed,
}

/// A cached entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    /// Whether the bytes parsed.
    pub status: CacheStatus,
    /// The serialized facts or failure.
    pub bytes: Vec<u8>,
}

/// An entry to put.
#[derive(Debug, Clone, Copy)]
pub struct NewCacheEntry<'a> {
    /// Its key.
    pub key: FactsKey<'a>,
    /// Whether the bytes parsed.
    pub status: CacheStatus,
    /// The serialized facts or failure.
    pub bytes: &'a [u8],
}

/// A cache that could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("facts cache: {0}")]
pub struct CacheError(pub Box<str>);

/// Serialized per-file facts by path, content hash, engine version, and settings hash.
///
/// The engine calls it from its update thread only, in batches, so an implementation over a
/// single database connection needs no more than a mutex.
pub trait FactsCache: Send + Sync {
    /// The entries for exactly these keys, aligned with `keys` (`None` for a miss).
    fn get(&self, keys: &[FactsKey<'_>]) -> Result<Vec<Option<CacheEntry>>, CacheError>;

    /// The most recent parsed entry for `path` under this engine and settings hash, with the
    /// hash of the bytes it came from: what a file that stopped parsing falls back to.
    fn last_good(
        &self,
        path: &RepoPath,
        engine: &'static str,
        settings: SettingsHash,
    ) -> Result<Option<(ContentHash, Vec<u8>)>, CacheError>;

    /// Stores `entries`, keeping each path's current and last good entries only (see the
    /// [module documentation](self)).
    fn put(&self, entries: &[NewCacheEntry<'_>]) -> Result<(), CacheError>;
}

/// Encodes a failure for a [`CacheStatus::Failed`] entry.
pub fn encode_failure(failure: &SourceFailure) -> Result<Vec<u8>, FactsError> {
    postcard::to_allocvec(&(&*failure.message, failure.offset))
        .map_err(|error| FactsError::Encode(error.to_string()))
}

/// Decodes a [`CacheStatus::Failed`] entry.
pub fn decode_failure(bytes: &[u8]) -> Result<SourceFailure, FactsError> {
    let (message, offset): (String, Option<u32>) =
        postcard::from_bytes(bytes).map_err(|error| FactsError::Decode(error.to_string()))?;
    Ok(SourceFailure {
        message: message.into(),
        offset,
    })
}

/// Decodes a [`CacheStatus::Parsed`] entry of `language`, re-checking its invariants and that the
/// language reads its payload ([`Language::reads_payload`]).
pub fn decode_facts(bytes: &[u8], language: &dyn Language) -> Result<FileFacts, FactsError> {
    let facts = FileFacts::from_bytes(bytes)?;
    if !language.reads_payload(&facts.payload) {
        return Err(FactsError::Payload);
    }
    Ok(facts)
}

#[derive(Debug, Clone)]
struct Stored {
    hash: ContentHash,
    engine: &'static str,
    settings: SettingsHash,
    status: CacheStatus,
    bytes: Vec<u8>,
}

/// An in-memory [`FactsCache`] with the store's pruning rules, for tests, headless tools, and
/// any session without a database.
#[derive(Debug, Default)]
pub struct MemoryFactsCache {
    entries: Mutex<FxHashMap<RepoPath, Vec<Stored>>>,
}

impl MemoryFactsCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of entries held.
    pub fn len(&self) -> usize {
        self.lock().values().map(Vec::len).sum()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FxHashMap<RepoPath, Vec<Stored>>> {
        // A panic while holding the lock leaves plain data behind; keep using it.
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl FactsCache for MemoryFactsCache {
    fn get(&self, keys: &[FactsKey<'_>]) -> Result<Vec<Option<CacheEntry>>, CacheError> {
        let entries = self.lock();
        Ok(keys
            .iter()
            .map(|key| {
                entries.get(key.path)?.iter().find_map(|stored| {
                    (stored.hash == key.hash
                        && stored.engine == key.engine
                        && stored.settings == key.settings)
                        .then(|| CacheEntry {
                            status: stored.status,
                            bytes: stored.bytes.clone(),
                        })
                })
            })
            .collect())
    }

    fn last_good(
        &self,
        path: &RepoPath,
        engine: &'static str,
        settings: SettingsHash,
    ) -> Result<Option<(ContentHash, Vec<u8>)>, CacheError> {
        let entries = self.lock();
        Ok(entries.get(path).and_then(|stored| {
            stored.iter().find_map(|entry| {
                (entry.status == CacheStatus::Parsed && entry.engine == engine && entry.settings == settings)
                    .then(|| (entry.hash, entry.bytes.clone()))
            })
        }))
    }

    fn put(&self, new: &[NewCacheEntry<'_>]) -> Result<(), CacheError> {
        let mut entries = self.lock();
        for entry in new {
            let key = entry.key;
            let stored = entries.entry(key.path.clone()).or_default();
            let same = |stored: &Stored| {
                stored.hash == key.hash && stored.engine == key.engine && stored.settings == key.settings
            };
            match entry.status {
                CacheStatus::Parsed => stored.clear(),
                CacheStatus::Failed => stored.retain(|stored| {
                    !same(stored)
                        && stored.status == CacheStatus::Parsed
                        && stored.engine == key.engine
                        && stored.settings == key.settings
                }),
            }
            stored.push(Stored {
                hash: key.hash,
                engine: key.engine,
                settings: key.settings,
                status: entry.status,
                bytes: entry.bytes.to_vec(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsHasher;

    fn key<'a>(path: &'a RepoPath, bytes: &[u8]) -> FactsKey<'a> {
        FactsKey {
            path,
            hash: ContentHash::of(bytes),
            engine: crate::ENGINE_VERSION,
            settings: SettingsHasher::new("test").finish(),
        }
    }

    #[test]
    fn keeps_the_current_and_last_good_entries() {
        let cache = MemoryFactsCache::new();
        let path = RepoPath::new("a.ts").unwrap();
        let (good, bad, worse) = (key(&path, b"good"), key(&path, b"bad"), key(&path, b"worse"));
        let put = |key, status, bytes: &[u8]| {
            cache.put(&[NewCacheEntry { key, status, bytes }]).unwrap();
        };
        put(good, CacheStatus::Parsed, b"facts");
        put(bad, CacheStatus::Failed, b"oops");
        put(worse, CacheStatus::Failed, b"oops2");
        assert_eq!(cache.len(), 2);
        let found = cache.get(&[good, bad, worse]).unwrap();
        assert_eq!(
            found[0].as_ref().map(|entry| entry.status),
            Some(CacheStatus::Parsed)
        );
        assert!(found[1].is_none());
        assert_eq!(
            found[2].as_ref().map(|entry| entry.status),
            Some(CacheStatus::Failed)
        );
        let last = cache.last_good(&path, good.engine, good.settings).unwrap();
        assert_eq!(last, Some((good.hash, b"facts".to_vec())));
        put(bad, CacheStatus::Parsed, b"fixed");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn failures_round_trip() {
        let failure = SourceFailure {
            message: "unexpected end".into(),
            offset: Some(3),
        };
        let bytes = encode_failure(&failure).unwrap();
        assert_eq!(decode_failure(&bytes).unwrap(), failure);
        assert!(decode_failure(&[0xff]).is_err());
    }
}
