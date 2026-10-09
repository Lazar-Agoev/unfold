//! A `Host` over a copy of a playground in a temporary folder, a sink that records every
//! message, and helpers to call commands the way the addon does.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
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

    /// Waits for a message that `matches`.
    pub fn wait_for(&self, matches: impl Fn(&Value) -> bool) -> Option<Value> {
        let deadline = Instant::now() + PATIENCE;
        let mut messages = self.messages.lock().expect("messages");
        loop {
            if let Some(found) = messages.iter().find(|message| matches(message)) {
                return Some(found.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            messages = self
                .arrived
                .wait_timeout(messages, deadline - now)
                .expect("wait")
                .0;
        }
    }
}

/// A host over a playground copy.
pub struct Harness {
    _dir: TempDir,
    pub root: PathBuf,
    pub host: Host,
    pub sink: Arc<Recorder>,
    next: AtomicU64,
}

/// Test options: no global Git excludes, two compile threads, and watching as asked.
pub fn options(watch: bool) -> WorkspaceOptions {
    let mut options = WorkspaceOptions {
        debounce: Duration::from_millis(100),
        watch,
        ..WorkspaceOptions::default()
    };
    options.engine.discover.global_ignore = false;
    options.engine.compile_threads = 2;
    options
}

impl Harness {
    /// A host over a copy of `testdata/playgrounds/<name>` (with its `.unfold/config.toml`).
    pub fn new(name: &str, watch: bool) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = std::fs::canonicalize(dir.path()).expect("canonical");
        let root = base.join(name);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/playgrounds")
                .join(name),
            &root,
        );
        let host = Host::with_options(options(watch));
        let init = serde_json::json!({
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
            root,
            host,
            sink,
            next: AtomicU64::new(1),
        }
    }

    /// A fresh request ID.
    pub fn request(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Attaches `window` to the playground.
    pub fn open(&self, window: u32) -> Value {
        let text = self
            .host
            .open_workspace(window, self.root.to_str().expect("utf-8 path"))
            .expect("open");
        serde_json::from_str(&text).expect("json")
    }

    /// Runs a command for `window`.
    pub fn call(&self, window: u32, command: &str, args: Value) -> Result<Value, CommandError> {
        self.call_with(window, serde_json::json!({ "command": command, "args": args }))
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

    /// Opens window 1 and compiles.
    pub fn compiled(name: &str) -> Self {
        let harness = Self::new(name, false);
        harness.open(1);
        harness.ok(1, "compile", serde_json::json!({}));
        harness
    }

    pub fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.host.shutdown();
    }
}

fn copy_dir(from: &Path, to: &Path) {
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
