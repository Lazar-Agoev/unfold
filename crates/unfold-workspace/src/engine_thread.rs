//! The engine thread of one live project (docs/ipc.md#the-addons-surface): it owns the [`Engine`] and the
//! store's [`Writer`], and runs one compile or update at a time.
//!
//! Each piece of work follows the same path: prepare the update (`Engine::prepare`), commit its
//! outcome in one transaction ([`crate::commit`]), let the engine adopt it (`Engine::adopt`),
//! install a fresh `ReaderIndex` built from the new graph, and tell the windows (`graph_updated`,
//! `problems_changed`, `project_state`). Work comes from the scheduler in [`crate::shared`]:
//!
//! | Work | Request | Pool |
//! | --- | --- | --- |
//! | Compile (first or later) | `compile()` | every core but one |
//! | Background update | `background(paths)` | background pool |
//! | Scan tick or event overflow | `reopen()`: every file by size and time | background pool |
//! | Reopen | store graph → `restore` → `reopen()` | background pool |
//! | Recompile (engine version changed, cache recreated) | fresh engine, `reopen()` | background pool |
//! | Entrance edit (only the `[[app]]` tables of `unfold.toml` changed) | `Engine::resolve_entrances`: no update | none |
//!
//! **The first Compile** creates `local/` (unless remembered state already did), takes the
//! folder lock, and opens the store. **If `local/cache/` disappears** while the folder is open, the next piece of work
//! notices, recreates it, and compiles everything again with a fresh engine. **The reopen check** restores the stored graph and checks every file by size and
//! time; when nothing the graph was built from changed while the folder was closed, it neither
//! links again nor commits. **A failed or cancelled commit** rolls back and the engine doesn't adopt the
//! update, so the engine, the store, and readers all keep the previous graph. A Compile the user
//! cancels stops even while committing; background work commits once started.
//!
//! **Entrances** are resolved again without an update when only `unfold.toml`'s `[[app]]`
//! tables changed: after a settings edit that changed nothing else (a queued job), and when a
//! background update's only path is `unfold.toml` (an edit outside the app). If its compile
//! settings changed too, the update runs instead. After restoring a stored graph, after every
//! update, and after re-resolving, the thread publishes the entrances ([`Shared::publish_entrances`]),
//! which sends `entrances_changed` when they changed.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use unfold_core::RepoPath;
use unfold_engine::settings::SETTINGS_PATH;
use unfold_engine::update::Progress;
use unfold_engine::{CancelToken, Engine, Language, UpdateError, UpdateRequest};
use unfold_store::{Store, Writer};

use crate::commit::{self, Committed};
use crate::compile::{CompileJob, CompileOutcome, CompileProgress, ProgressFn};
use crate::error::{Result, WorkspaceError};
use crate::events::WorkspaceEvent;
use crate::facts_cache::StoreFactsCache;
use crate::lock::FolderLock;
use crate::publish;
use crate::shared::{Job, Running, Shared, lock};
use crate::status::{Activity, WatchMode};
use crate::storage::open_store;

/// What the engine thread does next.
enum Work {
    Job(Job, CancelToken),
    Background {
        paths: Vec<RepoPath>,
        all: bool,
        cancel: CancelToken,
    },
}

/// Which update to run.
enum RunKind {
    Compile,
    Background(Vec<RepoPath>),
    Rescan,
    Reopen,
    Recompile,
}

impl RunKind {
    fn activity(&self) -> Activity {
        match self {
            Self::Compile | Self::Recompile => Activity::Compiling { done: 0, total: 0 },
            Self::Background(paths) => Activity::Updating {
                files: Some(u32::try_from(paths.len()).unwrap_or(u32::MAX)),
            },
            Self::Rescan | Self::Reopen => Activity::Updating { files: None },
        }
    }

    fn request<'a>(&self) -> UpdateRequest<'a> {
        match self {
            Self::Compile => UpdateRequest::compile(),
            Self::Background(paths) => UpdateRequest::background(paths.clone()),
            Self::Rescan | Self::Reopen | Self::Recompile => UpdateRequest::reopen(),
        }
    }
}

/// The engine thread's state. See the [module documentation](self).
pub(crate) struct EngineThread {
    shared: Arc<Shared>,
    languages: Vec<Arc<dyn Language>>,
    version: String,
    engine: Option<Engine>,
    writer: Option<Arc<Mutex<Writer>>>,
    committed: Option<Committed>,
    lock: Option<FolderLock>,
}

impl EngineThread {
    /// An engine thread for a folder; `store` is the database opened at open time, if the
    /// folder was compiled before.
    pub(crate) fn new(
        shared: Arc<Shared>,
        languages: Vec<Arc<dyn Language>>,
        version: String,
        store: Option<(Store, FolderLock)>,
    ) -> Result<Self> {
        let mut thread = Self {
            shared,
            languages,
            version,
            engine: None,
            writer: None,
            committed: None,
            lock: None,
        };
        if let Some((store, lock)) = store {
            thread.install(store, lock)?;
        }
        Ok(thread)
    }

    /// Starts the thread.
    pub(crate) fn spawn(self) -> std::io::Result<JoinHandle<()>> {
        let stack = self.shared.options.engine.stack_size;
        std::thread::Builder::new()
            .name("unfold-engine".into())
            .stack_size(stack)
            .spawn(move || self.main_loop())
    }

    fn main_loop(mut self) {
        while let Some(work) = self.next() {
            let result = catch_unwind(AssertUnwindSafe(|| self.execute(work)));
            if let Err(panic) = result {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "the engine thread panicked".to_owned());
                publish::failed(&self.shared, &WorkspaceError::Internal(message));
            }
        }
        let jobs: Vec<Job> = lock(&self.shared.sched).jobs.drain(..).collect();
        for job in jobs {
            if let Job::Compile(job) = job {
                let _ = job.reply.send(Err(WorkspaceError::Closed));
            }
        }
        let mut sched = lock(&self.shared.sched);
        sched.busy = false;
        sched.running = None;
        drop(sched);
        self.shared.wake.notify_all();
    }

    /// Waits for the next piece of work; `None` at shutdown.
    fn next(&mut self) -> Option<Work> {
        let shared = Arc::clone(&self.shared);
        let options = &shared.options;
        let mut sched = lock(&shared.sched);
        sched.busy = false;
        sched.running = None;
        shared.wake.notify_all();
        loop {
            if sched.shutdown {
                return None;
            }
            if let Some(job) = sched.jobs.pop_front() {
                let cancel = match &job {
                    Job::Compile(compile) => compile.cancel.clone(),
                    _ => CancelToken::new(),
                };
                sched.running = Some(Running {
                    cancel: cancel.clone(),
                    background: false,
                });
                sched.busy = true;
                return Some(Work::Job(job, cancel));
            }
            let now = Instant::now();
            let mut deadline: Option<Instant> = None;
            if sched.compiled {
                if sched.watch == WatchMode::Scanning {
                    let next = *sched.next_scan.get_or_insert(now + options.scan_interval);
                    if now >= next {
                        sched.rescan = true;
                        sched.next_scan = Some(now + options.scan_interval);
                    } else {
                        deadline = Some(next);
                    }
                }
                if !sched.pending.is_empty() || sched.rescan {
                    let due = sched.last_change.map_or(now, |last| last + options.debounce);
                    if now >= due {
                        let paths: Vec<RepoPath> = std::mem::take(&mut sched.pending).into_iter().collect();
                        let all = std::mem::take(&mut sched.rescan);
                        sched.last_change = None;
                        let cancel = CancelToken::new();
                        sched.running = Some(Running {
                            cancel: cancel.clone(),
                            background: true,
                        });
                        sched.busy = true;
                        return Some(Work::Background { paths, all, cancel });
                    }
                    deadline = Some(deadline.map_or(due, |other| other.min(due)));
                }
            }
            sched = match deadline {
                Some(deadline) => {
                    shared
                        .wake
                        .wait_timeout(sched, deadline.saturating_duration_since(now))
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                }
                None => shared
                    .wake
                    .wait(sched)
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            };
        }
    }

    fn execute(&mut self, work: Work) {
        if self.cache_missing() {
            if let Err(error) = self.recover() {
                publish::failed(&self.shared, &error);
                if let Work::Job(Job::Compile(job), _) = work {
                    let _ = job.reply.send(Err(error));
                }
                return;
            }
            // Everything is compiled again below: an explicit Compile does it itself.
            match work {
                Work::Job(Job::Compile(job), _) => self.compile(job),
                Work::Job(_, cancel) | Work::Background { cancel, .. } => self.recompile(cancel),
            }
            return;
        }
        match work {
            Work::Job(Job::Compile(job), _) => self.compile(job),
            Work::Job(Job::Reopen, cancel) => self.reopen(cancel),
            Work::Job(Job::Recompile, cancel) => self.recompile(cancel),
            Work::Job(Job::CheckCache, _) => {}
            Work::Job(Job::ResolveEntrances(reply), _) => {
                if !self.resolve_entrances() {
                    self.shared.add_changes(RepoPath::new(SETTINGS_PATH).ok());
                }
                let _ = reply.send(());
            }
            Work::Background { paths, all, cancel } => self.background(paths, all, cancel),
        }
    }

    /// Whether the store's database disappeared (someone deleted `local/cache/`).
    fn cache_missing(&self) -> bool {
        self.writer.is_some() && !self.shared.layout.database().exists()
    }

    /// Recreates `local/cache/` and an empty store, with a fresh engine.
    fn recover(&mut self) -> Result<()> {
        self.shared.set_readers(None);
        self.shared.reader_cache.invalidate();
        self.engine = None;
        self.writer = None;
        self.committed = None;
        self.lock = None;
        self.open_store()
    }

    /// Creates `local/` if needed and opens the store there (the first Compile, or recovery).
    fn open_store(&mut self) -> Result<()> {
        let (store, folder_lock) = open_store(&self.shared.layout, &self.version)?;
        self.install(store, folder_lock)
    }

    /// Takes over an open store: the writer for commits and the facts cache, the readers for
    /// everyone else, and a fresh engine over them.
    fn install(&mut self, store: Store, folder_lock: FolderLock) -> Result<()> {
        let Store { writer, readers, .. } = store;
        let writer = Arc::new(Mutex::new(writer));
        self.engine = Some(self.new_engine(&writer)?);
        self.writer = Some(writer);
        self.committed = None;
        self.lock = Some(folder_lock);
        self.shared.set_readers(Some(readers));
        self.shared.reader_cache.invalidate();
        Ok(())
    }

    fn new_engine(&self, writer: &Arc<Mutex<Writer>>) -> Result<Engine> {
        Engine::new(
            &self.shared.root,
            self.languages.clone(),
            Arc::new(StoreFactsCache::new(Arc::clone(writer))),
            self.shared.options.engine.clone(),
        )
        .map_err(|error| WorkspaceError::Update(error.to_string()))
    }

    fn compile(&mut self, job: CompileJob) {
        let CompileJob {
            cancel,
            progress,
            reply,
        } = job;
        // A Compile covers every pending change.
        let (paths, all) = {
            let mut sched = lock(&self.shared.sched);
            sched.last_change = None;
            (
                std::mem::take(&mut sched.pending),
                std::mem::take(&mut sched.rescan),
            )
        };
        let result = self.compile_inner(cancel, progress.as_ref());
        if result.is_err() {
            self.restore_pending(paths.into_iter().collect(), all);
        }
        let _ = reply.send(result);
    }

    fn compile_inner(
        &mut self,
        cancel: CancelToken,
        progress: Option<&ProgressFn>,
    ) -> Result<CompileOutcome> {
        cancel.check().map_err(|_| WorkspaceError::Cancelled)?;
        if self.writer.is_none() {
            self.shared.update_status(|model| {
                model.activity = Activity::Compiling { done: 0, total: 0 };
            });
            if let Err(error) = self.open_store() {
                self.shared.update_status(|model| model.activity = Activity::Idle);
                return Err(error);
            }
        }
        self.run(RunKind::Compile, cancel, progress)
    }

    fn background(&mut self, paths: Vec<RepoPath>, all: bool, cancel: CancelToken) {
        if self.engine.is_none() {
            return;
        }
        let settings_only = !all && paths.len() == 1 && paths[0].as_str() == SETTINGS_PATH;
        if settings_only && self.resolve_entrances() {
            return;
        }
        let kind = if all {
            RunKind::Rescan
        } else {
            RunKind::Background(paths.clone())
        };
        if let Err(WorkspaceError::Cancelled) = self.run(kind, cancel, None) {
            self.restore_pending(paths, all);
        }
    }

    /// Puts the paths of an update that didn't land back into the queue.
    fn restore_pending(&self, paths: Vec<RepoPath>, all: bool) {
        let mut sched = lock(&self.shared.sched);
        sched.pending.extend(paths);
        sched.rescan |= all;
        sched.last_change.get_or_insert_with(Instant::now);
        drop(sched);
        self.shared.wake.notify_all();
    }

    fn reopen(&mut self, cancel: CancelToken) {
        self.shared
            .update_status(|model| model.activity = Activity::Updating { files: None });
        let loaded = self
            .shared
            .readers()
            .ok_or(WorkspaceError::NotCompiled)
            .and_then(|pool| Ok(pool.get()?))
            .and_then(|reader| commit::load(&reader));
        let restored = loaded.and_then(|loaded| {
            let engine = self.engine.as_mut().ok_or(WorkspaceError::NotCompiled)?;
            engine
                .restore(loaded.graph, loaded.disks, loaded.basis)
                .map_err(|error| WorkspaceError::Update(error.to_string()))?;
            self.shared.publish_entrances(engine);
            Ok(loaded.committed)
        });
        match restored {
            Ok(committed) => {
                self.committed = Some(committed);
                let _ = self.run(RunKind::Reopen, cancel, None);
            }
            // A store that can't be read back is a cache: start it over.
            Err(_) => self.recompile(cancel),
        }
    }

    /// Compiles everything in the background with a fresh engine into an empty store.
    fn recompile(&mut self, cancel: CancelToken) {
        let Some(writer) = self.writer.clone() else {
            return;
        };
        let fresh = lock(&writer)
            .clear()
            .map_err(WorkspaceError::from)
            .and_then(|()| self.new_engine(&writer));
        match fresh {
            Ok(engine) => {
                self.engine = Some(engine);
                self.committed = None;
                self.shared.reader_cache.invalidate();
                let _ = self.run(RunKind::Recompile, cancel, None);
            }
            Err(error) => publish::failed(&self.shared, &error),
        }
    }

    /// Resolves the `[[app]]` tables of `unfold.toml` again against the current graph, without
    /// an update, and commits what changed. Whether it did: not without a graph whose stored
    /// copy is known, nor when the compile settings changed too (an update applies them then).
    fn resolve_entrances(&mut self) -> bool {
        let Self {
            shared,
            engine,
            writer,
            committed,
            ..
        } = self;
        let (Some(engine), Some(writer), Some(base)) = (engine.as_mut(), writer.as_ref(), committed.as_ref())
        else {
            return false;
        };
        let Ok(outcome) = engine.resolve_entrances() else {
            return false;
        };
        let complete = engine.is_complete();
        // The engine holds the new apps already: if the store can't, the next update replaces
        // everything it holds.
        match commit::commit_entrances(&mut lock(writer), &outcome, complete, base) {
            Ok(next) => *committed = Some(next),
            Err(error) => {
                *committed = None;
                publish::failed(shared, &error);
            }
        }
        publish::entrances_resolved(shared, &outcome, complete);
        shared.publish_entrances(engine);
        true
    }

    /// Runs one update and commits it, keeping the status current.
    fn run(
        &mut self,
        kind: RunKind,
        cancel: CancelToken,
        stream: Option<&ProgressFn>,
    ) -> Result<CompileOutcome> {
        let activity = kind.activity();
        self.shared.update_status(|model| model.activity = activity);
        let result = self.update_and_commit(&kind, cancel, stream);
        match &result {
            Ok(_) | Err(WorkspaceError::Cancelled) => {
                self.shared.update_status(|model| model.activity = Activity::Idle);
            }
            Err(error) => publish::failed(&self.shared, error),
        }
        result
    }

    fn update_and_commit(
        &mut self,
        kind: &RunKind,
        cancel: CancelToken,
        stream: Option<&ProgressFn>,
    ) -> Result<CompileOutcome> {
        let shared = &*self.shared;
        let compiling = matches!(kind.activity(), Activity::Compiling { .. });
        let last_sent: Mutex<Option<Instant>> = Mutex::new(None);
        let on_progress = |progress: Progress| {
            let report = CompileProgress {
                stage: progress.stage.as_str(),
                done: progress.done as u64,
                total: progress.total as u64,
            };
            if let Some(stream) = stream {
                stream(&report);
            }
            let mut last = lock(&last_sent);
            let due = progress.done >= progress.total
                || last.is_none_or(|at| at.elapsed() >= shared.options.progress_interval);
            if !due {
                return;
            }
            *last = Some(Instant::now());
            drop(last);
            if compiling {
                let (done, total) = (
                    u32::try_from(progress.done).unwrap_or(u32::MAX),
                    u32::try_from(progress.total).unwrap_or(u32::MAX),
                );
                shared.update_status(|model| model.activity = Activity::Compiling { done, total });
            }
            shared.emit(&WorkspaceEvent::CompileProgress {
                project: shared.project.clone(),
                stage: report.stage,
                done: report.done,
                total: report.total,
            });
        };
        let engine = self.engine.as_mut().ok_or(WorkspaceError::NotCompiled)?;
        let update_error = |error| match error {
            UpdateError::Cancelled => WorkspaceError::Cancelled,
            other => WorkspaceError::Update(other.to_string()),
        };
        // A Compile the user cancels stops even while committing, so the cancel is acknowledged
        // promptly; background work that newer changes cancel always commits once it has started
        // committing (§7.5), so a stream of edits can't starve it.
        let commit_cancel = matches!(kind, RunKind::Compile).then(|| cancel.clone());
        let request = kind.request().with_cancel(cancel).with_progress(&on_progress);
        let prepared = engine.prepare(request).map_err(update_error)?;
        let writer = self.writer.as_ref().ok_or(WorkspaceError::NotCompiled)?;
        let base = self.committed.as_ref();
        let first = base.is_none();
        // The engine adopts the update only once the store holds it: a failed or cancelled
        // commit rolls back and leaves the engine, the store, and readers on the previous graph.
        // An update that changed nothing the store holds isn't committed.
        let next = match base {
            Some(base) if commit::unchanged(engine, &prepared, base) => None,
            _ => Some(commit::commit(
                &mut lock(writer),
                engine,
                &prepared,
                base,
                commit_cancel.as_ref(),
            )?),
        };
        let outcome = engine.adopt(prepared).map_err(update_error)?;
        if let Some(next) = next {
            self.committed = Some(next);
        }
        self.shared.publish_entrances(engine);
        let rescanned = matches!(kind, RunKind::Rescan);
        Ok(publish::committed(&self.shared, &outcome, first, rescanned))
    }
}
