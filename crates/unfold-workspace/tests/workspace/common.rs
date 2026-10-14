//! Shared helpers: playground copies in temporary folders, a recording event sink, and
//! workspace options tuned for tests (short quiet period, no global Git excludes).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tempfile::TempDir;
use unfold_core::highlight::Highlighter;
use unfold_core::vocab::InputRole;
use unfold_core::{RepoPath, Snapshot};
use unfold_engine::settings::{Settings, SettingsHash};
use unfold_engine::{
    ExtractionOptions, FactsSet, FileFacts, InputProblem, Language, LanguageId, Linked, Producer,
    ProjectInputs, SourceFailure,
};
use unfold_workspace::{
    EventSink, OpenContext, ProjectId, ProjectState, Workspace, WorkspaceEvent, WorkspaceOptions,
    default_languages,
};

/// The longest any test waits for something asynchronous. Generous on purpose: waits end as
/// soon as the condition holds, so this only bounds a failing test.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// A copy of `testdata/playgrounds/<name>` in a temporary folder. `keep_unfold` keeps the
/// playground's `unfold.toml`.
pub struct Playground {
    _dir: TempDir,
    pub root: PathBuf,
}

impl Playground {
    pub fn new(name: &str, keep_unfold: bool) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical temp dir");
        let root = base.join(name);
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/playgrounds")
            .join(name);
        copy_dir(&source, &root);
        if !keep_unfold {
            std::fs::remove_file(root.join("unfold.toml")).expect("remove unfold.toml");
        }
        Self { _dir: dir, root }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn write(&self, relative: &str, text: &str) {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, text).expect("write");
    }

    pub fn context(&self, sink: Arc<RecordingSink>, watch: bool) -> OpenContext {
        OpenContext {
            project: project(),
            options: options(watch),
            sink,
            languages: default_languages(),
        }
    }

    /// Opens the playground with watching on or off.
    pub fn open(&self, sink: &Arc<RecordingSink>, watch: bool) -> Arc<Workspace> {
        Workspace::open(&self.root, self.context(Arc::clone(sink), watch)).expect("open")
    }

    /// Opens the playground unwatched, with languages that add every file they extract to
    /// `extracted`.
    pub fn open_counted(&self, sink: &Arc<RecordingSink>, extracted: &Arc<AtomicUsize>) -> Arc<Workspace> {
        let mut context = self.context(Arc::clone(sink), false);
        context.languages = context
            .languages
            .into_iter()
            .map(|inner| {
                Arc::new(Counted {
                    inner,
                    extracted: Arc::clone(extracted),
                }) as Arc<dyn Language>
            })
            .collect();
        Workspace::open(&self.root, context).expect("open")
    }
}

/// A language that counts the files it extracts: the ones the facts cache didn't answer.
struct Counted {
    inner: Arc<dyn Language>,
    extracted: Arc<AtomicUsize>,
}

impl Language for Counted {
    fn id(&self) -> LanguageId {
        self.inner.id()
    }

    fn producer(&self) -> Producer {
        self.inner.producer()
    }

    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool {
        self.inner.owns(path, settings)
    }

    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash {
        self.inner.settings_hash(path, settings)
    }

    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure> {
        self.extracted.fetch_add(1, Ordering::Relaxed);
        self.inner.extract(file, settings)
    }

    fn reads_payload(&self, payload: &[u8]) -> bool {
        self.inner.reads_payload(payload)
    }

    fn extraction_options(
        &self,
        paths: &[&RepoPath],
        settings: &Settings,
        inputs: &ProjectInputs,
    ) -> Vec<ExtractionOptions> {
        self.inner.extraction_options(paths, settings, inputs)
    }

    fn extract_with(
        &self,
        file: &Snapshot,
        settings: &Settings,
        options: &ExtractionOptions,
    ) -> Result<FileFacts, SourceFailure> {
        self.extracted.fetch_add(1, Ordering::Relaxed);
        self.inner.extract_with(file, settings, options)
    }

    fn input_problems(&self, inputs: &ProjectInputs) -> Vec<InputProblem> {
        self.inner.input_problems(inputs)
    }

    fn requested_inputs(&self, inputs: &ProjectInputs) -> Vec<(RepoPath, InputRole)> {
        self.inner.requested_inputs(inputs)
    }

    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a> {
        self.inner.link(files, inputs)
    }

    fn highlighter(&self) -> &dyn Highlighter {
        self.inner.highlighter()
    }
}

/// The project id the tests open playgrounds for.
pub fn project() -> ProjectId {
    serde_json::from_value(serde_json::json!("p-testtesttt")).expect("a project id")
}

pub fn options(watch: bool) -> WorkspaceOptions {
    let mut options = WorkspaceOptions {
        debounce: Duration::from_millis(150),
        watch,
        scan_interval: Duration::from_millis(500),
        ..WorkspaceOptions::default()
    };
    options.engine.discover.global_ignore = false;
    options.engine.compile_threads = 2;
    options
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// Records every event, and lets tests wait for one.
#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<WorkspaceEvent>>,
    arrived: Condvar,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: &WorkspaceEvent) {
        self.events.lock().expect("events").push(event.clone());
        self.arrived.notify_all();
    }
}

impl RecordingSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// How many events so far: pass it to [`RecordingSink::wait_for`] to wait for a later one.
    pub fn mark(&self) -> usize {
        self.events.lock().expect("events").len()
    }

    /// Waits for an event after `after` that `matches`, and returns it.
    pub fn wait_for(
        &self,
        after: usize,
        matches: impl Fn(&WorkspaceEvent) -> bool,
    ) -> Option<WorkspaceEvent> {
        let deadline = Instant::now() + PATIENCE;
        let mut events = self.events.lock().expect("events");
        loop {
            if let Some(found) = events.iter().skip(after).find(|event| matches(event)) {
                return Some(found.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            events = self.arrived.wait_timeout(events, deadline - now).expect("wait").0;
        }
    }

    /// Every event after `after` that `matches`.
    pub fn events_after(
        &self,
        after: usize,
        matches: impl Fn(&WorkspaceEvent) -> bool,
    ) -> Vec<WorkspaceEvent> {
        self.events
            .lock()
            .expect("events")
            .iter()
            .skip(after)
            .filter(|event| matches(event))
            .cloned()
            .collect()
    }

    /// Every event of `kind` after `after`.
    pub fn of_kind(&self, after: usize, kind: &str) -> Vec<WorkspaceEvent> {
        self.events
            .lock()
            .expect("events")
            .iter()
            .skip(after)
            .filter(|event| event.kind() == kind)
            .cloned()
            .collect()
    }
}

/// Whether `event` is a `graph_updated` naming `path` as changed.
pub fn updates(event: &WorkspaceEvent, path: &str) -> bool {
    matches!(event, WorkspaceEvent::GraphUpdated { changed_files, .. } if changed_files.iter().any(|file| file == path))
}

/// Compiles and waits for the result.
pub fn compile(workspace: &Workspace) -> unfold_workspace::CompileOutcome {
    workspace.compile(None).wait().expect("compile")
}

/// Waits for the workspace to go idle, failing the test if it doesn't.
pub fn settle(workspace: &Workspace) {
    assert!(workspace.wait_idle(PATIENCE), "the workspace never went idle");
}

/// Waits until the watcher has reported everything that happened in the folder so far, and the
/// work it queued is done. Copying a playground can still send events after the watcher starts;
/// events arrive in order, so once a board file written now is reported, those are too. (A board
/// file outside the app queues no work, only `boards_changed`.)
pub fn drain_watcher(playground: &Playground, workspace: &Workspace, sink: &RecordingSink) {
    const BARRIER: &str = ".unfold/boards/watcher-barrier.json";
    let mark = sink.mark();
    playground.write(BARRIER, &mark.to_string());
    let reported = sink.wait_for(mark, |event| {
        matches!(event, WorkspaceEvent::BoardsChanged { boards, .. } if boards.iter().any(|board| board == BARRIER))
    });
    assert!(reported.is_some(), "the watcher never reported {BARRIER}");
    settle(workspace);
}

/// Whether `event` says a background update or a Compile started.
pub fn starts_work(event: &WorkspaceEvent) -> bool {
    matches!(
        event,
        WorkspaceEvent::ProjectState {
            state: ProjectState::Updating { .. } | ProjectState::Compiling { .. },
            ..
        } | WorkspaceEvent::CompileProgress { .. }
    )
}

/// When the stored graph was last committed.
pub fn committed_at(workspace: &Workspace) -> i64 {
    let reader = workspace.readers().expect("readers").get().expect("reader");
    reader.meta().expect("meta").graph.expect("graph").compiled_at_ms
}

/// Whether the stored graph has a block named `name` in `path`.
pub fn has_block(workspace: &Workspace, path: &str, name: &str) -> bool {
    let reader = workspace.readers().expect("readers").get().expect("reader");
    reader
        .file_graph(path)
        .expect("file graph")
        .is_some_and(|file| file.nodes.iter().any(|node| node.name.as_deref() == Some(name)))
}
