//! One live project: [`Workspace`], the open engine, watcher, store writer, and reader cache of a
//! project's current folder, which every command for that project goes through. The workspace
//! knows nothing of windows: [`crate::projects::Projects`] opens and closes it as the project
//! wakes and rests, and its events go to the window.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use unfold_core::RepoPath;
use unfold_core::highlight::Highlighter;
use unfold_core::vocab::{FileState, Language as LanguageId};
use unfold_engine::settings::edit::{EntryEdit, SettingsEdit, apply_edits};
use unfold_engine::settings::{SETTINGS_PATH, SettingsError, SettingsFile};
use unfold_engine::{CancelToken, Language};
use unfold_reader::ReaderIndex;
use unfold_reader::source::SourceCache;
use unfold_store::{Opened, ReadPool};

use crate::compile::{CompileJob, CompileTicket, ProgressFn};
use crate::confine::{ConfinedPath, confine};
use crate::engine_thread::EngineThread;
use crate::error::{Result, WorkspaceError};
use crate::events::{EventSink, WorkspaceEvent};
use crate::layout::Layout;
use crate::options::{WorkspaceOptions, store_version};
use crate::problems::ProblemItem;
use crate::projects::ProjectId;
use crate::shared::{Job, Resolved, Shared, lock};
use crate::state::{LocalState, write_atomically, write_json};
use crate::status::{Activity, GraphCounts, GraphStatus, ProjectState};
use crate::storage::open_store;
use crate::watcher;

/// What opening a project's folder needs besides its path.
#[derive(Clone)]
pub struct OpenContext {
    /// The project the folder is opened for: its events name it.
    pub project: ProjectId,
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
    /// Whether `unfold.toml` was written (unchanged text, or an absent file left at the
    /// defaults, writes nothing).
    pub written: bool,
    /// The file's text after the edit.
    pub text: String,
    /// The validated settings and apps it holds.
    pub file: SettingsFile,
    /// The unified diff from the old text to the new; empty when nothing changed.
    pub diff: String,
    /// What each app and entrance edit did, in order.
    pub entries: Vec<EntryEdit>,
}

/// One live project's folder. See the [module documentation](self) and the crate documentation
/// for the lifecycle.
pub struct Workspace {
    shared: Arc<Shared>,
    languages: Vec<Arc<dyn Language>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("project", &self.shared.project)
            .field("root", &self.shared.root)
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
            project,
            options,
            sink,
            languages,
        } = context;
        let layout = Layout::new(&root);
        let version = store_version(&languages);
        let compiled = layout.is_compiled();
        let mut store = None;
        let mut first_job = None;
        let mut counts = None;
        if compiled {
            let (opened, folder_lock) = open_store(&layout, &version)?;
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
        let local_state = LocalState::read(&layout.state_file());
        let shared = Arc::new(Shared::new(
            project,
            root,
            options,
            sink,
            layout,
            local_state,
            compiled,
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

    /// The project this folder is open for.
    pub fn project(&self) -> &ProjectId {
        &self.shared.project
    }

    /// The folder's canonical path.
    pub fn root(&self) -> &Path {
        &self.shared.root
    }

    /// The folder's `.unfold/` layout.
    pub fn layout(&self) -> &Layout {
        &self.shared.layout
    }

    /// Whether the folder has a graph or one is being built.
    pub fn is_compiled(&self) -> bool {
        lock(&self.shared.sched).compiled
    }

    /// The graph's status (Home's state line).
    pub fn status(&self) -> GraphStatus {
        self.shared.status()
    }

    /// The project's row state now (crates/unfold-workspace/src/status.rs).
    pub fn project_state(&self) -> ProjectState {
        self.shared.project_states().0
    }

    /// The row state with no compile or update running: what the project remembers when it goes
    /// to rest (`lastKnown`).
    pub fn settled_state(&self) -> ProjectState {
        self.shared.project_states().1
    }

    /// The committed graph's file count, the files that don't parse, and its languages, as the
    /// status counts them.
    pub fn graph_counts(&self) -> (u32, u32, Vec<LanguageId>) {
        let counts = lock(&self.shared.status).model.counts.clone();
        (counts.files, counts.not_current, counts.languages)
    }

    /// Queues a Compile. The first Compile creates `.unfold/`. `progress` receives every
    /// progress report, for a streamed command.
    pub fn compile(&self, progress: Option<ProgressFn>) -> CompileTicket {
        let (reply, result) = mpsc::channel();
        let cancel = CancelToken::new();
        // A closed workspace drops the job, and the ticket resolves as closed.
        self.shared.push_job(Job::Compile(CompileJob {
            cancel: cancel.clone(),
            progress,
            reply,
        }));
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

    /// The timings and engine settings the folder was opened with.
    pub fn options(&self) -> &WorkspaceOptions {
        &self.shared.options
    }

    /// Reports changed folder-relative paths, as the watcher does: how tests that open folders
    /// with the watcher off ([`WorkspaceOptions::watch`]) drive background updates.
    pub fn notify_changes(&self, paths: impl IntoIterator<Item = RepoPath>) {
        self.shared.add_changes(paths);
    }

    /// Asks for a check of every file by size and modification time.
    pub fn rescan(&self) {
        self.shared.request_rescan();
    }

    /// Whether nothing is running, queued, or due: a project may rest only then.
    pub fn is_idle(&self) -> bool {
        self.shared.is_idle()
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

    /// Replaces the board-health problems (from the board commands' health refresh): the
    /// Problems list includes them; the row's state doesn't count them.
    pub fn set_board_problems(&self, problems: Vec<ProblemItem>) {
        self.shared.update_problems(|all| all.boards = problems);
    }

    /// The text of `unfold.toml`, or `None` when it doesn't exist.
    pub fn settings_text(&self) -> Result<Option<String>> {
        settings_text(&self.shared.root)
    }

    /// The settings file on disk now, validated.
    pub fn settings(&self) -> Result<SettingsFile> {
        load_settings(&self.shared.root)
    }

    /// Applies validated settings edits, preserving comments and formatting, and writes the file
    /// atomically when it changes (never when an absent file would hold only defaults). Sends
    /// `config_changed`. An edit that leaves the compile settings as they were (it changed only
    /// `[[app]]` tables) resolves the entrances again without compiling, and returns once they
    /// are (`entrances_changed` follows when they changed); any other edit queues the settings
    /// for the next background update.
    pub fn edit_settings(&self, edits: &[SettingsEdit]) -> Result<SettingsEditOutcome> {
        let _editing = lock(&self.shared.settings_edit);
        let current = self.settings_text()?;
        let edited = apply_edits(current.as_deref(), edits)?;
        let unchanged = current.as_deref() == Some(edited.text.as_str())
            || (current.is_none() && edited.file == SettingsFile::default());
        if !unchanged {
            let before = match current.as_deref() {
                // `apply_edits` refuses a file that doesn't parse.
                Some(text) => SettingsFile::parse(text).map_err(settings_error)?,
                None => SettingsFile::default(),
            };
            // An `unfold.toml` link that leaves the folder must never be written through.
            confine(&self.shared.root, crate::layout::CONFIG_PATH)?;
            let path = self.shared.layout.config();
            self.shared
                .own_writes
                .note(path.clone(), Some(edited.text.as_bytes()));
            write_atomically(&path, edited.text.as_bytes())?;
            self.shared.emit(&WorkspaceEvent::ConfigChanged {
                project: self.shared.project.clone(),
            });
            if before.settings == edited.file.settings {
                let (reply, resolved) = mpsc::channel();
                // A closed workspace queues nothing, and a closing one drops the reply: there
                // is nothing to wait for then.
                if self.shared.push_job(Job::ResolveEntrances(reply)) {
                    let _ = resolved.recv();
                }
            } else {
                self.shared.add_changes(RepoPath::new(SETTINGS_PATH).ok());
            }
        }
        Ok(SettingsEditOutcome {
            written: !unchanged,
            text: edited.text,
            file: edited.file,
            diff: edited.diff,
            entries: edited.entries,
        })
    }

    /// The entrances the engine last resolved; `None` until it has read `unfold.toml` (just
    /// after the project wakes, or before its first Compile).
    pub(crate) fn entrances(&self) -> Option<Arc<Resolved>> {
        self.shared.entrances()
    }

    /// The project's remembered state (`.unfold/local/state.json`).
    pub fn local_state(&self) -> LocalState {
        lock(&self.shared.local_state).clone()
    }

    /// Changes the remembered state and saves it to `.unfold/local/state.json`, creating
    /// `local/` first when the folder has none yet ([`Layout::prepare`]).
    pub fn update_local_state(&self, change: impl FnOnce(&mut LocalState)) -> Result<LocalState> {
        // The file is written while the state is locked, so writes land in the order the changes
        // were made: an older state can never overwrite a newer one.
        let (state, written) = {
            let mut state = lock(&self.shared.local_state);
            change(&mut state);
            let layout = &self.shared.layout;
            let written = if layout.is_compiled() {
                Ok(())
            } else {
                layout.prepare()
            }
            .and_then(|()| write_json(&layout.state_file(), &*state, "state.json"));
            (state.clone(), written)
        };
        written?;
        Ok(state)
    }

    /// Checks a path a command named against the project's folder (docs/architecture.md#security).
    pub fn confine(&self, path: &str) -> Result<ConfinedPath> {
        confine(&self.shared.root, path)
    }

    /// The registered language with `id`.
    pub fn language(&self, id: LanguageId) -> Option<&Arc<dyn Language>> {
        self.languages.iter().find(|language| language.id() == id)
    }

    /// The sources the drawings and the code panel verified most recently ([`SourceCache`]).
    pub fn kept_sources(&self) -> &SourceCache {
        &self.shared.sources
    }

    /// Every registered language's highlighter, for the code panel and the drawings.
    pub fn highlighters(&self) -> Vec<&dyn Highlighter> {
        self.languages
            .iter()
            .map(|language| language.highlighter())
            .collect()
    }

    /// Notes that the app is about to write `bytes` to `path` (`None`: delete it), so the
    /// watcher doesn't report that write as an outside edit (for board files).
    /// A different change to the file soon after is still reported.
    pub fn note_own_write(&self, path: &Path, bytes: Option<&[u8]>) {
        self.shared.own_writes.note(path.to_path_buf(), bytes);
    }

    /// Sends an event to every window (for commands that change boards).
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

/// The text of `<root>/unfold.toml`, or `None` when it doesn't exist.
pub(crate) fn settings_text(root: &Path) -> Result<Option<String>> {
    let path = root.join(crate::layout::CONFIG_PATH);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(WorkspaceError::io(path, error)),
    }
}

/// The settings file of the folder at `root` on disk now, validated.
pub(crate) fn load_settings(root: &Path) -> Result<SettingsFile> {
    SettingsFile::load(root).map_err(settings_error)
}

/// A settings file that can't be read or is invalid, with its position.
pub(crate) fn settings_error(error: SettingsError) -> WorkspaceError {
    match error {
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
    }
}

/// Counts for Home's state line from the stored graph's files.
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
