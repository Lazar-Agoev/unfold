//! The engine's per-file facts cache, backed by the store's `file_facts` table (architecture
//! §8.4).
//!
//! # Why one connection behind a mutex
//!
//! The cache shares the folder's single [`Writer`] (behind `Arc<Mutex<_>>`) with the engine
//! thread's commits rather than opening a second read-write connection:
//!
//! - The engine calls the cache from its update, which runs on the engine thread's behalf, and
//!   the commit follows the update on the same thread. The two never overlap, so the mutex is
//!   never contended; it only proves exclusive access to the compiler.
//! - "One writer per folder" (docs/ipc.md#the-addons-surface) stays literally true. A second writer would
//!   compete for SQLite's write lock with the commit (`SQLITE_BUSY` handling, busy timeouts)
//!   for no benefit.
//! - The writer prunes each path to its current and last good entries under its own engine
//!   version, which is exactly the contract of [`FactsCache`].
//!
//! The store's database is recreated whenever its version string changes, and the workspace
//! builds that string from [`unfold_engine::ENGINE_VERSION`] and every front end's producer
//! version. So every stored entry belongs to the running engine; a key for another engine
//! version is a miss.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use unfold_core::{ContentHash, RepoPath};
use unfold_engine::ENGINE_VERSION;
use unfold_engine::settings::SettingsHash;
use unfold_engine::update::{CacheEntry, CacheError, CacheStatus, FactsCache, FactsKey, NewCacheEntry};
use unfold_store::{FactsStatus, NewFacts, Writer};

/// A [`FactsCache`] over the folder's store writer. See the [module documentation](self).
#[derive(Debug, Clone)]
pub struct StoreFactsCache {
    writer: Arc<Mutex<Writer>>,
}

impl StoreFactsCache {
    /// A cache over `writer`.
    pub fn new(writer: Arc<Mutex<Writer>>) -> Self {
        Self { writer }
    }

    fn lock(&self) -> MutexGuard<'_, Writer> {
        // The writer rolls back an interrupted transaction itself, so a poisoned lock is usable.
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn settings_hash(hash: SettingsHash) -> ContentHash {
    ContentHash::from_bytes(*hash.as_bytes())
}

fn cache_error(error: unfold_store::StoreError) -> CacheError {
    CacheError(error.to_string().into())
}

impl FactsCache for StoreFactsCache {
    fn get(&self, keys: &[FactsKey<'_>]) -> Result<Vec<Option<CacheEntry>>, CacheError> {
        let writer = self.lock();
        keys.iter()
            .map(|key| {
                if key.engine != ENGINE_VERSION {
                    return Ok(None);
                }
                let found = writer
                    .facts(key.path, key.hash, settings_hash(key.settings))
                    .map_err(cache_error)?;
                Ok(found.map(|entry| CacheEntry {
                    status: match entry.status {
                        FactsStatus::Parsed => CacheStatus::Parsed,
                        FactsStatus::Failed => CacheStatus::Failed,
                    },
                    bytes: entry.facts,
                }))
            })
            .collect()
    }

    fn last_good(
        &self,
        path: &RepoPath,
        engine: &'static str,
        settings: SettingsHash,
    ) -> Result<Option<(ContentHash, Vec<u8>)>, CacheError> {
        if engine != ENGINE_VERSION {
            return Ok(None);
        }
        let found = self
            .lock()
            .last_good_facts(path, settings_hash(settings))
            .map_err(cache_error)?;
        Ok(found.map(|entry| (entry.hash, entry.facts)))
    }

    fn put(&self, entries: &[NewCacheEntry<'_>]) -> Result<(), CacheError> {
        let facts: Vec<NewFacts<'_>> = entries
            .iter()
            .filter(|entry| entry.key.engine == ENGINE_VERSION)
            .map(|entry| NewFacts {
                path: entry.key.path,
                hash: entry.key.hash,
                settings_hash: settings_hash(entry.key.settings),
                status: match entry.status {
                    CacheStatus::Parsed => FactsStatus::Parsed,
                    CacheStatus::Failed => FactsStatus::Failed,
                },
                facts: entry.bytes,
            })
            .collect();
        if facts.is_empty() {
            return Ok(());
        }
        self.lock().put_facts(&facts).map_err(cache_error)
    }
}

#[cfg(test)]
mod tests {
    use unfold_engine::settings::SettingsHasher;
    use unfold_store::{Store, StoreConfig};

    use super::*;

    #[test]
    fn keeps_current_and_last_good_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("test")).expect("open");
        let cache = StoreFactsCache::new(Arc::new(Mutex::new(store.writer)));
        let path = RepoPath::new("a.ts").expect("path");
        let settings = SettingsHasher::new("test").finish();
        let key = |bytes: &[u8]| FactsKey {
            path: &path,
            hash: ContentHash::of(bytes),
            engine: ENGINE_VERSION,
            settings,
        };
        let (good, bad) = (key(b"good"), key(b"bad"));
        cache
            .put(&[NewCacheEntry {
                key: good,
                status: CacheStatus::Parsed,
                bytes: b"facts",
            }])
            .expect("put");
        cache
            .put(&[NewCacheEntry {
                key: bad,
                status: CacheStatus::Failed,
                bytes: b"oops",
            }])
            .expect("put");
        let found = cache.get(&[good, bad]).expect("get");
        assert_eq!(
            found[0].as_ref().map(|entry| entry.status),
            Some(CacheStatus::Parsed)
        );
        assert_eq!(
            found[1].as_ref().map(|entry| entry.status),
            Some(CacheStatus::Failed)
        );
        assert_eq!(
            cache
                .last_good(&path, ENGINE_VERSION, settings)
                .expect("last good"),
            Some((good.hash, b"facts".to_vec()))
        );
        let other = FactsKey {
            engine: "another-engine",
            ..good
        };
        assert_eq!(cache.get(&[other]).expect("get"), vec![None]);
    }
}
