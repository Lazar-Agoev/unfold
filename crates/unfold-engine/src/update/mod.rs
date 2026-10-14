//! `update(changes)`: the single entry point that keeps one folder's graph current
//! (docs/engine.md#updates–7.8; decisions background updates, facts caching, last-good facts, source verification).
//!
//! An [`Engine`] is the long-lived, in-memory program of one open folder. Every action is an
//! update over it:
//!
//! | Action | Request | What changes |
//! | --- | --- | --- |
//! | First Compile | [`UpdateRequest::compile`] | Every file is new |
//! | Compile | [`UpdateRequest::compile`] | Every file is re-read and re-hashed; only changed bytes are extracted |
//! | Background update | [`UpdateRequest::background`] | The watcher's paths, checked by size and modification time first |
//! | Reopen check | [`UpdateRequest::reopen`] after [`Engine::restore`] | Every file, by size and modification time; the graph is linked again only if something it was linked from changed ([`LinkBasis`]) |
//! | Entrance edit | [`Engine::resolve_entrances`] | Only the `[[app]]` tables: they are resolved again against the current graph, which isn't linked again |
//!
//! Each update runs the stages of [`Stage`] in order: discover (or apply the changed paths),
//! snapshot the changed files (read and hash once), extract their facts in parallel through the
//! [`FactsCache`], link every enabled language over the whole program, resolve every link and
//! pair every argument, and validate the new graph with the core validator and each language's
//! census. Only a successful update replaces the engine's state; a
//! cancelled or failed one leaves the previous graph, facts, and fingerprints untouched.
//! `docs/engine.md` describes the rules.

mod basis;
mod build;
mod cache;
mod extract;
mod read;
mod restore;
mod run;
mod scan;
mod state;
mod stats;
mod summary;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::ThreadPool;
use unfold_core::validate::Violation;
use unfold_core::vocab::Language as LanguageId;
use unfold_core::{Graph, RepoPath};

pub use basis::LinkBasis;
pub use cache::{
    CacheEntry, CacheError, CacheStatus, FactsCache, FactsKey, MemoryFactsCache, NewCacheEntry, decode_facts,
    decode_failure, encode_failure,
};
pub use restore::RestoreStats;
pub use state::DiskState;
pub use stats::{Progress, Stage, UpdateStats};
pub use summary::ChangeSummary;

use crate::cancel::{CancelToken, Cancelled};
use crate::discovery::{DiscoverOptions, DiscoveryError};
use crate::entrances::Fixes;
use crate::error::BackendError;
use crate::language::{Language, Producer};
use crate::program::ProgramError;
use crate::settings::{AppEntry, Settings};
use state::State;

/// Which action an update is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UpdateKind {
    /// An explicit Compile: every core but one.
    #[default]
    Compile,
    /// A background update or reopen check: the small background pool.
    Background,
}

impl fmt::Display for UpdateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Compile => "Compile",
            Self::Background => "Background update",
        })
    }
}

/// What may have changed on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changes {
    /// Anything: discover the whole folder and check every file.
    All,
    /// Exactly these folder-relative paths (files or folders, created, changed, or deleted).
    /// A change to `unfold.toml` that alters the settings, to an ignore file, or to a
    /// folder or symlink falls back to a full discovery.
    Paths(Vec<RepoPath>),
}

/// How a file that may have changed is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verify {
    /// Compare size and modification time with the last check, and read and hash only files
    /// that differ (source verification's cheap check, for reopening and watcher events).
    Stamps,
    /// Read and hash every file that may have changed.
    Contents,
}

/// One call to [`Engine::update`].
pub struct UpdateRequest<'a> {
    /// Which action this is; it picks the thread pool.
    pub kind: UpdateKind,
    /// What may have changed.
    pub changes: Changes,
    /// How possibly changed files are checked.
    pub verify: Verify,
    /// Checked between stages and inside parallel loops.
    pub cancel: CancelToken,
    /// Called from any engine thread with progress through the snapshot and extract stages.
    pub progress: Option<&'a (dyn Fn(Progress) + Sync)>,
}

impl fmt::Debug for UpdateRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateRequest")
            .field("kind", &self.kind)
            .field("changes", &self.changes)
            .field("verify", &self.verify)
            .field("cancelled", &self.cancel.is_cancelled())
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

impl<'a> UpdateRequest<'a> {
    fn new(kind: UpdateKind, changes: Changes, verify: Verify) -> Self {
        Self {
            kind,
            changes,
            verify,
            cancel: CancelToken::new(),
            progress: None,
        }
    }

    /// An explicit Compile: discover everything and re-hash every file.
    pub fn compile() -> Self {
        Self::new(UpdateKind::Compile, Changes::All, Verify::Contents)
    }

    /// A background update for the paths the watcher reported.
    pub fn background(paths: Vec<RepoPath>) -> Self {
        Self::new(UpdateKind::Background, Changes::Paths(paths), Verify::Stamps)
    }

    /// The check after reopening a folder: discover everything, hash only files whose size or
    /// modification time differs from the restored state.
    pub fn reopen() -> Self {
        Self::new(UpdateKind::Background, Changes::All, Verify::Stamps)
    }

    /// Uses `cancel` as the cancellation token.
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// Reports progress to `progress`.
    pub fn with_progress(mut self, progress: &'a (dyn Fn(Progress) + Sync)) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Checks possibly changed files with `verify`.
    pub fn with_verify(mut self, verify: Verify) -> Self {
        self.verify = verify;
        self
    }
}

/// What kind of problem a [`Problem`] is. File failures are on the files' records instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProblemKind {
    /// `unfold.toml` can't be read or is invalid; the previous valid settings (or the
    /// defaults) stay in effect.
    Settings,
    /// Discovery couldn't read part of the folder.
    Discovery,
    /// A configuration input can't be read; languages don't see it.
    Input,
    /// The facts cache failed; the update went on without it.
    Cache,
}

/// Something the Problems view lists besides failed files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Problem {
    /// What kind of problem.
    pub kind: ProblemKind,
    /// The folder-relative path involved, or empty.
    pub path: Box<str>,
    /// What went wrong.
    pub message: Box<str>,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Why an update failed. The engine's state is unchanged.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// The update was cancelled.
    #[error("the update was cancelled")]
    Cancelled,
    /// The folder can't be walked.
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    /// The files can't form a program.
    #[error(transparent)]
    Program(#[from] ProgramError),
    /// A language broke its contract with the shared backend.
    #[error(transparent)]
    Backend(#[from] BackendError),
    /// The new graph broke invariants; nothing is committed.
    #[error("the new graph failed validation ({} violation(s)); first: {}", .0.len(), .0.first().map_or_else(String::new, ToString::to_string))]
    Validation(Vec<Violation>),
    /// A thread pool couldn't be started.
    #[error("cannot start the engine's threads: {0}")]
    Threads(String),
    /// Two registered languages share an identity.
    #[error("two registered languages are both `{0}`")]
    DuplicateLanguage(LanguageId),
    /// A prepared update was adopted after the engine's state had changed since it started.
    #[error("the prepared update is out of date: the engine changed since it started")]
    Superseded,
}

impl From<Cancelled> for UpdateError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

/// The result of a successful update.
#[derive(Debug, Clone)]
pub struct UpdateOutcome {
    /// The graph before the update.
    pub previous: Arc<Graph>,
    /// The new graph, normalized and validated.
    pub graph: Arc<Graph>,
    /// What changed, for `graph_updated`.
    pub summary: ChangeSummary,
    /// What the update did.
    pub stats: UpdateStats,
    /// Settings, discovery, input, and cache problems.
    pub problems: Vec<Problem>,
    /// Whether every discovered file is indexed: no failed file, unreadable input, discovery
    /// problem, or invalid settings.
    pub complete: bool,
    /// What the new graph was linked from: store it with the graph for [`Engine::restore`].
    pub basis: LinkBasis,
}

/// Why [`Engine::resolve_entrances`] changed nothing.
#[derive(Debug, thiserror::Error)]
pub enum EntrancesError {
    /// `unfold.toml` changed beyond its `[[app]]` tables; an update applies it.
    #[error("unfold.toml changed beyond its [[app]] tables; compile to apply it")]
    CompileNeeded,
}

/// The result of [`Engine::resolve_entrances`].
#[derive(Debug, Clone)]
pub struct EntrancesOutcome {
    /// The graph before.
    pub previous: Arc<Graph>,
    /// The graph with the apps resolved again; its files are the previous graph's.
    pub graph: Arc<Graph>,
    /// Whether the apps, their entrances, or the states of their code changed.
    pub apps_changed: bool,
    /// Whether the settings file's input record changed.
    pub inputs_changed: bool,
    /// The settings, discovery, and input problems, including a settings file that doesn't
    /// parse (the last good apps then stay).
    pub problems: Vec<Problem>,
}

/// An update [`Engine::prepare`] finished and the engine hasn't adopted yet.
#[derive(Debug)]
pub struct PreparedUpdate {
    state: State,
    outcome: UpdateOutcome,
    /// The engine generation it was prepared against.
    generation: u64,
}

impl PreparedUpdate {
    /// What the update produced.
    pub fn outcome(&self) -> &UpdateOutcome {
        &self.outcome
    }

    /// The effective settings after the update.
    pub fn settings(&self) -> &Settings {
        &self.state.settings.settings
    }

    /// What the update saw on disk for an indexed file (see [`Engine::disk_state`]).
    pub fn disk_state(&self, path: &RepoPath) -> Option<DiskState> {
        self.state.files.get(path).map(|entry| entry.disk)
    }
}

/// How many threads updates use, and how big their stacks are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineOptions {
    /// How discovery walks the folder.
    pub discover: DiscoverOptions,
    /// Threads for an explicit Compile; `0` means every core but one.
    pub compile_threads: usize,
    /// Threads for background updates.
    pub background_threads: usize,
    /// The stack size of every engine thread, as defense in depth against deeply nested
    /// syntax (front ends guard their recursion as well).
    pub stack_size: usize,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            discover: DiscoverOptions::default(),
            compile_threads: 0,
            background_threads: 2,
            stack_size: 16 * 1024 * 1024,
        }
    }
}

/// The in-memory program of one open folder. See the [module documentation](self).
pub struct Engine {
    root: PathBuf,
    root_text: String,
    /// Registered languages, sorted by identity.
    languages: Vec<Arc<dyn Language>>,
    cache: Arc<dyn FactsCache>,
    options: EngineOptions,
    state: State,
    /// Bumped whenever `state` changes, so a [`PreparedUpdate`] can tell it still applies.
    generation: u64,
    compile_pool: Option<Arc<ThreadPool>>,
    background_pool: Option<Arc<ThreadPool>>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("root", &self.root)
            .field(
                "languages",
                &self.languages.iter().map(|l| l.id()).collect::<Vec<_>>(),
            )
            .field("files", &self.state.files.len())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// An engine for the folder at `root` with no graph yet. The first update discovers and
    /// compiles everything.
    pub fn new(
        root: &Path,
        languages: Vec<Arc<dyn Language>>,
        cache: Arc<dyn FactsCache>,
        options: EngineOptions,
    ) -> Result<Self, UpdateError> {
        let root = std::fs::canonicalize(root).map_err(|source| DiscoveryError::Root {
            path: root.to_owned(),
            source,
        })?;
        if !root.is_dir() {
            return Err(DiscoveryError::NotAFolder { path: root }.into());
        }
        let mut languages = languages;
        languages.sort_by_key(|language| language.id());
        if let Some(pair) = languages.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
            return Err(UpdateError::DuplicateLanguage(pair[0].id()));
        }
        let root_text = root.to_string_lossy().replace('\\', "/");
        Ok(Self {
            root,
            root_text,
            languages,
            cache,
            options,
            state: State::default(),
            generation: 0,
            compile_pool: None,
            background_pool: None,
        })
    }

    /// Applies one update. On success the new graph replaces the old one; on failure or
    /// cancellation nothing changes. The same as [`Engine::prepare`] followed by
    /// [`Engine::adopt`].
    pub fn update(&mut self, request: UpdateRequest<'_>) -> Result<UpdateOutcome, UpdateError> {
        let prepared = self.prepare(request)?;
        self.adopt(prepared)
    }

    /// Runs one update without adopting its result: the engine keeps its current state until
    /// [`Engine::adopt`] is called with the result, so a caller can persist the new graph first
    /// and drop it (keeping the previous state) if persisting fails or is cancelled.
    pub fn prepare(&mut self, request: UpdateRequest<'_>) -> Result<PreparedUpdate, UpdateError> {
        request.cancel.check()?;
        let pool = self.pool(request.kind)?;
        let this = &*self;
        let (state, outcome) =
            pool.install(|| run::Run::new(this, &request, pool.current_num_threads()).run())?;
        Ok(PreparedUpdate {
            state,
            outcome,
            generation: self.generation,
        })
    }

    /// Makes a prepared update the engine's state and returns its outcome. Fails with
    /// [`UpdateError::Superseded`] (changing nothing) if the state changed since the update
    /// was prepared.
    pub fn adopt(&mut self, prepared: PreparedUpdate) -> Result<UpdateOutcome, UpdateError> {
        if prepared.generation != self.generation {
            return Err(UpdateError::Superseded);
        }
        self.set_state(prepared.state);
        Ok(prepared.outcome)
    }

    /// Replaces the state, invalidating updates prepared against the old one.
    fn set_state(&mut self, state: State) {
        self.state = state;
        self.generation += 1;
    }

    /// The folder's canonical path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The current graph (empty before the first update).
    pub fn graph(&self) -> &Arc<Graph> {
        &self.state.graph
    }

    /// The effective settings.
    pub fn settings(&self) -> &Settings {
        &self.state.settings.settings
    }

    /// The `[[app]]` tables as written, each with its problem, if any. The graph's apps are the
    /// valid ones, resolved.
    pub fn app_entries(&self) -> &[AppEntry] {
        &self.state.settings.apps
    }

    /// The fixes of the graph's apps whose code is missing.
    pub fn fixes(&self) -> &Fixes {
        &self.state.fixes
    }

    /// Reads `unfold.toml` again and resolves its apps and entrances against the current
    /// graph, without compiling or linking: the call to make after a change to only its
    /// `[[app]]` tables. A file that doesn't parse keeps the last good apps and is listed in
    /// the problems. Fails with [`EntrancesError::CompileNeeded`], changing nothing, when the
    /// compile settings changed too.
    pub fn resolve_entrances(&mut self) -> Result<EntrancesOutcome, EntrancesError> {
        let settings = scan::load_settings(&self.root, &self.state.settings, &Changes::All, Verify::Contents);
        if settings.settings != self.state.settings.settings {
            return Err(EntrancesError::CompileNeeded);
        }
        let mut state = self.state.clone();
        state.settings = settings;
        let previous = Arc::clone(&state.graph);
        let refreshed = run::refresh(self, &previous, &mut state);
        state.complete = run::is_complete(&state);
        let outcome = EntrancesOutcome {
            previous,
            graph: Arc::clone(&state.graph),
            apps_changed: refreshed.apps_changed,
            inputs_changed: refreshed.inputs_changed,
            problems: run::standing_problems(&state),
        };
        self.set_state(state);
        Ok(outcome)
    }

    /// Whether the current graph is complete (see [`UpdateOutcome::complete`]).
    pub fn is_complete(&self) -> bool {
        self.state.complete
    }

    /// The current settings, discovery, and input problems.
    pub fn problems(&self) -> Vec<Problem> {
        run::standing_problems(&self.state)
    }

    /// What the engine last saw on disk for an indexed file, for the store's disk stamps.
    pub fn disk_state(&self, path: &RepoPath) -> Option<DiskState> {
        self.state.files.get(path).map(|entry| entry.disk)
    }

    /// The registered front ends, sorted by language.
    pub fn producers(&self) -> Vec<(LanguageId, Producer)> {
        self.languages
            .iter()
            .map(|language| (language.id(), language.producer()))
            .collect()
    }

    /// The registered language for `id`.
    pub fn language(&self, id: LanguageId) -> Option<&Arc<dyn Language>> {
        self.languages.iter().find(|language| language.id() == id)
    }

    /// Whether a registered language with identity `language` indexes `path`.
    pub(crate) fn owner(&self, path: &RepoPath, language: LanguageId, settings: &Settings) -> bool {
        self.language(language)
            .is_some_and(|found| found.owns(path, settings))
    }

    fn pool(&mut self, kind: UpdateKind) -> Result<Arc<ThreadPool>, UpdateError> {
        let (slot, threads, name) = match kind {
            UpdateKind::Compile => (
                &mut self.compile_pool,
                compile_threads(self.options.compile_threads),
                "unfold-compile",
            ),
            UpdateKind::Background => (
                &mut self.background_pool,
                self.options.background_threads.max(1),
                "unfold-update",
            ),
        };
        if let Some(pool) = slot {
            return Ok(Arc::clone(pool));
        }
        let stack_size = self.options.stack_size;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .stack_size(stack_size)
            .thread_name(move |index| format!("{name}-{index}"))
            .build()
            .map_err(|error| UpdateError::Threads(error.to_string()))?;
        Ok(Arc::clone(slot.insert(Arc::new(pool))))
    }
}

/// Every core but one (at least one), or exactly `configured`.
fn compile_threads(configured: usize) -> usize {
    if configured > 0 {
        return configured;
    }
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .saturating_sub(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_move_to_their_own_thread() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Engine>();
        assert_send_sync::<UpdateOutcome>();
        assert!(compile_threads(0) >= 1);
        assert_eq!(compile_threads(3), 3);
    }
}
