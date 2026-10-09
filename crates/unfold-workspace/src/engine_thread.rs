//! The engine thread of one folder (architecture §4.2): it owns the [`Engine`] and the store's
//! [`Writer`], and runs one compile or update at a time.
//!
//! Each piece of work follows the same path: prepare the update (`Engine::prepare`), commit its
//! outcome in one transaction ([`crate::commit`]), let the engine adopt it (`Engine::adopt`),
//! install a fresh `ReaderIndex` built from the new graph, and tell the windows (`graph_updated`, `problems_changed`, `graph_status`). Work comes from the
//! scheduler in [`crate::shared`]:
//!
//! | Work | Request | Pool |
//! | --- | --- | --- |
//! | Compile (first or later) | `compile()` | every core but one |
//! | Rebuild from scratch | facts cache cleared, `rebuild()`, the stored graph replaced | every core but one |
//! | Background update | `background(paths)` | background pool |
//! | Scan tick or event overflow | `reopen()`: every file by size and time | background pool |
//! | Reopen | store graph → `restore` → `reopen()` | background pool |
//! | Recompile (engine version changed, cache recreated) | fresh engine, `reopen()` | background pool |
//!
//! **The first Compile** creates `.unfold/` (or the fallback location), takes the folder lock,
//! and opens the store, and only then. **If `local/cache/` disappears** while the folder is
//! open, the next piece of work notices, recreates it, and compiles everything again with a
//! fresh engine. **A failed or cancelled commit** rolls back and the engine doesn't adopt the
//! update, so the engine, the store, and readers all keep the previous graph. A Compile or
//! Rebuild the user cancels stops even while committing; background work commits once started.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use unfold_core::RepoPath;
use unfold_core::vocab::FileState;
use unfold_engine::update::Progress;
use unfold_engine::{CancelToken, Engine, Language, UpdateError, UpdateRequest};
use unfold_store::{Store, Writer};

use crate::commit::{self, Committed};
use crate::compile::{CompileJob, CompileMode, CompileOutcome, CompileProgress, ProgressFn};
use crate::error::{Result, WorkspaceError};
use crate::events::WorkspaceEvent;
use crate::facts_cache::StoreFactsCache;
use crate::lock::FolderLock;
use crate::publish;
use crate::shared::{Job, Running, Shared, fallback_of, lock};
use crate::state::write_json;
use crate::status::{Activity, WatchMode};
use crate::storage::open_store;
use crate::watcher;

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
    Rebuild,
    Background(Vec<RepoPath>),
    Rescan,
    Reopen,
    Recompile,
}

impl RunKind {
    fn activity(&self) -> Activity {
        match self {
            Self::Compile | Self::Rebuild | Self::Recompile => Activity::Compiling { done: 0, total: 0 },
            Self::Background(paths) => Activity::Updating {
                files: Some(u32::try_from(paths.len()).unwrap_or(u32::MAX)),
            },
            Self::Rescan | Self::Reopen => Activity::Updating { files: None },
        }
    }

    fn request<'a>(&self) -> UpdateRequest<'a> {
        match self {
            Self::Compile => UpdateRequest::compile(),
            Self::Rebuild => UpdateRequest::rebuild(),
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
            if sched.compiled && !sched.paused {
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
            Work::Background { paths, all, cancel } => self.background(paths, all, cancel),
        }
    }

    /// Whether the store's database disappeared (someone deleted `local/cache/`).
    fn cache_missing(&self) -> bool {
        self.writer.is_some() && !lock(&self.shared.layout).database().exists()
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
        let mut layout = lock(&self.shared.layout).clone();
        let (store, folder_lock) = open_store(&mut layout, &self.version)?;
        let fallback = fallback_of(&layout);
        let drafts = layout.drafts();
        *lock(&self.shared.layout) = layout;
        self.shared.update_status(|model| model.fallback = fallback);
        if fallback.is_some() {
            watcher::watch_outside(&self.shared, &drafts);
        }
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
        // Under the state lock, like `Workspace::update_folder_state`, so this first write can't
        // overwrite a newer state saved meanwhile.
        let state = lock(&self.shared.folder_state);
        let state_file = lock(&self.shared.layout).state_file();
        if !state_file.exists() {
            write_json(&state_file, &*state, "state.json")?;
        }
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
            mode,
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
        let result = self.compile_inner(mode, cancel, progress.as_ref());
        if result.is_err() {
            self.restore_pending(paths.into_iter().collect(), all);
        } else {
            self.shared.update_status(|model| model.changed = 0);
        }
        let _ = reply.send(result);
    }

    fn compile_inner(
        &mut self,
        mode: CompileMode,
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
        let kind = match mode {
            CompileMode::Check => RunKind::Compile,
            // The rebuild reads no cached facts. The stored graph stays until the rebuilt one
            // replaces it, and the cache gains only fresh entries, so a cancelled or failed
            // rebuild leaves the previous graph with its facts.
            CompileMode::FromScratch => RunKind::Rebuild,
        };
        self.run(kind, cancel, progress)
    }

    fn background(&mut self, paths: Vec<RepoPath>, all: bool, cancel: CancelToken) {
        if self.engine.is_none() {
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
        let (paused, changed) = (sched.paused, sched.changed());
        drop(sched);
        self.shared.wake.notify_all();
        if paused {
            self.shared.update_status(|model| model.changed = changed);
        }
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
                .restore(loaded.graph, loaded.disks)
                .map_err(|error| WorkspaceError::Update(error.to_string()))?;
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
                folder: shared.folder.clone(),
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
        // A Compile the user cancels stops even while committing (the cancel acknowledgement
        // budget, architecture §17); background work that newer changes cancel always commits
        // once it has started committing (§7.5), so a stream of edits can't starve it.
        let commit_cancel = matches!(kind, RunKind::Compile | RunKind::Rebuild).then(|| cancel.clone());
        let request = kind.request().with_cancel(cancel).with_progress(&on_progress);
        let prepared = engine.prepare(request).map_err(update_error)?;
        let writer = self.writer.as_ref().ok_or(WorkspaceError::NotCompiled)?;
        // A rebuild rewrites the whole stored graph, as when the store's contents are unknown.
        let base = self
            .committed
            .as_ref()
            .filter(|_| !matches!(kind, RunKind::Rebuild));
        let first = base.is_none();
        // The engine adopts the update only once the store holds it: a failed or cancelled
        // commit rolls back and leaves the engine, the store, and readers on the previous graph.
        let (stats, next) =
            commit::commit(&mut lock(writer), engine, &prepared, base, commit_cancel.as_ref())?;
        let outcome = engine.adopt(prepared).map_err(update_error)?;
        self.committed = Some(next);
        if matches!(kind, RunKind::Rebuild) {
            // Rebuild drops last-good facts (§7.1): a file that doesn't parse now is failed,
            // and the cache must not bring its last good version back later. Fresh entries
            // replaced every other file's. The cache is an accelerator: if this fails, the
            // worst case is a last-good version returning for a file that is still broken.
            let failed: Vec<&RepoPath> = outcome
                .graph
                .files
                .iter()
                .filter(|file| file.file.state == FileState::Failed)
                .map(|file| &file.file.path)
                .collect();
            if !failed.is_empty() {
                let _ = lock(writer).forget_facts(&failed);
            }
        }
        let rescanned = matches!(kind, RunKind::Rescan);
        Ok(publish::committed(
            &self.shared,
            &outcome,
            &stats,
            first,
            rescanned,
        ))
    }
}
