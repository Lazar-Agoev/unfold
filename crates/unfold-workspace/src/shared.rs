//! The state one project's threads share: the scheduler queue, the status model, problems, the
//! Index log, the reader pool, remembered state, and the event sink.
//!
//! # Scheduling (docs/engine.md#updates)
//!
//! [`Sched`] is the engine thread's queue, guarded by one mutex with one condition variable
//! (`wake`) that every change notifies:
//!
//! - **Jobs** (an explicit Compile, the reopen check, a background recompile, a cache check)
//!   run first, in order.
//! - **Changed paths** from the watcher accumulate in `pending`. A background update starts
//!   once `debounce` (about 300 ms) has passed since the last change, so a branch switch that
//!   touches 500 files becomes one update.
//! - A newer change **cancels** a running background update (the engine acknowledges within one
//!   file's work). Its paths go back into `pending`, merge with the new ones, and the update
//!   starts again after the next quiet period. An update that already returned commits: the
//!   engine swapped its state, so the store must follow, and the new paths simply make the next
//!   update.
//! - While background updates are **paused**, paths keep accumulating and their count is the
//!   "N files changed since the last compile" on Home's state line. A Compile covers everything and
//!   clears them.
//! - In **scanning** mode (no usable file-system events) the engine thread checks the whole
//!   folder every `scan_interval` instead.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant};

use notify::RecommendedWatcher;
use notify_debouncer_full::{Debouncer, NoCache};
use unfold_core::{ContentHash, RepoPath};
use unfold_engine::CancelToken;
use unfold_reader::ReaderCache;
use unfold_store::ReadPool;

use crate::compile::CompileJob;
use crate::events::{EventSink, WorkspaceEvent};
use crate::layout::Layout;
use crate::options::WorkspaceOptions;
use crate::problems::{IndexLog, ProblemItem, Problems, project_problem_count};
use crate::projects::ProjectId;
use crate::state::LocalState;
use crate::status::{GraphStatus, ProjectState, StatusModel, WatchMode};

/// Locks a mutex, recovering the data from a panicked holder: every structure behind these
/// mutexes stays consistent between statements.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One unit of queued engine-thread work.
pub(crate) enum Job {
    /// An explicit Compile or Rebuild.
    Compile(CompileJob),
    /// Restore from the store, then check every file by size and modification time.
    Reopen,
    /// Compile everything in the background with a fresh engine (a recreated or missing cache).
    Recompile,
    /// Recreate `local/cache/` if it disappeared, and recompile.
    CheckCache,
}

/// The update running on the engine thread.
pub(crate) struct Running {
    pub(crate) cancel: CancelToken,
    /// A background update, which newer changes cancel.
    pub(crate) background: bool,
}

/// The engine thread's queue. See the [module documentation](self).
pub(crate) struct Sched {
    pub(crate) jobs: VecDeque<Job>,
    pub(crate) pending: BTreeSet<RepoPath>,
    pub(crate) last_change: Option<Instant>,
    /// Check the whole folder (an event overflow, a scan tick).
    pub(crate) rescan: bool,
    pub(crate) running: Option<Running>,
    pub(crate) paused: bool,
    /// Whether the folder has a graph, so background updates may run.
    pub(crate) compiled: bool,
    pub(crate) watch: WatchMode,
    pub(crate) next_scan: Option<Instant>,
    pub(crate) shutdown: bool,
    /// The engine thread is executing work.
    pub(crate) busy: bool,
}

impl Sched {
    /// Whether no work is running, queued, or due.
    pub(crate) fn is_idle(&self) -> bool {
        !self.busy
            && self.jobs.is_empty()
            && (self.paused || !self.compiled || (self.pending.is_empty() && !self.rescan))
    }

    /// The changed-file count Home's state line shows while paused.
    pub(crate) fn changed(&self) -> u32 {
        u32::try_from(self.pending.len()).unwrap_or(u32::MAX)
    }
}

/// Writes the app made itself, so the watcher doesn't report them as outside edits. A write
/// counts as the app's own only while the file still holds what the app wrote (or is still gone,
/// after the app deleted it): an outside edit that lands moments later is still reported.
#[derive(Debug, Default)]
pub(crate) struct OwnWrites {
    recent: Mutex<Vec<(PathBuf, Option<ContentHash>, Instant)>>,
}

impl OwnWrites {
    const WINDOW: Duration = Duration::from_secs(2);

    /// Notes that the app is about to write `bytes` to `path` (`None`: delete it).
    pub(crate) fn note(&self, path: PathBuf, bytes: Option<&[u8]>) {
        let mut recent = lock(&self.recent);
        recent.retain(|(_, _, at)| at.elapsed() < Self::WINDOW);
        recent.push((path, bytes.map(ContentHash::of), Instant::now()));
    }

    /// Whether `path` holds what the app recently wrote to it (or is gone after the app deleted
    /// it).
    pub(crate) fn contains(&self, path: &std::path::Path) -> bool {
        let written: Vec<Option<ContentHash>> = lock(&self.recent)
            .iter()
            .filter(|(noted, _, at)| noted == path && at.elapsed() < Self::WINDOW)
            .map(|(_, hash, _)| *hash)
            .collect();
        if written.is_empty() {
            return false;
        }
        let now = std::fs::read(path).ok().map(|bytes| ContentHash::of(&bytes));
        written.contains(&now)
    }
}

/// The status model and the last row state sent.
pub(crate) struct StatusCell {
    pub(crate) model: StatusModel,
    sent: Option<ProjectState>,
}

/// Everything one project's threads share.
pub(crate) struct Shared {
    pub(crate) project: ProjectId,
    pub(crate) root: PathBuf,
    pub(crate) options: WorkspaceOptions,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) layout: Mutex<Layout>,
    pub(crate) sched: Mutex<Sched>,
    pub(crate) wake: Condvar,
    pub(crate) status: Mutex<StatusCell>,
    pub(crate) problems: Mutex<Problems>,
    pub(crate) index_log: Mutex<IndexLog>,
    pub(crate) readers: RwLock<Option<ReadPool>>,
    pub(crate) reader_cache: ReaderCache,
    pub(crate) local_state: Mutex<LocalState>,
    /// Held across a settings edit's read, edit, and write, so concurrent edits (two windows)
    /// apply one after the other instead of the later write dropping the earlier edit.
    pub(crate) settings_edit: Mutex<()>,
    pub(crate) own_writes: OwnWrites,
    pub(crate) watcher: Mutex<Option<Debouncer<RecommendedWatcher, NoCache>>>,
}

impl Shared {
    pub(crate) fn new(
        project: ProjectId,
        root: PathBuf,
        options: WorkspaceOptions,
        sink: Arc<dyn EventSink>,
        layout: Layout,
        local_state: LocalState,
        compiled: bool,
    ) -> Self {
        // Not watched until the workspace starts its watcher.
        let watch = WatchMode::Off;
        let paused = !local_state.background_updates;
        let mut model = StatusModel::new(watch);
        model.compiled = compiled;
        model.paused = paused;
        model.fallback = fallback_of(&layout);
        Self {
            project,
            root,
            options,
            sink,
            layout: Mutex::new(layout),
            sched: Mutex::new(Sched {
                jobs: VecDeque::new(),
                pending: BTreeSet::new(),
                last_change: None,
                rescan: false,
                running: None,
                paused,
                compiled,
                watch,
                next_scan: None,
                shutdown: false,
                busy: false,
            }),
            wake: Condvar::new(),
            status: Mutex::new(StatusCell { model, sent: None }),
            problems: Mutex::new(Problems::default()),
            index_log: Mutex::new(IndexLog::default()),
            readers: RwLock::new(None),
            reader_cache: ReaderCache::new(),
            local_state: Mutex::new(local_state),
            settings_edit: Mutex::new(()),
            own_writes: OwnWrites::default(),
            watcher: Mutex::new(None),
        }
    }

    /// Sends `event` to every window.
    pub(crate) fn emit(&self, event: &WorkspaceEvent) {
        self.sink.emit(event);
    }

    /// The current status.
    pub(crate) fn status(&self) -> GraphStatus {
        lock(&self.status).model.status()
    }

    /// The row state now, and the one a project going to rest remembers.
    pub(crate) fn project_states(&self) -> (ProjectState, ProjectState) {
        let cell = lock(&self.status);
        (cell.model.project_state(), cell.model.settled_state())
    }

    /// Changes the status model and sends `project_state` when the row state changed kind. The
    /// event is sent while the model is locked, so windows see states in order.
    pub(crate) fn update_status(&self, change: impl FnOnce(&mut StatusModel)) {
        let mut cell = lock(&self.status);
        change(&mut cell.model);
        let state = cell.model.project_state();
        if cell.sent.as_ref().is_none_or(|sent| sent.differs_in_kind(&state)) {
            cell.sent = Some(state.clone());
            self.emit(&WorkspaceEvent::ProjectState {
                project: self.project.clone(),
                state,
            });
        }
    }

    /// Changes the problems and sends `problems_changed` when the list changed (and
    /// `project_state` when the change moves the row's state: board health doesn't count toward
    /// it, see [`project_problem_count`]).
    pub(crate) fn update_problems(&self, change: impl FnOnce(&mut Problems)) {
        let mut problems = lock(&self.problems);
        let before = problems.all();
        change(&mut problems);
        let after = problems.all();
        if before != after {
            let count = u32::try_from(after.len()).unwrap_or(u32::MAX);
            self.emit(&WorkspaceEvent::ProblemsChanged {
                project: self.project.clone(),
                count,
            });
            // Still under the problems lock, so the row's count follows the list's order.
            self.update_status(|model| model.problems = project_problem_count(&after));
        }
    }

    /// Every problem.
    pub(crate) fn problems(&self) -> Vec<ProblemItem> {
        lock(&self.problems).all()
    }

    /// Queues `job` and wakes the engine thread. A queued Compile cancels a running background
    /// update, whose changes the Compile covers.
    pub(crate) fn push_job(&self, job: Job) {
        let mut sched = lock(&self.sched);
        if matches!(job, Job::Compile(_))
            && let Some(running) = sched.running.as_ref().filter(|running| running.background)
        {
            running.cancel.cancel();
        }
        sched.jobs.push_back(job);
        drop(sched);
        self.wake.notify_all();
    }

    /// Records changed paths from the watcher (or a caller) and cancels a running background
    /// update so it restarts with them.
    pub(crate) fn add_changes(&self, paths: impl IntoIterator<Item = RepoPath>) {
        let mut sched = lock(&self.sched);
        let mut any = false;
        for path in paths {
            sched.pending.insert(path);
            any = true;
        }
        if !any {
            return;
        }
        // A path changed again also restarts the quiet period.
        sched.last_change = Some(Instant::now());
        if let Some(running) = sched.running.as_ref().filter(|running| running.background) {
            running.cancel.cancel();
        }
        let (paused, changed) = (sched.paused, sched.changed());
        drop(sched);
        self.wake.notify_all();
        if paused {
            self.update_status(|model| model.changed = changed);
        }
    }

    /// Asks for a check of the whole folder (an event overflow or a scan tick).
    pub(crate) fn request_rescan(&self) {
        let mut sched = lock(&self.sched);
        sched.rescan = true;
        sched.last_change = Some(Instant::now());
        if let Some(running) = sched.running.as_ref().filter(|running| running.background) {
            running.cancel.cancel();
        }
        drop(sched);
        self.wake.notify_all();
    }

    /// Switches to scanning when file-system events are unusable.
    pub(crate) fn fall_back_to_scanning(&self) {
        let mut sched = lock(&self.sched);
        if sched.watch == WatchMode::Scanning {
            return;
        }
        sched.watch = WatchMode::Scanning;
        sched.rescan = true;
        sched.last_change = Some(Instant::now());
        drop(sched);
        self.wake.notify_all();
        self.update_status(|model| model.watching = WatchMode::Scanning);
    }

    /// Whether the engine thread has nothing running, queued, or due (or is shut down).
    pub(crate) fn is_idle(&self) -> bool {
        let sched = lock(&self.sched);
        sched.is_idle() || sched.shutdown
    }

    /// Waits until the engine thread has nothing running, queued, or due, or `timeout` passes.
    pub(crate) fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut sched = lock(&self.sched);
        loop {
            if sched.is_idle() || sched.shutdown {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            sched = self
                .wake
                .wait_timeout(sched, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The reader pool, once the folder has a store.
    pub(crate) fn readers(&self) -> Option<ReadPool> {
        self.readers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_readers(&self, readers: Option<ReadPool>) {
        *self.readers.write().unwrap_or_else(PoisonError::into_inner) = readers;
    }
}

/// The fallback reason a layout shows in the status.
pub(crate) fn fallback_of(layout: &Layout) -> Option<crate::layout::FallbackReason> {
    match layout.location() {
        crate::layout::Location::InFolder => None,
        crate::layout::Location::Fallback(reason) => Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_writes_are_the_bytes_the_app_wrote() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("board.json");
        let writes = OwnWrites::default();
        assert!(!writes.contains(&path), "nothing noted");
        writes.note(path.clone(), Some(b"mine"));
        std::fs::write(&path, "mine").expect("write");
        assert!(writes.contains(&path));
        std::fs::write(&path, "theirs").expect("write");
        assert!(!writes.contains(&path), "an outside edit right after is reported");
        writes.note(path.clone(), None);
        assert!(!writes.contains(&path), "not deleted yet");
        std::fs::remove_file(&path).expect("delete");
        assert!(writes.contains(&path));
    }

    struct Silent;

    impl EventSink for Silent {
        fn emit(&self, _event: &WorkspaceEvent) {}
    }

    fn shared(compiled: bool) -> (tempfile::TempDir, Shared) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        let layout = Layout::discover(&root, &root.join("fallback"));
        let shared = Shared::new(
            ProjectId::new_unchecked("p-aaaaaaaaaa"),
            root,
            WorkspaceOptions::default(),
            Arc::new(Silent),
            layout,
            LocalState::default(),
            compiled,
        );
        (dir, shared)
    }

    fn path(text: &str) -> RepoPath {
        RepoPath::new(text).expect("path")
    }

    #[test]
    fn a_newer_change_cancels_a_running_background_update() {
        let (_dir, shared) = shared(true);
        let background = CancelToken::new();
        lock(&shared.sched).running = Some(Running {
            cancel: background.clone(),
            background: true,
        });
        shared.add_changes([path("a.ts")]);
        assert!(background.is_cancelled());
        let sched = lock(&shared.sched);
        assert!(sched.pending.contains(&path("a.ts")));
        assert!(sched.last_change.is_some());
    }

    #[test]
    fn changes_never_cancel_a_compile_but_a_compile_cancels_background_work() {
        let (_dir, shared) = shared(true);
        let compile = CancelToken::new();
        lock(&shared.sched).running = Some(Running {
            cancel: compile.clone(),
            background: false,
        });
        shared.add_changes([path("a.ts")]);
        shared.request_rescan();
        assert!(!compile.is_cancelled());

        let background = CancelToken::new();
        lock(&shared.sched).running = Some(Running {
            cancel: background.clone(),
            background: true,
        });
        let (reply, _result) = std::sync::mpsc::channel();
        shared.push_job(Job::Compile(CompileJob {
            mode: crate::compile::CompileMode::Check,
            cancel: CancelToken::new(),
            progress: None,
            reply,
        }));
        assert!(background.is_cancelled());
        assert_eq!(lock(&shared.sched).jobs.len(), 1);
    }

    #[test]
    fn idleness_waits_for_due_work_but_not_for_paused_work() {
        let (_dir, shared) = shared(true);
        assert!(shared.wait_idle(Duration::from_millis(10)));
        assert!(shared.is_idle());
        shared.add_changes([path("a.ts")]);
        assert!(!shared.is_idle());
        // Nobody runs the queue here, so pending work keeps the folder busy.
        assert!(!shared.wait_idle(Duration::from_millis(20)));
        lock(&shared.sched).paused = true;
        assert!(shared.wait_idle(Duration::from_millis(10)));
        assert_eq!(lock(&shared.sched).changed(), 1);
        let (_dir, uncompiled) = self::shared(false);
        uncompiled.add_changes([path("b.ts")]);
        assert!(uncompiled.wait_idle(Duration::from_millis(10)));
    }

    #[test]
    fn scanning_replaces_events_once() {
        let (_dir, shared) = shared(true);
        shared.fall_back_to_scanning();
        assert_eq!(shared.status().watching, WatchMode::Scanning);
        assert!(lock(&shared.sched).rescan);
        lock(&shared.sched).rescan = false;
        shared.fall_back_to_scanning();
        assert!(!lock(&shared.sched).rescan);
    }
}
