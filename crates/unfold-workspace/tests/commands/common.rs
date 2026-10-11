//! A `Host` over copies of the playgrounds in a temporary folder, added as projects through the
//! trusted `add_project`, a sink that records every message, and helpers to call commands the
//! way the addon does.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;
use unfold_workspace::{CommandError, Host, WorkspaceOptions};

/// The longest a test waits for something asynchronous.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// Records sink messages and lets tests wait for one.
#[derive(Default)]
pub struct Recorder {
    messages: Mutex<Vec<Value>>,
    arrived: Condvar,
}

impl Recorder {
    fn push(&self, text: &str) {
        let value: Value = serde_json::from_str(text).expect("sink messages are JSON");
        self.messages.lock().expect("messages").push(value);
        self.arrived.notify_all();
    }

    /// Every message so far.
    pub fn all(&self) -> Vec<Value> {
        self.messages.lock().expect("messages").clone()
    }

    /// How many messages so far: pass it to [`Recorder::events`] to see only later ones.
    pub fn mark(&self) -> usize {
        self.messages.lock().expect("messages").len()
    }

    /// The events of `kind` after the first `after` messages.
    pub fn events(&self, after: usize, kind: &str) -> Vec<Value> {
        self.all()
            .into_iter()
            .skip(after)
            .filter(|message| message["event"]["type"] == kind)
            .map(|message| message["event"].clone())
            .collect()
    }
}

/// Whether `message` is a broadcast event of `kind` about `project`.
pub fn is_event(message: &Value, kind: &str, project: &str) -> bool {
    message["kind"] == "broadcast"
        && message["event"]["type"] == kind
        && message["event"]["project"] == project
}

/// A host over playground copies. `project` and `root` are the first project's.
pub struct Harness {
    _dir: TempDir,
    pub base: PathBuf,
    pub host: Host,
    pub sink: Arc<Recorder>,
    next: AtomicU64,
    pub project: String,
    pub root: PathBuf,
}

/// Test options: no global Git excludes, two compile threads, watching as asked, and liveness
/// timings that keep a project live for the length of a test unless it asks otherwise.
pub fn options(watch: bool) -> WorkspaceOptions {
    let mut options = WorkspaceOptions {
        debounce: Duration::from_millis(100),
        watch,
        rest_after: Duration::from_secs(120),
        reap_interval: Duration::from_millis(50),
        ..WorkspaceOptions::default()
    };
    options.engine.discover.global_ignore = false;
    options.engine.compile_threads = 2;
    options
}

/// [`options`] that let an unused project rest almost at once.
pub fn restless() -> WorkspaceOptions {
    WorkspaceOptions {
        rest_after: Duration::from_millis(150),
        reap_interval: Duration::from_millis(25),
        ..options(false)
    }
}

/// The playground `name` in the repository.
pub fn playground(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/playgrounds")
        .join(name)
}

/// The fixture `name` in the repository (`testdata/fixtures/<name>`).
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures")
        .join(name)
}

impl Harness {
    /// A host with no projects yet.
    pub fn empty(options: WorkspaceOptions) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical");
        let host = Host::with_options(options);
        let init = json!({
            "appDataDir": base.join("app"),
            "logsDir": base.join("logs"),
            "fallbackCacheDir": base.join("fallback"),
        });
        host.init(&init.to_string()).expect("init");
        let sink = Arc::new(Recorder::default());
        let recorder = Arc::clone(&sink);
        host.subscribe(move |message| recorder.push(&message));
        Self {
            _dir: dir,
            base,
            host,
            sink,
            next: AtomicU64::new(1),
            project: String::new(),
            root: PathBuf::new(),
        }
    }

    /// A host over a copy of `testdata/playgrounds/<name>` (with its `.unfold/config.toml`),
    /// added as a project (resting, not compiled).
    pub fn new(name: &str, watch: bool) -> Self {
        Self::with(name, options(watch))
    }

    /// [`Harness::new`] with `options`.
    pub fn with(name: &str, options: WorkspaceOptions) -> Self {
        let mut harness = Self::empty(options);
        let (project, root) = harness.add(name, name);
        harness.project = project;
        harness.root = root;
        harness
    }

    /// A host over a playground copy, selected in window 1 (so it stays live) and compiled.
    pub fn compiled(name: &str) -> Self {
        let harness = Self::new(name, false);
        harness.ok(1, "select_project", json!({ "project": harness.project }));
        harness.ok(1, "compile", json!({ "project": harness.project }));
        harness
    }

    /// A host over a copy of `testdata/fixtures/<name>`, selected in window 1 and compiled.
    pub fn compiled_fixture(name: &str) -> Self {
        let mut harness = Self::empty(options(false));
        let (project, root) = harness.add_copy(&fixture(name), name);
        harness.project = project;
        harness.root = root;
        harness.ok(1, "select_project", json!({ "project": harness.project }));
        harness.ok(1, "compile", json!({ "project": harness.project }));
        harness
    }

    /// Copies playground `name` to `<base>/<folder>` and adds it as a project; its id and folder.
    pub fn add(&self, name: &str, folder: &str) -> (String, PathBuf) {
        self.add_copy(&playground(name), folder)
    }

    /// Copies the folder `source` to `<base>/<folder>` and adds it as a project; its id and folder.
    pub fn add_copy(&self, source: &Path, folder: &str) -> (String, PathBuf) {
        let root = self.base.join(folder);
        copy_dir(source, &root);
        let added = self
            .trusted("add_project", json!({ "path": root }))
            .expect("add_project");
        assert_eq!(added["status"], "added", "{added}");
        let id = added["project"]["id"].as_str().expect("id").to_owned();
        (id, root)
    }

    /// A fresh request ID.
    pub fn request(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Runs a command for `window`.
    pub fn call(&self, window: u32, command: &str, args: Value) -> Result<Value, CommandError> {
        self.call_with(window, json!({ "command": command, "args": args }))
    }

    /// Runs a command as the main process sends it (`trusted`, window 0).
    pub fn trusted(&self, command: &str, args: Value) -> Result<Value, CommandError> {
        self.call_with(0, json!({ "command": command, "args": args, "trusted": true }))
    }

    /// Runs a raw command JSON for `window`.
    pub fn call_with(&self, window: u32, command: Value) -> Result<Value, CommandError> {
        let text = self.host.dispatch(window, self.request(), &command.to_string())?;
        Ok(serde_json::from_str(&text).expect("results are JSON"))
    }

    /// Runs a command that must succeed.
    pub fn ok(&self, window: u32, command: &str, args: Value) -> Value {
        self.call(window, command, args)
            .unwrap_or_else(|error| panic!("{command} failed: {error}"))
    }

    /// Runs a command that must fail, and returns its error code.
    pub fn code(&self, window: u32, command: &str, args: Value) -> String {
        match self.call(window, command, args) {
            Ok(value) => panic!("{command} succeeded: {value}"),
            Err(error) => error.code,
        }
    }

    /// A project's row in `workspace`.
    pub fn row(&self, project: &str) -> Value {
        let workspace = self.ok(1, "workspace", json!({}));
        workspace["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .find(|row| row["id"] == project)
            .cloned()
            .unwrap_or_else(|| panic!("no row {project} in {workspace}"))
    }

    /// Writes a file in the first project.
    pub fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.host.shutdown();
    }
}

/// Copies a folder recursively.
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

/// Waits until `check` holds, or fails the test after [`PATIENCE`].
pub fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
