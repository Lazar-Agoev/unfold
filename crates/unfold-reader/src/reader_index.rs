//! Graph-wide answers for requests: owners, callers, closures, and reached-from, from one
//! [`GraphIndex`] per graph version.
//!
//! Cards are served from the store: per-card data (blocks, slots, edges, definitions) comes from
//! per-file queries. What needs the whole graph (callers mapped to owners, activation closures,
//! reached-from) comes from a [`ReaderIndex`], loaded once from the store (or built from the
//! in-memory graph the workspace just committed) and shared through a [`ReaderCache`] until the
//! next commit invalidates it.
//!
//! Closures are cached per seed: the closure of a seed set is the union of its seeds'
//! closures, and many entrances share handlers, so reached-from for every definition costs one
//! walk per distinct seed in the life of the index.

use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use unfold_core::vocab::EntranceKind;
use unfold_core::{Graph, Key};
use unfold_store::{ReadPool, Reader};

use crate::applications::{entrance_seeds, keys};
use crate::closure::{closure, union};
use crate::error::Result;
use crate::index::{BlockId, GraphIndex, GraphIndexBuilder};
use crate::relations::{Caller, callers, reached_from};

/// The whole-graph structure of one graph version, with cached closures. Cheap to share
/// behind an [`Arc`]; every method takes `&self` and is safe to call from several threads.
#[derive(Debug)]
pub struct ReaderIndex {
    index: GraphIndex,
    seed_closures: Mutex<FxHashMap<BlockId, Arc<[BlockId]>>>,
}

impl ReaderIndex {
    /// Loads the committed graph's structure from the store, file by file, in one snapshot.
    pub fn load(reader: &Reader) -> Result<Self> {
        let index = reader.snapshot(|reader| {
            let mut builder = GraphIndexBuilder::new();
            for row in reader.files()? {
                if let Some(file) = reader.file_graph(row.path.as_str())? {
                    builder.add_file(&file);
                }
            }
            Ok(builder.finish(&reader.entrances()?))
        })?;
        Ok(Self::new(index))
    }

    /// Builds the structure from an in-memory graph, such as the one just committed.
    pub fn from_graph(graph: &Graph) -> Self {
        Self::new(GraphIndex::from_graph(graph))
    }

    fn new(index: GraphIndex) -> Self {
        Self {
            index,
            seed_closures: Mutex::new(FxHashMap::default()),
        }
    }

    /// The underlying index.
    pub fn index(&self) -> &GraphIndex {
        &self.index
    }

    /// Whether a block with `key` exists.
    pub fn contains(&self, key: &str) -> bool {
        self.index.id(key).is_some()
    }

    /// The function, class, or file whose activation the block with `key` belongs to.
    pub fn owner(&self, key: &str) -> Option<&Key> {
        let id = self.index.id(key)?;
        Some(self.index.key(self.index.owner(id)))
    }

    /// The resolved edges into the block with `key`, each with its occurrence's owner, in graph
    /// order. `None` when the block does not exist.
    pub fn callers(&self, key: &str) -> Option<Vec<Caller>> {
        self.index.id(key).map(|id| callers(&self.index, id))
    }

    /// The non-process entrances whose closures contain the block with `key`, in canonical
    /// order. `None` when the block does not exist.
    pub fn reached_from(&self, key: &str) -> Option<Vec<Key>> {
        let id = self.index.id(key)?;
        let entrances = self.index.entrances();
        Some(reached_from(&self.index, id, |position, id| {
            entrance_seeds(&self.index, &entrances[position])
                .into_iter()
                .any(|seed| self.seed_closure(seed).binary_search(&id).is_ok())
        }))
    }

    /// The activation closure of the blocks with `seeds`, as keys sorted by graph order. Unknown
    /// keys are ignored.
    pub fn closure(&self, seeds: &[&str]) -> Vec<Key> {
        let ids: Vec<BlockId> = seeds.iter().filter_map(|key| self.index.id(key)).collect();
        keys(&self.index, &self.closure_ids(&ids))
    }

    /// The activation closure of the entrance with `key`: a process entrance's application
    /// seeds, or every resolved target of any other entrance. `None` when the entrance does not
    /// exist.
    pub fn entrance_closure(&self, key: &str) -> Option<Vec<Key>> {
        let entrance = self
            .index
            .entrances()
            .iter()
            .find(|entrance| entrance.key.as_str() == key)?;
        let seeds = entrance_seeds(&self.index, entrance);
        Some(keys(&self.index, &self.closure_ids(&seeds)))
    }

    /// The keys of the non-process entrances, in canonical order.
    pub fn non_process_entrances(&self) -> Vec<Key> {
        self.index
            .entrances()
            .iter()
            .filter(|entrance| entrance.kind != EntranceKind::Process)
            .map(|entrance| entrance.key.clone())
            .collect()
    }

    fn closure_ids(&self, seeds: &[BlockId]) -> Vec<BlockId> {
        let per_seed: Vec<Arc<[BlockId]>> = seeds.iter().map(|&seed| self.seed_closure(seed)).collect();
        union(&per_seed.iter().map(|list| &list[..]).collect::<Vec<_>>())
    }

    /// The closure of one seed, computed once per index.
    fn seed_closure(&self, seed: BlockId) -> Arc<[BlockId]> {
        let cached = self
            .seed_closures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&seed)
            .cloned();
        if let Some(found) = cached {
            return found;
        }
        // Computed outside the lock so other requests are not held up; a race computes the
        // same set twice, harmlessly.
        let computed: Arc<[BlockId]> = closure(&self.index, &[seed]).into();
        self.seed_closures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(seed)
            .or_insert(computed)
            .clone()
    }
}

/// The current [`ReaderIndex`], shared by every request for one folder.
///
/// The workspace calls [`ReaderCache::invalidate`] after each commit (or
/// [`ReaderCache::install`] with an index built from the committed graph). A load that started
/// before an invalidation is returned to its caller but not kept, so a stale index never
/// outlives the commit that replaced its graph. Loads are single-flight: requests that miss
/// together wait for one load instead of each reading the whole graph from the store.
#[derive(Debug, Default)]
pub struct ReaderCache {
    state: Mutex<CacheState>,
    /// Held while loading from the store. Lock order: `loading`, then `state`; `state` is never
    /// held across a load.
    loading: Mutex<()>,
}

#[derive(Debug, Default)]
struct CacheState {
    epoch: u64,
    current: Option<Arc<ReaderIndex>>,
}

impl ReaderCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the current index; the next [`ReaderCache::get`] loads a new one.
    pub fn invalidate(&self) {
        let mut state = self.lock();
        state.epoch += 1;
        state.current = None;
    }

    /// Replaces the current index, for a workspace that builds it from the graph it just
    /// committed instead of reloading it.
    pub fn install(&self, index: ReaderIndex) -> Arc<ReaderIndex> {
        let index = Arc::new(index);
        let mut state = self.lock();
        state.epoch += 1;
        state.current = Some(Arc::clone(&index));
        index
    }

    /// The current index, loading it from the store on a miss.
    pub fn get(&self, pool: &ReadPool) -> Result<Arc<ReaderIndex>> {
        if let Some(current) = self.current() {
            return Ok(current);
        }
        let _turn = self.loading.lock().unwrap_or_else(PoisonError::into_inner);
        // Another request may have loaded (or the workspace installed) one while this waited.
        let epoch = {
            let state = self.lock();
            if let Some(current) = &state.current {
                return Ok(Arc::clone(current));
            }
            state.epoch
        };
        let loaded = Arc::new(ReaderIndex::load(&pool.get()?)?);
        let mut state = self.lock();
        if state.epoch == epoch && state.current.is_none() {
            state.current = Some(Arc::clone(&loaded));
        }
        Ok(loaded)
    }

    fn current(&self) -> Option<Arc<ReaderIndex>> {
        self.lock().current.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CacheState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
