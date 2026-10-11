//! The workspace's projects (docs/ui-shell.md#state-and-engine-data–11.2; decisions project-scoped commands, shared project names, project liveness): one per
//! [`crate::host::Host`], owned by it.
//!
//! ```text
//! Projects
//!   workspace.json   (cached in memory, written on every change)
//!   entries: ProjectId -> ProjectEntry {
//!       live:      Option<Arc<Workspace>>   the open engine, watcher, store writer, and lock
//!       resting:   Option<RestingReader>    a read-only pool on the stored graph, opened lazily
//!       extras:    Arc<ProjectExtras>       command-side state, dropped on rest and switch
//!       claims:    BoardClaims              board id -> window (and the projects its cards use)
//!       interest:  compile pins, lease_until
//!   }
//!   selection: WindowId -> ProjectId         each window's Home selection
//! ```
//!
//! # Live and resting projects
//!
//! A project is **live** while something needs it, and **rests** otherwise (project liveness: Rust
//! owns liveness; the router knows nothing of projects):
//!
//! ```text
//! interest(p) = a window holds a board of p || a window has p selected
//!            || an open board (of any project) has cards from p || a compile of p is running
//! wake(p)     on a command that uses p's engine (ProjectHandle::wake), select_project, and
//!             gaining interest otherwise (the reaper's next tick); opens the Workspace (readable
//!             at once; the reopen check runs in the background). A wake that fails marks the row
//!             unavailable.
//! lease       every wake and every loss of interest: lease_until = max(lease_until, now + rest_after)
//! rest(p)     live && !interest && now >= lease_until && the engine is idle: remembers the row's
//!             state in workspace.json (lastKnown), closes the Workspace, drops the extras
//! reaper      a thread ("unfold-liveness") tends every project each reap_interval: rests it, or
//!             wakes it when it gained interest while resting; and closes resting read pools
//!             unused for a minute
//! ```
//!
//! A resting project answers the commands that can read from disk ([`ProjectHandle::readers`]
//! opens `graph.sqlite` read-only through `ReadPool::open_existing`, never waking it). A result
//! from it wakes it when used: every command that takes a key or a path of a project wakes it.
//!
//! # Locks
//!
//! Each entry has a `life` lock, held across a wake, a rest, a switch, or a removal (the slow
//! parts: opening and closing a `Workspace`). The order is `life` → `inner` → an entry's `state`
//! → its claims; nothing calls into a `Workspace` while holding `inner`, and events are emitted
//! with `inner` released, so a workspace's event (which may reach back into the registry) never
//! waits on it.

use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use unfold_store::ReadPool;

use crate::claims::BoardClaims;
use crate::commands::ProjectExtras;
use crate::commit::now_ms;
use crate::confine::{ConfinedPath, confine};
use crate::error::{Result, WorkspaceError};
use crate::events::{EventSink, WindowId, WorkspaceEvent};
use crate::git::{self, GitSummary, RepositoryId};
use crate::layout::Layout;
use crate::options::{AppConfig, WorkspaceOptions, default_languages, store_version};
use crate::shared::lock;
use crate::state::{AppData, read_json, write_json};
use crate::status::ProjectState;
use crate::workspace::{OpenContext, Workspace, canonical_folder, load_settings};

/// The most projects a workspace holds.
pub const MAX_PROJECTS: usize = 64;
/// The most recent boards `workspace.json` keeps.
pub const MAX_RECENT_BOARDS: usize = 20;
/// The most removed folders "Opened before" keeps.
pub const MAX_REMOVED: usize = 5;
/// How long a resting project's read pool stays open unused.
const RESTING_POOL_IDLE: Duration = Duration::from_secs(60);
/// Why a row whose folder is missing is unavailable.
const FOLDER_GONE: &str = "The folder is gone or can't be read.";

/// A project's id: `p-` and 10 characters of `[a-z2-7]`, random, assigned when the project is
/// added (shared project names). It survives restarts and worktree switches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct ProjectId(String);

impl ProjectId {
    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A new random id that `taken` doesn't know.
    fn generate(taken: impl Fn(&ProjectId) -> bool) -> Self {
        const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        loop {
            let mut digest = Sha256::new();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            digest.update(nanos.to_le_bytes());
            digest.update(std::process::id().to_le_bytes());
            digest.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
            digest.update(RandomState::new().hash_one(nanos).to_le_bytes());
            let mut id = String::from("p-");
            id.extend(
                digest
                    .finalize()
                    .iter()
                    .take(10)
                    .map(|byte| char::from(ALPHABET[usize::from(byte & 31)])),
            );
            let id = Self(id);
            if !taken(&id) {
                return id;
            }
        }
    }

    /// An id from text, unchecked (tests).
    #[cfg(test)]
    pub(crate) fn new_unchecked(text: &str) -> Self {
        Self(text.to_owned())
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A board opened recently, in any project (`workspace.json` `recentBoards`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecentBoard {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub board: String,
    /// Its title when it was opened.
    pub title: String,
    /// When it was opened, in milliseconds since the Unix epoch.
    pub opened_at_ms: i64,
}

/// A folder removed from the workspace, which "Opened before" offers to add back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemovedProject {
    /// The folder's canonical path (what `readd_project` takes).
    pub path: String,
    /// Its shared name when it was removed.
    pub name: String,
    /// When it was removed, in milliseconds since the Unix epoch.
    pub removed_at_ms: i64,
}

/// One project row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    /// The project's id.
    pub id: ProjectId,
    /// Its shared name.
    pub name: String,
    /// The canonical path of the worktree folder it points at.
    pub path: String,
    /// Whether the row is expanded in the sidebar.
    pub expanded: bool,
    /// Whether the project is live (its engine is open).
    pub live: bool,
    /// Its state.
    pub state: ProjectState,
    /// Its repository's branch and worktree count, or `None` outside a repository.
    pub git: Option<GitSummary>,
}

/// The whole workspace, as every window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    /// The projects, in sidebar order.
    pub projects: Vec<ProjectRow>,
    /// The project last selected in any window: the default for a new window.
    pub selected: Option<ProjectId>,
    /// Recent boards, most recent first.
    pub recent_boards: Vec<RecentBoard>,
    /// Folders removed from the workspace, most recent first ("Opened before").
    pub removed: Vec<RemovedProject>,
}

/// Whether `add_project` added a row or found the folder's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AddStatus {
    /// A new row.
    Added,
    /// The folder, or another worktree of its repository, already has a row.
    Exists,
}

/// What `add_project` and `readd_project` return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AddedProject {
    /// Whether the row is new.
    pub status: AddStatus,
    /// The row.
    pub project: ProjectRow,
}

/// `workspace.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WorkspaceFile {
    version: u32,
    projects: Vec<ProjectRecord>,
    selected: Option<ProjectId>,
    recent_boards: Vec<RecentBoard>,
    removed: Vec<RemovedRecord>,
}

impl Default for WorkspaceFile {
    fn default() -> Self {
        Self {
            version: 1,
            projects: Vec::new(),
            selected: None,
            recent_boards: Vec::new(),
            removed: Vec::new(),
        }
    }
}

impl WorkspaceFile {
    /// Drops what the file can't hold: duplicate ids, rows past the limit, and references to
    /// projects that aren't in it.
    fn sanitize(&mut self) {
        let mut seen = std::collections::BTreeSet::new();
        self.projects.retain(|record| seen.insert(record.id.clone()));
        self.projects.truncate(MAX_PROJECTS);
        let known = |id: &ProjectId| self.projects.iter().any(|record| record.id == *id);
        if self.selected.as_ref().is_some_and(|id| !known(id)) {
            self.selected = None;
        }
        let recents: Vec<RecentBoard> = std::mem::take(&mut self.recent_boards)
            .into_iter()
            .filter(|recent| known(&recent.project))
            .take(MAX_RECENT_BOARDS)
            .collect();
        self.recent_boards = recents;
        self.removed.truncate(MAX_REMOVED);
    }

    fn record(&self, id: &ProjectId) -> Option<&ProjectRecord> {
        self.projects.iter().find(|record| record.id == *id)
    }

    fn record_mut(&mut self, id: &ProjectId) -> Option<&mut ProjectRecord> {
        self.projects.iter_mut().find(|record| record.id == *id)
    }
}

/// One `workspace.json` project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRecord {
    id: ProjectId,
    path: String,
    name: String,
    expanded: bool,
    repository: Option<RepositoryId>,
    added_at_ms: i64,
    last_known: Option<LastKnown>,
}

/// The state a project had when it last went to rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LastKnown {
    state: ProjectState,
    at_ms: i64,
}

/// One removed folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemovedRecord {
    path: String,
    name: String,
    removed_at_ms: i64,
}

/// One project. See the [module documentation](self).
pub(crate) struct ProjectEntry {
    id: ProjectId,
    /// Held across a wake, a rest, a switch, or a removal.
    life: Mutex<()>,
    state: Mutex<EntryState>,
    claims: BoardClaims,
}

/// What a project entry holds besides its claims.
struct EntryState {
    /// The canonical folder the project points at.
    root: PathBuf,
    live: Option<Arc<Workspace>>,
    resting: Option<RestingReader>,
    extras: Arc<ProjectExtras>,
    /// Compiles running.
    pins: u32,
    lease_until: Instant,
    /// Whether the reaper saw interest last time (so it notices losing it).
    interested: bool,
    /// Why the last wake failed, until one succeeds.
    unavailable: Option<String>,
    /// The project left the workspace.
    removed: bool,
}

/// A resting project's read-only pool on its stored graph (`None`: no usable graph).
struct RestingReader {
    pool: Option<ReadPool>,
    used: Instant,
}

impl ProjectEntry {
    fn new(id: ProjectId, root: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            id,
            life: Mutex::new(()),
            state: Mutex::new(EntryState {
                root,
                live: None,
                resting: None,
                extras: Arc::default(),
                pins: 0,
                lease_until: Instant::now(),
                interested: false,
                unavailable: None,
                removed: false,
            }),
            claims: BoardClaims::new(),
        })
    }
}

/// The registry's shared state.
struct Inner {
    file: WorkspaceFile,
    entries: BTreeMap<ProjectId, Arc<ProjectEntry>>,
    selection: BTreeMap<WindowId, ProjectId>,
}

/// Called after a project woke (the host starts its board-health refresh).
pub(crate) type WakeHook = Box<dyn Fn(&ProjectId) + Send + Sync>;

/// The workspace's projects. See the [module documentation](self).
pub struct Projects {
    config: AppConfig,
    app: AppData,
    options: WorkspaceOptions,
    sink: Arc<dyn EventSink>,
    on_wake: WakeHook,
    /// The store version resting read pools check.
    version: String,
    inner: Mutex<Inner>,
    /// The app is quitting: nothing wakes any more.
    shut: AtomicBool,
}

impl std::fmt::Debug for Projects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Projects")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Locks an entry's `life`, recovering from a panicked holder.
fn life(entry: &ProjectEntry) -> MutexGuard<'_, ()> {
    lock(&entry.life)
}

/// The words a row shows for a project that can't be opened.
fn unavailable_reason(error: &WorkspaceError) -> String {
    match error {
        WorkspaceError::NotAFolder { .. } => FOLDER_GONE.to_owned(),
        WorkspaceError::OpenElsewhere { .. } => "Another copy of Unfold has it open.".to_owned(),
        other => other.to_string(),
    }
}

/// The shared name of the canonical folder `root`: `[project] name`, or the default (shared project names).
pub(crate) fn shared_name(root: &Path) -> String {
    load_settings(root)
        .ok()
        .and_then(|settings| settings.project_name)
        .unwrap_or_else(|| git::default_name(root))
}

impl Projects {
    /// The projects of the workspace in `config.app_data_dir`, every one resting.
    pub(crate) fn new(
        config: AppConfig,
        sink: Arc<dyn EventSink>,
        options: WorkspaceOptions,
        on_wake: WakeHook,
    ) -> Self {
        let app = AppData::new(config.app_data_dir.clone());
        let mut file: WorkspaceFile = read_json(&app.dir().join("workspace.json"));
        file.sanitize();
        let entries = file
            .projects
            .iter()
            .map(|record| {
                (
                    record.id.clone(),
                    ProjectEntry::new(record.id.clone(), PathBuf::from(&record.path)),
                )
            })
            .collect();
        Self {
            version: store_version(&default_languages()),
            config,
            app,
            options,
            sink,
            on_wake,
            inner: Mutex::new(Inner {
                file,
                entries,
                selection: BTreeMap::new(),
            }),
            shut: AtomicBool::new(false),
        }
    }

    /// The app folders.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// App preferences and remembered windows.
    pub fn app(&self) -> &AppData {
        &self.app
    }

    /// Writes `workspace.json`. Called with `inner` locked, so writes land in order.
    fn save(&self, inner: &Inner) -> Result<()> {
        write_json(
            &self.app.dir().join("workspace.json"),
            &inner.file,
            "workspace.json",
        )
    }

    /// Sends `workspace_changed` with the workspace now. Call with `inner` released.
    fn announce(&self) {
        self.sink.emit(&WorkspaceEvent::WorkspaceChanged {
            workspace: self.view(),
        });
    }

    /// Every project id, in workspace order.
    pub fn ids(&self) -> Vec<ProjectId> {
        lock(&self.inner)
            .file
            .projects
            .iter()
            .map(|record| record.id.clone())
            .collect()
    }

    /// The project with `id` (without waking it).
    pub fn handle(&self, id: &ProjectId) -> Result<ProjectHandle<'_>> {
        let (entry, name) = {
            let inner = lock(&self.inner);
            let entry = inner
                .entries
                .get(id)
                .cloned()
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            let name = inner
                .file
                .record(id)
                .map(|record| record.name.clone())
                .unwrap_or_default();
            (entry, name)
        };
        let root = lock(&entry.state).root.clone();
        Ok(ProjectHandle {
            projects: self,
            entry,
            root,
            name,
        })
    }

    /// The first project in workspace order whose shared name is `name` (shared project names): what a
    /// board's `ref.project` names.
    pub fn resolve_name(&self, name: &str) -> Option<ProjectId> {
        lock(&self.inner)
            .file
            .projects
            .iter()
            .find(|record| record.name == name)
            .map(|record| record.id.clone())
    }

    /// The workspace as every window shows it. Reads every repository's `HEAD`.
    pub fn view(&self) -> WorkspaceView {
        let (rows, selected, recent_boards, removed) = {
            let inner = lock(&self.inner);
            let rows: Vec<(ProjectRecord, Arc<ProjectEntry>)> = inner
                .file
                .projects
                .iter()
                .filter_map(|record| {
                    inner
                        .entries
                        .get(&record.id)
                        .map(|entry| (record.clone(), Arc::clone(entry)))
                })
                .collect();
            let removed = inner
                .file
                .removed
                .iter()
                .map(|removed| RemovedProject {
                    path: removed.path.clone(),
                    name: removed.name.clone(),
                    removed_at_ms: removed.removed_at_ms,
                })
                .collect();
            (
                rows,
                inner.file.selected.clone(),
                inner.file.recent_boards.clone(),
                removed,
            )
        };
        WorkspaceView {
            projects: rows
                .iter()
                .map(|(record, entry)| self.row(record, entry))
                .collect(),
            selected,
            recent_boards,
            removed,
        }
    }

    /// One row. Reads the repository's `HEAD`.
    fn row(&self, record: &ProjectRecord, entry: &ProjectEntry) -> ProjectRow {
        let (live, state) = self.row_state(record, entry);
        let root = lock(&entry.state).root.clone();
        ProjectRow {
            id: record.id.clone(),
            name: record.name.clone(),
            path: record.path.clone(),
            expanded: record.expanded,
            live,
            state,
            git: git::info(&root).map(|info| info.summary()),
        }
    }

    /// Whether the project is live, and its row state.
    fn row_state(&self, record: &ProjectRecord, entry: &ProjectEntry) -> (bool, ProjectState) {
        let (live, unavailable, root) = {
            let state = lock(&entry.state);
            (state.live.clone(), state.unavailable.clone(), state.root.clone())
        };
        if let Some(workspace) = live {
            return (true, workspace.project_state());
        }
        if let Some(reason) = unavailable {
            return (false, ProjectState::Unavailable { reason });
        }
        if !root.is_dir() {
            return (
                false,
                ProjectState::Unavailable {
                    reason: FOLDER_GONE.to_owned(),
                },
            );
        }
        let (last, at_ms) = match &record.last_known {
            Some(known) => (known.state.clone(), known.at_ms),
            None => {
                let compiled = Layout::discover(&root, &self.config.fallback_cache_dir)
                    .cache()
                    .is_dir();
                let last = if compiled {
                    ProjectState::Current
                } else {
                    ProjectState::NotCompiled
                };
                (last, record.added_at_ms)
            }
        };
        (
            false,
            ProjectState::Resting {
                last: Box::new(last),
                at_ms,
            },
        )
    }

    /// The row of project `id`.
    fn row_of(&self, id: &ProjectId) -> Result<ProjectRow> {
        let (record, entry) = {
            let inner = lock(&self.inner);
            let record = inner
                .file
                .record(id)
                .cloned()
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            let entry = inner
                .entries
                .get(id)
                .cloned()
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            (record, entry)
        };
        Ok(self.row(&record, &entry))
    }

    /// Adds the folder at absolute `path` as a resting, expanded row at the end, or finds the
    /// row that already has it (the same folder, or another worktree of its repository). Writes
    /// nothing in the folder.
    pub fn add(&self, path: &Path) -> Result<AddedProject> {
        let root = canonical_folder(path)?;
        let text = root.to_string_lossy().into_owned();
        let repository = git::find(&root).map(|repository| repository.id());
        let name = shared_name(&root);
        let (status, id) = {
            let mut inner = lock(&self.inner);
            let existing = inner.file.projects.iter().find(|record| {
                record.path == text || (repository.is_some() && record.repository == repository)
            });
            if let Some(existing) = existing {
                (AddStatus::Exists, existing.id.clone())
            } else {
                if inner.file.projects.len() >= MAX_PROJECTS {
                    return Err(WorkspaceError::TooManyProjects(MAX_PROJECTS));
                }
                let id = ProjectId::generate(|id| inner.entries.contains_key(id));
                inner.file.projects.push(ProjectRecord {
                    id: id.clone(),
                    path: text.clone(),
                    name,
                    expanded: true,
                    repository,
                    added_at_ms: now_ms(),
                    last_known: None,
                });
                inner.file.removed.retain(|removed| removed.path != text);
                inner
                    .entries
                    .insert(id.clone(), ProjectEntry::new(id.clone(), root));
                self.save(&inner)?;
                (AddStatus::Added, id)
            }
        };
        if status == AddStatus::Added {
            self.announce();
        }
        Ok(AddedProject {
            status,
            project: self.row_of(&id)?,
        })
    }

    /// Whether `path` is one of the removed folders "Opened before" lists.
    pub fn was_removed(&self, path: &str) -> bool {
        lock(&self.inner)
            .file
            .removed
            .iter()
            .any(|removed| removed.path == path)
    }

    /// Removes project `id` from the workspace: closes it if live, forgets its recent boards and
    /// selections, and lists its folder under "Opened before". Never touches its `.unfold/`.
    /// Refused with `boards_open` while a window holds one of its boards.
    pub fn remove(&self, id: &ProjectId) -> Result<()> {
        let entry = self.handle(id)?.entry;
        let _life = life(&entry);
        if !entry.claims.is_empty() {
            return Err(WorkspaceError::BoardsOpen);
        }
        let closing = {
            let mut inner = lock(&self.inner);
            let index = inner
                .file
                .projects
                .iter()
                .position(|record| record.id == *id)
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            let record = inner.file.projects.remove(index);
            inner.entries.remove(id);
            inner.selection.retain(|_, selected| selected != id);
            if inner.file.selected.as_ref() == Some(id) {
                inner.file.selected = None;
            }
            inner.file.recent_boards.retain(|recent| recent.project != *id);
            inner.file.removed.retain(|removed| removed.path != record.path);
            inner.file.removed.insert(
                0,
                RemovedRecord {
                    path: record.path,
                    name: record.name,
                    removed_at_ms: now_ms(),
                },
            );
            inner.file.removed.truncate(MAX_REMOVED);
            let mut state = lock(&entry.state);
            state.removed = true;
            state.resting = None;
            let closing = state.live.take();
            drop(state);
            self.save(&inner)?;
            closing
        };
        if let Some(workspace) = closing {
            workspace.close();
        }
        self.sink
            .emit(&WorkspaceEvent::ProjectReset { project: id.clone() });
        self.announce();
        Ok(())
    }

    /// Puts the rows in `order`, which must hold every project id once; `None` when it doesn't.
    pub fn reorder(&self, order: &[ProjectId]) -> Result<Option<WorkspaceView>> {
        {
            let mut inner = lock(&self.inner);
            let mut wanted = order.to_vec();
            wanted.sort();
            let mut have: Vec<ProjectId> = inner
                .file
                .projects
                .iter()
                .map(|record| record.id.clone())
                .collect();
            have.sort();
            if wanted != have {
                return Ok(None);
            }
            let mut records = std::mem::take(&mut inner.file.projects);
            for id in order {
                if let Some(index) = records.iter().position(|record| record.id == *id) {
                    inner.file.projects.push(records.swap_remove(index));
                }
            }
            self.save(&inner)?;
        }
        self.announce();
        Ok(Some(self.view()))
    }

    /// Expands or collapses project `id`'s sidebar row.
    pub fn set_expanded(&self, id: &ProjectId, expanded: bool) -> Result<()> {
        {
            let mut inner = lock(&self.inner);
            let record = inner
                .file
                .record_mut(id)
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            if record.expanded == expanded {
                return Ok(());
            }
            record.expanded = expanded;
            self.save(&inner)?;
        }
        self.announce();
        Ok(())
    }

    /// Sets `window`'s Home selection (interest for liveness, and the default for new windows).
    /// A project is woken before it is selected, so the window's next query finds it live.
    pub fn select(&self, window: WindowId, project: Option<&ProjectId>) -> Result<()> {
        if let Some(id) = project {
            self.handle(id)?.wake()?;
        }
        let previous = {
            let mut inner = lock(&self.inner);
            let previous = match project {
                Some(id) => {
                    if !inner.entries.contains_key(id) {
                        return Err(WorkspaceError::UnknownProject(id.clone()));
                    }
                    let previous = inner.selection.insert(window, id.clone());
                    if inner.file.selected.as_ref() != Some(id) {
                        inner.file.selected = Some(id.clone());
                        self.save(&inner)?;
                    }
                    previous
                }
                None => inner.selection.remove(&window),
            };
            previous
                .filter(|previous| Some(previous) != project)
                .and_then(|previous| inner.entries.get(&previous).cloned())
        };
        if let Some(entry) = previous {
            self.extend_lease(&entry);
        }
        Ok(())
    }

    /// Re-points project `id` at another worktree of its repository: `path` must be one of the
    /// available worktree paths `git_info` lists. Refused with `boards_open` while a window holds
    /// one of its boards. The project's graph, boards, drafts, and `state.json` become the new
    /// folder's; it reopens there when it has interest.
    pub fn switch_worktree(&self, id: &ProjectId, path: &str) -> Result<ProjectRow> {
        let handle = self.handle(id)?;
        let available = git::info(&handle.root).is_some_and(|info| {
            info.worktrees
                .iter()
                .any(|worktree| worktree.available && worktree.path == path)
        });
        if !available {
            return Err(WorkspaceError::NotAWorktree(path.to_owned()));
        }
        let root = canonical_folder(Path::new(path))?;
        let entry = handle.entry;
        let _life = life(&entry);
        if !entry.claims.is_empty() {
            return Err(WorkspaceError::BoardsOpen);
        }
        if lock(&entry.state).root == root {
            return self.row_of(id);
        }
        let name = shared_name(&root);
        let repository = git::find(&root).map(|repository| repository.id());
        let (closing, interested) = {
            let mut inner = lock(&self.inner);
            let record = inner
                .file
                .record_mut(id)
                .ok_or_else(|| WorkspaceError::UnknownProject(id.clone()))?;
            record.path = root.to_string_lossy().into_owned();
            record.name = name;
            record.repository = repository;
            record.last_known = None;
            let mut state = lock(&entry.state);
            let closing = state.live.take();
            state.root = root;
            state.resting = None;
            state.unavailable = None;
            state.extras = Arc::default();
            let interested = self.interested(&inner, &entry, &state);
            drop(state);
            self.save(&inner)?;
            (closing, interested)
        };
        if let Some(workspace) = closing {
            workspace.close();
        }
        // A failed reopen leaves the row unavailable, which the row says; the switch stands.
        if interested {
            let _ = self.open_locked(&entry);
        }
        self.sink
            .emit(&WorkspaceEvent::ProjectReset { project: id.clone() });
        self.announce();
        let row = self.row_of(id)?;
        self.sink.emit(&WorkspaceEvent::ProjectState {
            project: id.clone(),
            state: row.state.clone(),
        });
        Ok(row)
    }

    /// Reads project `id`'s shared name from its folder again; when it changed, saves it and
    /// sends `workspace_changed`.
    pub(crate) fn refresh_name(&self, id: &ProjectId) {
        let Ok(handle) = self.handle(id) else {
            return;
        };
        let name = shared_name(&handle.root);
        let changed = {
            let mut inner = lock(&self.inner);
            match inner.file.record_mut(id) {
                Some(record) if record.name != name => {
                    record.name = name;
                    // The name is a cache; a failed write is rewritten with the next change.
                    let _ = self.save(&inner);
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.announce();
        }
    }

    /// Changes the recent boards; saves and announces them when `change` says they changed.
    fn update_recents(&self, change: impl FnOnce(&mut Vec<RecentBoard>) -> bool) -> Result<()> {
        {
            let mut inner = lock(&self.inner);
            if !change(&mut inner.file.recent_boards) {
                return Ok(());
            }
            inner.file.recent_boards.truncate(MAX_RECENT_BOARDS);
            self.save(&inner)?;
        }
        self.announce();
        Ok(())
    }

    /// Records that board `board` of project `project`, titled `title`, was opened or created
    /// (most recent first; at most 20 are kept).
    pub fn record_recent_board(&self, project: &ProjectId, board: &str, title: &str) -> Result<()> {
        let recent = RecentBoard {
            project: project.clone(),
            board: board.to_owned(),
            title: title.to_owned(),
            opened_at_ms: now_ms(),
        };
        self.update_recents(|recents| {
            recents.retain(|existing| !(existing.project == *project && existing.board == board));
            recents.insert(0, recent);
            true
        })
    }

    /// Board `from` of `project` has the id `to` now: its recent entry follows.
    pub fn move_recent_board(&self, project: &ProjectId, from: &str, to: &str) -> Result<()> {
        let is = |recent: &RecentBoard, board: &str| recent.project == *project && recent.board == board;
        self.update_recents(|recents| {
            if from == to || !recents.iter().any(|recent| is(recent, from)) {
                return false;
            }
            recents.retain(|recent| !is(recent, to));
            for recent in recents.iter_mut().filter(|recent| is(recent, from)) {
                to.clone_into(&mut recent.board);
            }
            true
        })
    }

    /// Board `board` of `project` is gone: it leaves the recent boards.
    pub fn forget_recent_board(&self, project: &ProjectId, board: &str) -> Result<()> {
        self.update_recents(|recents| {
            let before = recents.len();
            recents.retain(|recent| !(recent.project == *project && recent.board == board));
            recents.len() != before
        })
    }

    /// Whether anything keeps project `entry` live. Call with `inner` and its `state` locked.
    fn interested(&self, inner: &Inner, entry: &ProjectEntry, state: &EntryState) -> bool {
        state.pins > 0
            || !entry.claims.is_empty()
            || inner.selection.values().any(|selected| *selected == entry.id)
            || inner
                .entries
                .values()
                .any(|other| other.claims.references(&entry.id))
    }

    /// Lets project `entry` stay live at least `rest_after` from now.
    fn extend_lease(&self, entry: &ProjectEntry) {
        let mut state = lock(&entry.state);
        state.lease_until = state.lease_until.max(Instant::now() + self.options.rest_after);
    }

    /// The live workspace of `entry`, extending its lease; `None` when it rests.
    fn live_now(&self, entry: &ProjectEntry) -> Result<Option<Arc<Workspace>>> {
        let mut state = lock(&entry.state);
        if state.removed {
            return Err(WorkspaceError::UnknownProject(entry.id.clone()));
        }
        let live = state.live.clone();
        if live.is_some() {
            state.lease_until = state.lease_until.max(Instant::now() + self.options.rest_after);
        }
        Ok(live)
    }

    /// Wakes `entry` (see the [module documentation](self)).
    fn wake(&self, entry: &Arc<ProjectEntry>) -> Result<Arc<Workspace>> {
        if let Some(live) = self.live_now(entry)? {
            return Ok(live);
        }
        let _life = life(entry);
        self.open_locked(entry)
    }

    /// Opens `entry`'s workspace unless it is open; call with its `life` held.
    fn open_locked(&self, entry: &Arc<ProjectEntry>) -> Result<Arc<Workspace>> {
        if let Some(live) = self.live_now(entry)? {
            return Ok(live);
        }
        if self.shut.load(Ordering::SeqCst) {
            return Err(WorkspaceError::Closed);
        }
        let root = lock(&entry.state).root.clone();
        match Workspace::open(&root, self.context(entry.id.clone())) {
            Ok(workspace) => {
                {
                    let mut state = lock(&entry.state);
                    state.live = Some(Arc::clone(&workspace));
                    state.resting = None;
                    state.unavailable = None;
                    state.extras = Arc::default();
                    state.lease_until = state.lease_until.max(Instant::now() + self.options.rest_after);
                }
                self.refresh_name(&entry.id);
                (self.on_wake)(&entry.id);
                Ok(workspace)
            }
            Err(error) => {
                let reason = unavailable_reason(&error);
                lock(&entry.state).unavailable = Some(reason.clone());
                self.sink.emit(&WorkspaceEvent::ProjectState {
                    project: entry.id.clone(),
                    state: ProjectState::Unavailable { reason },
                });
                Err(error)
            }
        }
    }

    /// What opening project `project`'s folder needs: the preferences' background default and
    /// thread limit apply to every project opened after they change.
    fn context(&self, project: ProjectId) -> OpenContext {
        let preferences = self.app.preferences();
        let mut options = self.options.clone();
        options.background_updates_default = preferences.background_updates;
        if let Some(limit) = preferences.thread_limit.filter(|limit| *limit > 0) {
            options.engine.compile_threads = limit as usize;
        }
        OpenContext {
            project,
            fallback_cache_dir: self.config.fallback_cache_dir.clone(),
            options,
            sink: Arc::clone(&self.sink),
            languages: default_languages(),
        }
    }

    /// Releases every board `window` holds and its Home selection (the window closed). Calls
    /// `released` with each project whose boards it held; projects losing interest start their
    /// lease.
    pub(crate) fn detach_window(
        &self,
        window: WindowId,
        mut released: impl FnMut(&ProjectHandle<'_>, &[String]),
    ) {
        let (previous, entries) = {
            let mut inner = lock(&self.inner);
            let previous = inner.selection.remove(&window);
            (previous, inner.entries.values().cloned().collect::<Vec<_>>())
        };
        for entry in entries {
            let boards = entry.claims.release_window(window);
            if !boards.is_empty()
                && let Ok(handle) = self.handle(&entry.id)
            {
                released(&handle, &boards);
            }
            if !boards.is_empty() || previous.as_ref() == Some(&entry.id) {
                self.extend_lease(&entry);
            }
        }
    }

    /// The reaper's tick: rests every project that may rest, wakes every resting project that
    /// gained interest (an open board with cards from it), and closes resting read pools unused
    /// for a minute.
    pub(crate) fn reap(&self) {
        let entries: Vec<Arc<ProjectEntry>> = lock(&self.inner).entries.values().cloned().collect();
        for entry in entries {
            self.tend(&entry);
        }
    }

    /// Rests `entry` if nothing keeps it live, or wakes it if something does while it rests;
    /// never waits for a wake or a rest in progress.
    fn tend(&self, entry: &Arc<ProjectEntry>) {
        let _life = match entry.life.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        let now = Instant::now();
        let resting = {
            let mut inner = lock(&self.inner);
            let mut state = lock(&entry.state);
            if state
                .resting
                .as_ref()
                .is_some_and(|resting| resting.used.elapsed() >= RESTING_POOL_IDLE)
            {
                state.resting = None;
            }
            let interested = self.interested(&inner, entry, &state);
            let Some(workspace) = state.live.clone() else {
                // A project that can't be opened stays resting until a command tries again.
                let wake = interested && !state.removed && state.unavailable.is_none();
                drop(state);
                drop(inner);
                if wake {
                    // A failure marks the row unavailable, which the row says.
                    let _ = self.open_locked(entry);
                }
                return;
            };
            if interested {
                state.interested = true;
                return;
            }
            if std::mem::take(&mut state.interested) {
                state.lease_until = state.lease_until.max(now + self.options.rest_after);
            }
            if now < state.lease_until || !workspace.is_idle() {
                return;
            }
            state.live = None;
            state.extras = Arc::default();
            drop(state);
            let last = workspace.settled_state();
            let at_ms = now_ms();
            if let Some(record) = inner.file.record_mut(&entry.id) {
                record.last_known = Some(LastKnown {
                    state: last.clone(),
                    at_ms,
                });
            }
            // A failed write only loses the remembered state shown while resting.
            let _ = self.save(&inner);
            (workspace, last, at_ms)
        };
        let (workspace, last, at_ms) = resting;
        workspace.close();
        self.sink.emit(&WorkspaceEvent::ProjectState {
            project: entry.id.clone(),
            state: ProjectState::Resting {
                last: Box::new(last),
                at_ms,
            },
        });
    }

    /// Closes every live project, remembering each one's state (the app is quitting). Nothing
    /// wakes afterwards.
    pub(crate) fn shutdown(&self) {
        self.shut.store(true, Ordering::SeqCst);
        let entries: Vec<Arc<ProjectEntry>> = lock(&self.inner).entries.values().cloned().collect();
        for entry in entries {
            let _life = life(&entry);
            let closing = {
                let mut inner = lock(&self.inner);
                let mut state = lock(&entry.state);
                state.resting = None;
                let Some(workspace) = state.live.take() else {
                    continue;
                };
                drop(state);
                if let Some(record) = inner.file.record_mut(&entry.id) {
                    record.last_known = Some(LastKnown {
                        state: workspace.settled_state(),
                        at_ms: now_ms(),
                    });
                }
                let _ = self.save(&inner);
                workspace
            };
            closing.close();
        }
    }
}

/// One project, as a command sees it: its id, folder, and shared name when it was looked up, and
/// doors to its engine (which wake it) and its stored graph (which don't).
pub struct ProjectHandle<'a> {
    projects: &'a Projects,
    entry: Arc<ProjectEntry>,
    root: PathBuf,
    name: String,
}

impl std::fmt::Debug for ProjectHandle<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectHandle")
            .field("id", &self.entry.id)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl ProjectHandle<'_> {
    /// The project's id.
    pub fn id(&self) -> &ProjectId {
        &self.entry.id
    }

    /// The canonical folder the project points at.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project's shared name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The live workspace, or `None` while the project rests. Never wakes it.
    pub fn live(&self) -> Option<Arc<Workspace>> {
        lock(&self.entry.state).live.clone()
    }

    /// The live workspace, waking the project if it rests, and extending its lease. Fails like
    /// opening the folder (`not_a_folder`, `open_elsewhere`), and the row says so.
    pub fn wake(&self) -> Result<Arc<Workspace>> {
        self.projects.wake(&self.entry)
    }

    /// The stored graph, without waking the project: the live workspace's reader pool, or a
    /// resting project's read-only pool (opened on first use). `None` when there is no usable
    /// graph (never compiled here, or built by another engine version).
    pub fn readers(&self) -> Result<Option<ReadPool>> {
        if let Some(workspace) = self.live() {
            return Ok(workspace.readers().ok());
        }
        if let Some(resting) = lock(&self.entry.state).resting.as_mut() {
            resting.used = Instant::now();
            return Ok(resting.pool.clone());
        }
        let layout = self.layout();
        let pool = if layout.is_compiled() {
            ReadPool::open_existing(&layout.database(), &self.projects.version)?
        } else {
            None
        };
        let mut state = lock(&self.entry.state);
        if let Some(workspace) = &state.live {
            return Ok(workspace.readers().ok());
        }
        state.resting = Some(RestingReader {
            pool: pool.clone(),
            used: Instant::now(),
        });
        Ok(pool)
    }

    /// The project's `.unfold/` layout (where `local/` is), without writing anything.
    pub fn layout(&self) -> Layout {
        match self.live() {
            Some(workspace) => workspace.layout(),
            None => Layout::discover(&self.root, &self.projects.config.fallback_cache_dir),
        }
    }

    /// Checks a path a command named against the project's folder (docs/architecture.md#security).
    pub fn confine(&self, path: &str) -> Result<ConfinedPath> {
        confine(&self.root, path)
    }

    /// Which window holds each open board of the project.
    pub fn claims(&self) -> &BoardClaims {
        &self.entry.claims
    }

    /// The project's command-side state (dropped when it rests or switches worktree).
    pub fn extras(&self) -> Arc<ProjectExtras> {
        Arc::clone(&lock(&self.entry.state).extras)
    }

    /// The project's row state now.
    pub fn state(&self) -> ProjectState {
        let record = lock(&self.projects.inner).file.record(self.id()).cloned();
        match record {
            Some(record) => self.projects.row_state(&record, &self.entry).1,
            None => ProjectState::Unavailable {
                reason: "The project was removed.".to_owned(),
            },
        }
    }

    /// Keeps the project live until the returned pin is dropped (a running compile).
    pub fn pin(&self) -> CompilePin<'_> {
        lock(&self.entry.state).pins += 1;
        CompilePin {
            projects: self.projects,
            entry: Arc::clone(&self.entry),
        }
    }
}

/// Keeps a project live while a compile of it runs (see [`ProjectHandle::pin`]).
pub struct CompilePin<'a> {
    projects: &'a Projects,
    entry: Arc<ProjectEntry>,
}

impl Drop for CompilePin<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.entry.state);
        state.pins = state.pins.saturating_sub(1);
        state.lease_until = state
            .lease_until
            .max(Instant::now() + self.projects.options.rest_after);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Silent;

    impl EventSink for Silent {
        fn emit(&self, _event: &WorkspaceEvent) {}
    }

    fn projects(dir: &Path) -> Projects {
        let config = AppConfig {
            app_data_dir: dir.join("app"),
            logs_dir: dir.join("logs"),
            fallback_cache_dir: dir.join("fallback"),
        };
        Projects::new(
            config,
            Arc::new(Silent),
            WorkspaceOptions::default(),
            Box::new(|_| {}),
        )
    }

    #[test]
    fn ids_are_random_base32_and_unique() {
        let first = ProjectId::generate(|_| false);
        assert_eq!(first.as_str().len(), 12);
        assert!(first.as_str().starts_with("p-"));
        assert!(
            first.as_str()[2..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
        );
        let second = ProjectId::generate(|id| *id == first);
        assert_ne!(first, second);
    }

    #[test]
    fn recent_boards_follow_their_boards() {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical");
        std::fs::create_dir_all(base.join("repo")).expect("mkdir");
        let projects = projects(&base);
        let added = projects.add(&base.join("repo")).expect("add");
        let id = added.project.id;
        projects
            .record_recent_board(&id, ".unfold/local/drafts/b.json", "B")
            .expect("record");
        projects
            .record_recent_board(&id, ".unfold/boards/a.json", "A")
            .expect("record");
        projects
            .move_recent_board(&id, ".unfold/local/drafts/b.json", ".unfold/boards/b.json")
            .expect("move");
        let boards: Vec<String> = projects
            .view()
            .recent_boards
            .into_iter()
            .map(|recent| recent.board)
            .collect();
        assert_eq!(boards, [".unfold/boards/a.json", ".unfold/boards/b.json"]);
        projects
            .forget_recent_board(&id, ".unfold/boards/a.json")
            .expect("forget");
        assert_eq!(projects.view().recent_boards.len(), 1);
        for index in 0..MAX_RECENT_BOARDS + 4 {
            projects
                .record_recent_board(&id, &format!(".unfold/boards/{index}.json"), "x")
                .expect("record");
        }
        assert_eq!(projects.view().recent_boards.len(), MAX_RECENT_BOARDS);
        assert_eq!(projects.resolve_name("repo"), Some(id));
        assert_eq!(projects.resolve_name("nope"), None);
    }

    #[test]
    fn a_damaged_workspace_file_reads_as_an_empty_workspace() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("app")).expect("mkdir");
        std::fs::write(dir.path().join("app/workspace.json"), "{ nope").expect("write");
        assert!(projects(dir.path()).view().projects.is_empty());
    }
}
