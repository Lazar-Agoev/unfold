//! One open folder: [`Workspace`], the object every command for that folder goes through.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use unfold_core::RepoPath;
use unfold_core::highlight::Highlighter;
use unfold_core::vocab::{FileState, Language as LanguageId};
use unfold_engine::settings::{SETTINGS_PATH, Settings, SettingsError};
use unfold_engine::{CancelToken, Language};
use unfold_reader::ReaderIndex;
use unfold_store::{Opened, ReadPool};

use crate::claims::BoardClaims;
use crate::compile::{CompileJob, CompileMode, CompileTicket, ProgressFn};
use crate::confine::{ConfinedPath, confine};
use crate::engine_thread::EngineThread;
use crate::error::{Result, WorkspaceError};
use crate::events::{EventSink, WindowId, WorkspaceEvent};
use crate::layout::Layout;
use crate::options::{WorkspaceOptions, store_version};
use crate::problems::{IndexLogEntry, ProblemItem};
use crate::settings_edit::{SettingsEdit, apply_edits};
use crate::shared::{Job, Shared, lock};
use crate::state::{FolderState, read_json, write_atomically, write_json};
use crate::status::{Activity, GraphCounts, GraphStatus, WatchMode};
use crate::storage::open_store;
use crate::watcher;

/// What opening a folder needs besides its path.
#[derive(Clone)]
pub struct OpenContext {
    /// The parent of fallback `local/` folders.
    pub fallback_cache_dir: PathBuf,
    /// Timings and engine settings.
    pub options: WorkspaceOptions,
    /// Where events go.
    pub sink: Arc<dyn EventSink>,
    /// The languages to register.
    pub languages: Vec<Arc<dyn Language>>,
}

/// The result of a settings edit.
#[derive(Debug, Clone)]
pub struct SettingsEditOutcome {
    /// Whether `.unfold/config.toml` was written (unchanged text, or an absent file left at the
    /// defaults, writes nothing).
    pub written: bool,
    /// The file's text after the edit.
    pub text: String,
    /// The validated settings it holds.
    pub settings: Settings,
}

/// One open folder, shared by every window showing it (decision U-37). See the crate
/// documentation for the lifecycle.
pub struct Workspace {
    shared: Arc<Shared>,
    languages: Vec<Arc<dyn Language>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("folder", &self.shared.folder)
            .finish_non_exhaustive()
    }
}

/// Canonicalizes `root` and checks it is a folder.
pub(crate) fn canonical_folder(root: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(root).map_err(|error| WorkspaceError::NotAFolder {
        path: root.to_path_buf(),
        reason: error.to_string(),
    })?;
    if !canonical.is_dir() {
        return Err(WorkspaceError::NotAFolder {
            path: root.to_path_buf(),
            reason: "it is not a folder".into(),
        });
    }
    Ok(canonical)
}

impl Workspace {
    /// Opens the folder at `root`. Writes nothing unless the folder was compiled before (then it
    /// takes the folder lock); the store's graph, if any, is readable when this returns, and the
    /// reopen check runs in the background.
    pub fn open(root: &Path, context: OpenContext) -> Result<Arc<Self>> {
        let root = canonical_folder(root)?;
        let OpenContext {
            fallback_cache_dir,
            options,
            sink,
            languages,
        } = context;
        let mut layout = Layout::discover(&root, &fallback_cache_dir);
        let version = store_version(&languages);
        let compiled = layout.is_compiled();
        let mut store = None;
        let mut first_job = None;
        let mut counts = None;
        if compiled {
            let (opened, folder_lock) = open_store(&mut layout, &version)?;
            let reader = opened.readers.get()?;
            if opened.opened == Opened::Reused && reader.meta()?.graph.is_some() {
                counts = Some(graph_counts(&reader)?);
                first_job = Some(Job::Reopen);
            } else {
                first_job = Some(Job::Recompile);
            }
            drop(reader);
            store = Some((opened, folder_lock));
        }
        let state_file = layout.state_file();
        let folder_state = if state_file.exists() {
            read_json(&state_file)
        } else {
            FolderState {
                background_updates: options.background_updates_default,
                ..FolderState::default()
            }
        };
        let shared = Arc::new(Shared::new(
            root,
            options,
            sink,
            layout,
            folder_state,
            compiled,
            WatchMode::Off,
        ));
        shared.update_status(|model| {
            if let Some(counts) = counts.take() {
                model.counts = counts;
            }
            model.activity = match first_job {
                Some(Job::Reopen) => Activity::Updating { files: None },
                Some(_) => Activity::Compiling { done: 0, total: 0 },
                None => Activity::Idle,
            };
        });
        let thread = EngineThread::new(Arc::clone(&shared), languages.clone(), version, store)?;
        let watching = watcher::start(&shared);
        lock(&shared.sched).watch = watching;
        shared.update_status(|model| model.watching = watching);
        if let Some(job) = first_job {
            shared.push_job(job);
        }
        let handle = thread.spawn().map_err(|error| {
            watcher::stop(&shared);
            WorkspaceError::Internal(format!("cannot start the engine thread: {error}"))
        })?;
        Ok(Arc::new(Self {
            shared,
            languages,
            thread: Mutex::new(Some(handle)),
        }))
    }

    /// The folder's canonical path.
    pub fn root(&self) -> &Path {
        &self.shared.root
    }

    /// The folder's canonical path as text: how events name it.
    pub fn folder(&self) -> &str {
        &self.shared.folder
    }

    /// The folder's name.
    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// The folder's `.unfold/` layout (where `local/` is right now).
    pub fn layout(&self) -> Layout {
        lock(&self.shared.layout).clone()
    }

    /// Whether the folder has a graph or one is being built.
    pub fn is_compiled(&self) -> bool {
        lock(&self.shared.sched).compiled
    }

    /// The windows attached to the folder, sorted.
    pub fn windows(&self) -> Vec<WindowId> {
        lock(&self.shared.windows).iter().copied().collect()
    }

    /// Attaches `window`, so it receives the folder's events. The [`crate::Registry`] does this;
    /// call it directly only when using a workspace without one.
    pub fn attach(&self, window: WindowId) {
        lock(&self.shared.windows).insert(window);
    }

    /// Detaches `window`, releasing its boards; returns how many windows remain.
    pub fn detach(&self, window: WindowId) -> usize {
        self.shared.claims.release_window(window);
        let mut windows = lock(&self.shared.windows);
        windows.remove(&window);
        windows.len()
    }

    /// The status bar's state.
    pub fn status(&self) -> GraphStatus {
        self.shared.status()
    }

    /// Queues a Compile. The first Compile creates `.unfold/` (or the fallback location).
    /// `progress` receives every progress report, for a streamed command.
    pub fn compile(&self, mode: CompileMode, progress: Option<ProgressFn>) -> CompileTicket {
        let (reply, result) = mpsc::channel();
        let cancel = CancelToken::new();
        if lock(&self.shared.sched).shutdown {
            let _ = reply.send(Err(WorkspaceError::Closed));
        } else {
            self.shared.push_job(Job::Compile(CompileJob {
                mode,
                cancel: cancel.clone(),
                progress,
                reply,
            }));
        }
        CompileTicket::new(cancel, result)
    }

    /// Cancels the running Compile and any queued ones; whether there was one.
    pub fn cancel_compile(&self) -> bool {
        let sched = lock(&self.shared.sched);
        let mut found = false;
        if let Some(running) = sched.running.as_ref().filter(|running| !running.background) {
            running.cancel.cancel();
            found = true;
        }
        for job in &sched.jobs {
            if let Job::Compile(compile) = job {
                compile.cancel.cancel();
                found = true;
            }
        }
        found
    }

    /// Turns background updates on or off for this folder, and remembers it in `state.json`.
    pub fn set_background_updates(&self, on: bool) -> Result<()> {
        self.apply_paused(!on);
        self.update_folder_state(|state| state.background_updates = on)
            .map(|_| ())
    }

    /// Whether background updates are on.
    pub fn background_updates(&self) -> bool {
        !lock(&self.shared.sched).paused
    }

    /// The timings and engine settings the folder was opened with (discovery's ignore options
    /// among them, which the Files view and text search share).
    pub fn options(&self) -> &WorkspaceOptions {
        &self.shared.options
    }

    /// The changed paths not yet in the graph (waiting for, or paused before, a background
    /// update) at or under the folder-relative `folder` (`""` for everything), sorted.
    pub fn pending_under(&self, folder: &str) -> Vec<String> {
        let sched = lock(&self.shared.sched);
        sched
            .pending
            .iter()
            .map(RepoPath::as_str)
            .filter(|path| {
                folder.is_empty()
                    || path
                        .strip_prefix(folder)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
            .map(str::to_owned)
            .collect()
    }

    fn apply_paused(&self, paused: bool) {
        let mut sched = lock(&self.shared.sched);
        if sched.paused == paused {
            return;
        }
        sched.paused = paused;
        if paused && let Some(running) = sched.running.as_ref().filter(|running| running.background) {
            running.cancel.cancel();
        }
        let changed = sched.changed();
        drop(sched);
        self.shared.wake.notify_all();
        self.shared.update_status(|model| {
            model.paused = paused;
            model.changed = changed;
        });
    }

    /// Reports changed folder-relative paths, as the watcher does. Useful where the watcher is
    /// off, and after the app itself writes files the graph reads.
    pub fn notify_changes(&self, paths: impl IntoIterator<Item = RepoPath>) {
        self.shared.add_changes(paths);
    }

    /// Asks for a check of every file by size and modification time.
    pub fn rescan(&self) {
        self.shared.request_rescan();
    }

    /// Waits until nothing is running, queued, or due (pending changes wait for their quiet
    /// period first), or `timeout` passes; whether it got there.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        self.shared.wait_idle(timeout)
    }

    /// Read-only access to the committed graph. Fails before the first Compile.
    pub fn readers(&self) -> Result<ReadPool> {
        self.shared.readers().ok_or(WorkspaceError::NotCompiled)
    }

    /// The whole-graph index of the committed graph (callers, closures, reached-from).
    pub fn reader_index(&self) -> Result<Arc<ReaderIndex>> {
        let pool = self.readers()?;
        Ok(self.shared.reader_cache.get(&pool)?)
    }

    /// The Problems list.
    pub fn problems(&self) -> Vec<ProblemItem> {
        self.shared.problems()
    }

    /// Replaces the board-health problems (from the board commands).
    pub fn set_board_problems(&self, problems: Vec<ProblemItem>) {
        self.shared.update_problems(|all| all.boards = problems);
    }

    /// The Index log, oldest first.
    pub fn index_log(&self) -> Vec<IndexLogEntry> {
        lock(&self.shared.index_log).entries()
    }

    /// The text of `.unfold/config.toml`, or `None` when it doesn't exist.
    pub fn settings_text(&self) -> Result<Option<String>> {
        let path = lock(&self.shared.layout).config();
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(WorkspaceError::io(path, error)),
        }
    }

    /// The settings on disk now, validated.
    pub fn settings(&self) -> Result<Settings> {
        Settings::load(&self.shared.root).map_err(|error| match error {
            SettingsError::Invalid {
                message,
                line,
                column,
                ..
            } => WorkspaceError::InvalidSettings {
                message: format!("{SETTINGS_PATH}:{line}:{column}: {message}"),
                line: Some(line),
                column: Some(column),
            },
            other => WorkspaceError::InvalidSettings {
                message: other.to_string(),
                line: None,
                column: None,
            },
        })
    }

    /// Applies validated settings edits, preserving comments and formatting, and writes the file
    /// atomically when it changes (never when an absent file would hold only defaults). Sends
    /// `config_changed` and queues the settings for the next background update.
    pub fn edit_settings(&self, edits: &[SettingsEdit]) -> Result<SettingsEditOutcome> {
        let _editing = lock(&self.shared.settings_edit);
        let current = self.settings_text()?;
        let edited = apply_edits(current.as_deref(), edits)?;
        let unchanged = current.as_deref() == Some(edited.text.as_str())
            || (current.is_none() && edited.settings == Settings::default());
        if !unchanged {
            // A `.unfold` link that leaves the folder must never be written through.
            confine(&self.shared.root, crate::layout::CONFIG_PATH)?;
            let path = lock(&self.shared.layout).config();
            self.shared.own_writes.note(path.clone());
            write_atomically(&path, edited.text.as_bytes())?;
            self.shared.emit(&WorkspaceEvent::ConfigChanged {
                folder: self.shared.folder.clone(),
            });
            if let Ok(settings_path) = RepoPath::new(SETTINGS_PATH) {
                self.shared.add_changes([settings_path]);
            }
        }
        Ok(SettingsEditOutcome {
            written: !unchanged,
            text: edited.text,
            settings: edited.settings,
        })
    }

    /// The folder's remembered state.
    pub fn folder_state(&self) -> FolderState {
        lock(&self.shared.folder_state).clone()
    }

    /// Changes the remembered state and saves it to `.unfold/local/state.json`, or keeps it in
    /// memory until the first Compile creates `local/`.
    pub fn update_folder_state(&self, change: impl FnOnce(&mut FolderState)) -> Result<FolderState> {
        // The file is written while the state is locked, so writes land in the order the changes
        // were made: an older state can never overwrite a newer one.
        let (state, written) = {
            let mut state = lock(&self.shared.folder_state);
            change(&mut state);
            let layout = lock(&self.shared.layout).clone();
            let written = if layout.is_compiled() {
                write_json(&layout.state_file(), &*state, "state.json")
            } else {
                Ok(())
            };
            (state.clone(), written)
        };
        self.apply_paused(!state.background_updates);
        written?;
        Ok(state)
    }

    /// Checks a path a command named against the folder (architecture §13).
    pub fn confine(&self, path: &str) -> Result<ConfinedPath> {
        confine(&self.shared.root, path)
    }

    /// Which window holds each open board.
    pub fn claims(&self) -> &BoardClaims {
        &self.shared.claims
    }

    /// Moves `board` from window `from` to window `to` (Move to New Window); `to` must show
    /// this folder.
    pub fn transfer_board(&self, board: &str, from: WindowId, to: WindowId) -> Result<()> {
        if !lock(&self.shared.windows).contains(&to) {
            return Err(WorkspaceError::NotAttached(to));
        }
        self.shared.claims.transfer(board, from, to)
    }

    /// The registered language with `id`.
    pub fn language(&self, id: LanguageId) -> Option<&Arc<dyn Language>> {
        self.languages.iter().find(|language| language.id() == id)
    }

    /// The highlighter for a language's Code view (the reader takes it as a trait object).
    pub fn highlighter(&self, id: LanguageId) -> Option<&dyn Highlighter> {
        self.language(id).map(|language| language.highlighter())
    }

    /// Notes that the app is about to write `path`, so the watcher doesn't report the write as
    /// an outside edit (for drafts and published boards).
    pub fn note_own_write(&self, path: &Path) {
        self.shared.own_writes.note(path.to_path_buf());
    }

    /// Sends an event to every window of the folder (for commands that change boards).
    pub fn emit(&self, event: &WorkspaceEvent) {
        self.shared.emit(event);
    }

    /// Stops the engine thread (cancelling its work) and the watcher. Idempotent.
    pub fn close(&self) {
        {
            let mut sched = lock(&self.shared.sched);
            sched.shutdown = true;
            if let Some(running) = &sched.running {
                running.cancel.cancel();
            }
        }
        self.shared.wake.notify_all();
        watcher::stop(&self.shared);
        let handle = lock(&self.thread).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.close();
    }
}

/// Counts for the status bar from the stored graph's files.
fn graph_counts(reader: &unfold_store::Reader) -> Result<GraphCounts> {
    let rows = reader.files()?;
    let mut languages: Vec<LanguageId> = rows.iter().map(|row| row.language).collect();
    languages.sort();
    languages.dedup();
    Ok(GraphCounts {
        files: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        not_current: u32::try_from(rows.iter().filter(|row| row.state != FileState::Current).count())
            .unwrap_or(u32::MAX),
        languages,
    })
}
