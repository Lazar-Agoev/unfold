//! Graph-wide answers for requests: owners, closures, and Reached from, from one index per
//! graph version.
//!
//! Cards are served from the store: per-card data comes from per-file queries, and callers from
//! the store's reverse index ([`crate::relations::callers`]). What needs the whole graph (owners
//! across files, closures, Reached from) comes from a [`ReaderIndex`], loaded once from the store
//! (or built from the in-memory graph the workspace just committed) and shared through a
//! [`ReaderCache`] until the next commit invalidates it.
//!
//! Closures are cached per seed: the closure of a seed set is the union of its seeds'
//! closures, and many entrances share code, so Reached from for every definition costs one walk
//! per distinct seed in the life of the index.

use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use unfold_core::{Graph, Key};
use unfold_store::{ReadPool, Reader};

use crate::closure::{closure, union};
use crate::error::Result;
use crate::index::{GraphIndex, NodeId};

/// The whole-graph structure of one graph version, with cached closures. Cheap to share
/// behind an [`Arc`]; every method takes `&self` and is safe to call from several threads.
#[derive(Debug)]
pub struct ReaderIndex {
    index: GraphIndex,
    seed_closures: Mutex<FxHashMap<NodeId, Arc<[NodeId]>>>,
}

impl ReaderIndex {
    /// Loads the committed graph's structure from the store, in one snapshot.
    pub fn load(reader: &Reader) -> Result<Self> {
        Ok(Self::from_graph(&reader.graph()?))
    }

    /// Builds the structure from an in-memory graph, such as the one just committed.
    pub fn from_graph(graph: &Graph) -> Self {
        Self {
            index: GraphIndex::from_graph(graph),
            seed_closures: Mutex::new(FxHashMap::default()),
        }
    }

    /// Whether a block with `key` exists.
    pub fn contains(&self, key: &str) -> bool {
        self.index.id(key).is_some()
    }

    /// The key of the definition the block with `key` belongs to: the nearest definition above
    /// it. `None` for a module or a key that is not a block.
    pub fn owner(&self, key: &str) -> Option<&Key> {
        let owner = self.index.owner(self.index.id(key)?)?;
        Some(self.index.key(owner))
    }

    /// The first line of the block with `key`.
    pub fn line(&self, key: &str) -> Option<u32> {
        self.index.id(key).map(|id| self.index.line(id))
    }

    /// The entrances whose closures contain the block with `key` (a definition, or the
    /// definition a block belongs to), in file order. An app's own start is left out: it starts
    /// the whole process, so it reaches nearly everything. `None` when the block does not exist.
    pub fn reached_from(&self, key: &str) -> Option<Vec<Key>> {
        let id = self.index.id(key)?;
        let Some(definition) = self.index.definition(id) else {
            return Some(Vec::new());
        };
        Some(
            self.index
                .starts()
                .iter()
                .filter(|start| start.entrance)
                .filter(|start| {
                    start
                        .seeds
                        .iter()
                        .any(|&seed| self.seed_closure(seed).binary_search(&definition).is_ok())
                })
                .map(|start| start.key.clone())
                .collect(),
        )
    }

    /// The closure of the blocks with `seeds` (each standing for its definition), as definition
    /// keys in graph order. Unknown keys are ignored.
    pub fn closure(&self, seeds: &[&str]) -> Vec<Key> {
        let ids: Vec<NodeId> = seeds.iter().filter_map(|key| self.index.id(key)).collect();
        self.keys(&self.closure_ids(&ids))
    }

    /// The closure of the app or entrance with `key`: of every `code` target it starts at that
    /// names a definition. `None` when no app or entrance has that key.
    pub fn start_closure(&self, key: &str) -> Option<Vec<Key>> {
        let start = self
            .index
            .starts()
            .iter()
            .find(|start| start.key.as_str() == key)?;
        Some(self.keys(&self.closure_ids(&start.seeds)))
    }

    fn keys(&self, ids: &[NodeId]) -> Vec<Key> {
        ids.iter().map(|&id| self.index.key(id).clone()).collect()
    }

    fn closure_ids(&self, seeds: &[NodeId]) -> Vec<NodeId> {
        let per_seed: Vec<Arc<[NodeId]>> = seeds.iter().map(|&seed| self.seed_closure(seed)).collect();
        union(&per_seed.iter().map(|list| &list[..]).collect::<Vec<_>>())
    }

    /// The closure of one seed, computed once per index.
    fn seed_closure(&self, seed: NodeId) -> Arc<[NodeId]> {
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
        let computed: Arc<[NodeId]> = closure(&self.index, &[seed]).into();
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
