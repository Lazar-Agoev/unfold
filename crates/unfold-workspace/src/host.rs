//! [`Host`]: the facade the napi-rs addon wraps, one per engine-host process (brief §6,
//! architecture §12).
//!
//! | Addon function | Host |
//! | --- | --- |
//! | `init(options)` | [`Host::init`] with the JSON `{"appDataDir","logsDir","fallbackCacheDir"}` |
//! | `subscribe(sink)` | [`Host::subscribe`]: every event and stream message, as one JSON string each |
//! | `openWorkspace(windowId, root)` | [`Host::open_workspace_async`] (or the blocking [`Host::open_workspace`]) |
//! | `closeWorkspace(windowId)` | [`Host::close_workspace_async`] (or [`Host::close_workspace`]) |
//! | `dispatch(windowId, requestId, command)` | [`Host::dispatch_async`] (or [`Host::dispatch`]) |
//! | `cancel(requestId)` | [`Host::cancel`] |
//!
//! The `_async` entry points return at once and run the work on a new Rust thread, calling
//! `done` there with the result, so the addon can resolve a promise through a thread-safe
//! function and never blocks the JavaScript thread. Results are JSON strings; errors are
//! [`CommandError`]s whose [`CommandError::to_json`] is the rejected promise's message.
//!
//! A command is `{"command": "<name>", "args": {…}}` (the [`Request`] table). The engine host may
//! add `"trusted": true` to a command **the main process** sent (never to one a window sent):
//! commands that write outside the folder (`export_mermaid`) or read another folder
//! (`folder_windows`) require it.
//!
//! **Closing while a board is saved.** A window's final saves (its boards, its remembered state)
//! and its detach travel on different channels (the window's port and the main process's
//! lifecycle message), so `closeWorkspace` can arrive while a save it should follow is still
//! running, or just after the save was dispatched. [`CLOSING_WRITES`] are counted per window from
//! the moment `dispatch` is called; [`Host::close_workspace`] waits (up to [`CLOSE_WAIT`]) until the
//! window has none in flight, so those saves land while the window is still attached.
//!
//! Sink messages are `{"kind":"event","windows":[…],"event":{…}}`,
//! `{"kind":"broadcast","event":{…}}` (an app-wide event for every window the engine host has,
//! including windows that show no folder and so never attach to a workspace here), and
//! `{"kind":"stream","windowId":…,"requestId":…,"message":{…}}` ([`SinkMessage`]). A panic in a
//! command is caught and becomes an `internal` error; the process keeps running.

use std::any::Any;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_engine::CancelToken;

use crate::commands::{FolderExtras, boards};
use crate::events::{EventSink, WindowId, WorkspaceEvent};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::ipc::table::Request;
use crate::options::{AppConfig, WorkspaceOptions};
use crate::registry::Registry;
use crate::shared::lock;
use crate::workspace::Workspace;

/// Where every sink message goes: the addon's thread-safe JavaScript callback.
pub type MessageSink = Arc<dyn Fn(String) + Send + Sync>;

/// The longest [`Host::close_workspace`] waits for the window's closing writes to finish. A save
/// takes milliseconds; the bound only keeps a stuck write from blocking a close forever.
pub const CLOSE_WAIT: Duration = Duration::from_secs(15);

/// Commands a closing window's detach waits for: board writes, and the window's remembered state
/// and bounds (saved one last time as the window closes, e.g. when the app quits). Keep in sync with
/// `CLOSING_WRITES` in `desktop/src/engine-host/router.ts`.
const CLOSING_WRITES: &[&str] = &[
    "save_board",
    "repair_board",
    "publish_board",
    "unpublish_board",
    "rename_board",
    "delete_board",
    "save_window_state",
    "save_window_bounds",
];

/// One message to the engine host's sink.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SinkMessage {
    /// An event for the windows attached to its folder.
    Event {
        /// The windows to deliver it to.
        windows: Vec<WindowId>,
        /// The event.
        event: WorkspaceEvent,
    },
    /// An app-wide event (`preferences_changed`) for every window. Only the engine host knows
    /// every window: one showing no folder never attaches to a workspace, so Rust cannot list it.
    Broadcast {
        /// The event.
        event: WorkspaceEvent,
    },
    /// A message of a streamed command (compile progress, text-search matches). The command's
    /// final result is its `dispatch` resolution.
    Stream {
        /// The window that ran the command.
        window_id: WindowId,
        /// The request.
        request_id: u64,
        /// The command's stream message (see `Commands[name]["stream"]`).
        message: serde_json::Value,
    },
}

/// [`SinkMessage`] by reference, which is what is actually serialized: an event or a stream
/// message goes to JSON once, without copying it first. Its serde attributes must match
/// `SinkMessage`'s (a test checks they serialize alike).
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
enum Outgoing<'a, T: Serialize> {
    Event {
        windows: &'a [WindowId],
        event: &'a WorkspaceEvent,
    },
    Broadcast {
        event: &'a WorkspaceEvent,
    },
    Stream {
        window_id: WindowId,
        request_id: u64,
        message: &'a T,
    },
}

/// The engine host's door into Rust. See the [module documentation](self). Thread-safe and
/// cheap to clone.
#[derive(Clone)]
pub struct Host {
    state: Arc<HostState>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

/// The shared state behind a [`Host`].
pub(crate) struct HostState {
    options: WorkspaceOptions,
    registry: OnceLock<Registry>,
    sink: RwLock<Option<MessageSink>>,
    requests: Mutex<FxHashMap<u64, CancelToken>>,
    folders: Mutex<BTreeMap<String, Arc<FolderExtras>>>,
    writes: Arc<ClosingWrites>,
}

/// [`CLOSING_WRITES`] in flight, per window.
#[derive(Default)]
struct ClosingWrites {
    running: Mutex<FxHashMap<WindowId, usize>>,
    finished: Condvar,
}

/// One counted closing write; dropping it (when the command ends, even by panicking) uncounts it.
struct ClosingWrite {
    writes: Arc<ClosingWrites>,
    window: WindowId,
}

impl ClosingWrites {
    /// Counts the command if it is one of [`CLOSING_WRITES`]. Only the command's name is read here, on the
    /// caller's thread, so the count is in place before `dispatch` returns.
    fn begin(writes: &Arc<Self>, window: WindowId, command: &str) -> Option<ClosingWrite> {
        #[derive(Deserialize)]
        struct Name<'a> {
            #[serde(borrow)]
            command: Cow<'a, str>,
        }
        let name = serde_json::from_str::<Name<'_>>(command).ok()?;
        if !CLOSING_WRITES.contains(&name.command.as_ref()) {
            return None;
        }
        *lock(&writes.running).entry(window).or_default() += 1;
        Some(ClosingWrite {
            writes: Arc::clone(writes),
            window,
        })
    }

    /// Waits until `window` has no closing write in flight, or `timeout` passes; whether it got
    /// there.
    fn wait_idle(&self, window: WindowId, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut running = lock(&self.running);
        while running.get(&window).is_some_and(|count| *count > 0) {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            running = self
                .finished
                .wait_timeout(running, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

impl Drop for ClosingWrite {
    fn drop(&mut self) {
        let mut running = lock(&self.writes.running);
        if let Some(count) = running.get_mut(&self.window) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                running.remove(&self.window);
            }
        }
        drop(running);
        self.writes.finished.notify_all();
    }
}

/// Delivers workspace events to the host's sink and lets the host react to them.
struct HostSink(Weak<HostState>);

impl EventSink for HostSink {
    fn deliver(&self, windows: &[WindowId], event: &WorkspaceEvent) {
        if let Some(state) = self.0.upgrade() {
            // Observe first: the windows react to the event with new requests, which must find
            // the command-side caches already marked stale.
            HostState::observe(&state, event);
            state.deliver(windows, event);
        }
    }
}

impl Host {
    /// A host whose workspaces use the default options.
    pub fn new() -> Self {
        Self::with_options(WorkspaceOptions::default())
    }

    /// A host whose workspaces use `options` (tests turn watching off and shorten timings).
    pub fn with_options(options: WorkspaceOptions) -> Self {
        Self {
            state: Arc::new(HostState {
                options,
                registry: OnceLock::new(),
                sink: RwLock::new(None),
                requests: Mutex::new(FxHashMap::default()),
                folders: Mutex::new(BTreeMap::new()),
                writes: Arc::default(),
            }),
        }
    }

    /// Sets the app folders from the JSON the main process chose. Call once, before anything
    /// else.
    pub fn init(&self, options_json: &str) -> CommandResult<()> {
        let config: AppConfig = serde_json::from_str(options_json)
            .map_err(|error| CommandError::invalid_args(format!("invalid init options: {error}")))?;
        let sink = Arc::new(HostSink(Arc::downgrade(&self.state)));
        let registry = Registry::with_options(config, sink, self.state.options.clone());
        let weak = Arc::downgrade(&self.state);
        registry.set_close_hook(Arc::new(move |workspace: &Workspace| {
            if let Some(state) = weak.upgrade() {
                lock(&state.folders).remove(workspace.folder());
            }
        }));
        self.state
            .registry
            .set(registry)
            .map_err(|_| CommandError::new("already_initialized", "the engine is already initialized"))
    }

    /// Registers the sink that receives every event and stream message (replacing any earlier
    /// one). Called from any thread.
    pub fn subscribe(&self, sink: impl Fn(String) + Send + Sync + 'static) {
        *self.state.sink.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(sink));
    }

    /// Attaches `window` to the folder at `root` (opening its workspace if no window has it) and
    /// returns the `OpenedFolder` JSON. Blocks while the folder opens.
    pub fn open_workspace(&self, window: WindowId, root: &str) -> CommandResult<String> {
        let registry = self.state.registry()?;
        guarded(|| {
            let opened = registry.open(window, Path::new(root))?;
            encode(&opened)
        })
    }

    /// [`Host::open_workspace`] on a new thread; `done` runs there.
    pub fn open_workspace_async(
        &self,
        window: WindowId,
        root: String,
        done: impl FnOnce(CommandResult<String>) + Send + 'static,
    ) {
        let host = self.clone();
        spawn("unfold-open", done, move || host.open_workspace(window, &root));
    }

    /// Detaches `window`, after its closing writes in flight finish (see the
    /// [module documentation](self)). The folder's last window closes its workspace.
    pub fn close_workspace(&self, window: WindowId) -> CommandResult<()> {
        let registry = self.state.registry()?;
        // A write still running after the wait fails cleanly once the window is detached.
        let _ = self.state.writes.wait_idle(window, CLOSE_WAIT);
        guarded(|| {
            if let Ok(workspace) = registry.workspace(window) {
                boards::release_window(&self.state, &workspace, window);
            }
            registry.close(window);
            Ok(())
        })
    }

    /// [`Host::close_workspace`] on a new thread; `done` runs there.
    pub fn close_workspace_async(
        &self,
        window: WindowId,
        done: impl FnOnce(CommandResult<()>) + Send + 'static,
    ) {
        let host = self.clone();
        spawn("unfold-close", done, move || host.close_workspace(window));
    }

    /// Runs one command for `window` and returns its result JSON. Blocks until it finishes;
    /// `cancel(request)` from another thread stops it early.
    pub fn dispatch(&self, window: WindowId, request: u64, command: &str) -> CommandResult<String> {
        let _write = ClosingWrites::begin(&self.state.writes, window, command);
        let cancel = self.state.register(request);
        let result = self.state.run(window, request, command, cancel);
        self.state.unregister(request);
        result
    }

    /// [`Host::dispatch`] on a new thread; `done` runs there with the result. The request is
    /// cancellable as soon as this returns.
    pub fn dispatch_async(
        &self,
        window: WindowId,
        request: u64,
        command: String,
        done: impl FnOnce(CommandResult<String>) + Send + 'static,
    ) {
        let write = ClosingWrites::begin(&self.state.writes, window, &command);
        let cancel = self.state.register(request);
        let state = Arc::clone(&self.state);
        spawn("unfold-command", done, move || {
            let result = state.run(window, request, &command, cancel);
            state.unregister(request);
            drop(write);
            result
        });
    }

    /// Cancels a running command or stream; does nothing when it already finished.
    pub fn cancel(&self, request: u64) {
        if let Some(token) = lock(&self.state.requests).get(&request) {
            token.cancel();
        }
    }

    /// Closes every workspace (the app is quitting).
    pub fn shutdown(&self) {
        if let Some(registry) = self.state.registry.get() {
            registry.shutdown();
        }
    }

    /// The registry, once initialized.
    pub fn registry(&self) -> Option<&Registry> {
        self.state.registry.get()
    }
}

/// Runs `work` on a new thread and hands its result to `done` there. If no thread can be
/// started, `done` gets an `internal` error on the calling thread.
fn spawn<T: Send + 'static>(
    name: &str,
    done: impl FnOnce(CommandResult<T>) + Send + 'static,
    work: impl FnOnce() -> CommandResult<T> + Send + 'static,
) {
    let slot = Arc::new(Mutex::new(Some(done)));
    let inside = Arc::clone(&slot);
    let spawned = std::thread::Builder::new().name(name.to_owned()).spawn(move || {
        let result = guarded(work);
        if let Some(done) = lock(&inside).take() {
            done(result);
        }
    });
    if let Err(error) = spawned
        && let Some(done) = lock(&slot).take()
    {
        done(Err(CommandError::internal(format!(
            "cannot start a thread: {error}"
        ))));
    }
}

/// Runs `work`, turning a panic into an `internal` error.
fn guarded<T>(work: impl FnOnce() -> CommandResult<T>) -> CommandResult<T> {
    catch_unwind(AssertUnwindSafe(work))
        .unwrap_or_else(|panic| Err(CommandError::internal(panic_text(&panic))))
}

fn panic_text(panic: &Box<dyn Any + Send>) -> String {
    let detail = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown cause".to_owned());
    format!("the command panicked: {detail}")
}

/// Serializes a command's result.
pub(crate) fn encode<T: Serialize>(value: &T) -> CommandResult<String> {
    serde_json::to_string(value)
        .map_err(|error| CommandError::internal(format!("cannot encode the result: {error}")))
}

impl HostState {
    fn registry(&self) -> CommandResult<&Registry> {
        self.registry
            .get()
            .ok_or_else(|| CommandError::new("not_initialized", "the engine is not initialized"))
    }

    fn register(&self, request: u64) -> CancelToken {
        let token = CancelToken::new();
        lock(&self.requests).insert(request, token.clone());
        token
    }

    fn unregister(&self, request: u64) {
        lock(&self.requests).remove(&request);
    }

    fn run(
        self: &Arc<Self>,
        window: WindowId,
        request: u64,
        command: &str,
        cancel: CancelToken,
    ) -> CommandResult<String> {
        let registry = self.registry()?;
        guarded(|| {
            let (parsed, trusted) = parse_command(command)?;
            let ctx = Ctx {
                host: self,
                registry,
                window,
                request,
                cancel,
                trusted,
            };
            parsed.run(&ctx)
        })
    }

    /// The command-side state of `folder`, created on first use; dropped when the folder's
    /// workspace closes.
    pub(crate) fn extras(&self, folder: &str) -> Arc<FolderExtras> {
        Arc::clone(lock(&self.folders).entry(folder.to_owned()).or_default())
    }

    fn existing_extras(&self, folder: &str) -> Option<Arc<FolderExtras>> {
        lock(&self.folders).get(folder).cloned()
    }

    /// The open workspace of `folder`.
    pub(crate) fn workspace_of(&self, folder: &str) -> Option<Arc<Workspace>> {
        self.registry
            .get()?
            .workspaces()
            .into_iter()
            .find(|workspace| workspace.folder() == folder)
    }

    fn send<T: Serialize>(&self, message: &Outgoing<'_, T>) {
        let sink = self.sink.read().unwrap_or_else(PoisonError::into_inner).clone();
        if let (Some(sink), Ok(text)) = (sink, serde_json::to_string(message)) {
            sink(text);
        }
    }

    /// Sends `event` to `windows`.
    pub(crate) fn deliver(&self, windows: &[WindowId], event: &WorkspaceEvent) {
        if windows.is_empty() {
            return;
        }
        self.send(&Outgoing::<()>::Event { windows, event });
    }

    /// Sends an app-wide `event` to every window.
    pub(crate) fn broadcast(&self, event: &WorkspaceEvent) {
        self.send(&Outgoing::<()>::Broadcast { event });
    }

    /// Sends one stream message for `request`.
    pub(crate) fn stream<T: Serialize>(&self, window: WindowId, request: u64, message: &T) {
        self.send(&Outgoing::Stream {
            window_id: window,
            request_id: request,
            message,
        });
    }

    /// Keeps command-side state in step with the folder: the file list goes stale when files
    /// change, open boards are checked when board files change on disk, and board health is
    /// recomputed after the graph changes. Board work runs on its own thread: events arrive on
    /// the engine and watcher threads, which must not wait for it.
    fn observe(state: &Arc<Self>, event: &WorkspaceEvent) {
        let Some(folder) = event.folder() else {
            return;
        };
        match event {
            WorkspaceEvent::FilesChanged { .. } | WorkspaceEvent::ConfigChanged { .. } => {
                if let Some(extras) = state.existing_extras(folder) {
                    extras.files_changed();
                }
            }
            WorkspaceEvent::GraphUpdated { .. } => {
                if let Some(extras) = state.existing_extras(folder) {
                    extras.files_changed();
                    if extras.has_open_boards()
                        && extras.health.request()
                        && !Self::in_background(state, folder, boards::refresh_health)
                    {
                        extras.health.release();
                    }
                }
            }
            WorkspaceEvent::BoardsChanged { .. }
                if state
                    .existing_extras(folder)
                    .is_some_and(|extras| extras.has_open_boards()) =>
            {
                let _ = Self::in_background(state, folder, boards::check_disk);
            }
            _ => {}
        }
    }

    /// Runs board work for `folder` on a new thread; whether the thread started.
    fn in_background(
        state: &Arc<Self>,
        folder: &str,
        work: fn(&Arc<HostState>, &Workspace, &FolderExtras),
    ) -> bool {
        let state = Arc::clone(state);
        let folder = folder.to_owned();
        std::thread::Builder::new()
            .name("unfold-boards".to_owned())
            .spawn(move || {
                let workspace = state.workspace_of(&folder);
                let extras = state.existing_extras(&folder);
                if let (Some(workspace), Some(extras)) = (workspace, extras) {
                    // A failure here only leaves health or conflicts unreported until the next
                    // board command; it must not take the process down.
                    let _ = catch_unwind(AssertUnwindSafe(|| work(&state, &workspace, &extras)));
                }
            })
            .is_ok()
    }
}

/// Parses `{"command", "args", "trusted"?}`.
fn parse_command(command: &str) -> CommandResult<(Request, bool)> {
    let invalid = |message: String| CommandError::new("invalid_command", message);
    let mut value: serde_json::Value = serde_json::from_str(command)
        .map_err(|error| invalid(format!("the command is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("the command is not a JSON object".to_owned()))?;
    let trusted = match object.remove("trusted") {
        None => false,
        Some(serde_json::Value::Bool(trusted)) => trusted,
        Some(_) => return Err(invalid("`trusted` must be a boolean".to_owned())),
    };
    let name = match object.get("command") {
        Some(serde_json::Value::String(name)) => name.clone(),
        _ => return Err(invalid("the command has no `command` name".to_owned())),
    };
    if !Request::NAMES.contains(&name.as_str()) {
        return Err(invalid(format!("unknown command `{name}`")));
    }
    object
        .entry("args")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let request = serde_json::from_value(value)
        .map_err(|error| CommandError::invalid_args(format!("{name}: {error}")))?;
    Ok((request, trusted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_and_rejects_bad_ones() {
        let (request, trusted) = parse_command(r#"{"command":"graph_status"}"#).expect("parse");
        assert_eq!(request.name(), "graph_status");
        assert!(!trusted);
        let (_, trusted) =
            parse_command(r#"{"command":"graph_status","args":{},"trusted":true}"#).expect("parse");
        assert!(trusted);
        for (bad, code) in [
            ("nope", "invalid_command"),
            ("[]", "invalid_command"),
            (r#"{"command":"no_such_command"}"#, "invalid_command"),
            (r#"{"command":"graph_status","trusted":"yes"}"#, "invalid_command"),
            (r#"{"command":"card","args":{}}"#, "invalid_args"),
            (r#"{"command":"graph_status","args":{"extra":1}}"#, "invalid_args"),
        ] {
            let error = parse_command(bad).expect_err(bad);
            assert_eq!(error.code, code, "{bad}");
        }
    }

    #[test]
    fn outgoing_messages_serialize_like_sink_messages() {
        let event = WorkspaceEvent::ConfigChanged {
            folder: "/repo".into(),
        };
        let borrowed = serde_json::to_value(Outgoing::<()>::Event {
            windows: &[1, 2],
            event: &event,
        })
        .expect("borrowed");
        let owned = serde_json::to_value(SinkMessage::Event {
            windows: vec![1, 2],
            event,
        })
        .expect("owned");
        assert_eq!(borrowed, owned);
        let event = WorkspaceEvent::PreferencesChanged {
            preferences: crate::state::Preferences::default(),
        };
        let borrowed = serde_json::to_value(Outgoing::<()>::Broadcast { event: &event }).expect("borrowed");
        let owned = serde_json::to_value(SinkMessage::Broadcast { event }).expect("owned");
        assert_eq!(borrowed, owned);
        assert_eq!(owned["kind"], "broadcast");
        let message = serde_json::json!({"done": 3});
        let borrowed = serde_json::to_value(Outgoing::Stream {
            window_id: 4,
            request_id: 7,
            message: &message,
        })
        .expect("borrowed");
        let owned = serde_json::to_value(SinkMessage::Stream {
            window_id: 4,
            request_id: 7,
            message,
        })
        .expect("owned");
        assert_eq!(borrowed, owned);
        assert_eq!(owned["kind"], "stream");
        assert_eq!(owned["requestId"], 7);
    }

    #[test]
    fn panics_become_internal_errors() {
        let result: CommandResult<()> = guarded(|| panic!("boom"));
        let error = result.expect_err("panic");
        assert_eq!(error.code, "internal");
        assert!(error.message.contains("boom"));
    }

    #[test]
    fn spawned_panics_reach_done_as_internal_errors() {
        let (sender, receiver) = std::sync::mpsc::channel();
        spawn::<()>(
            "unfold-test",
            move |result| {
                let _ = sender.send(result);
            },
            || panic!("handler bug"),
        );
        let error = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("done is called")
            .expect_err("panic");
        assert_eq!(error.code, "internal");
        assert!(error.message.contains("handler bug"));
    }

    #[test]
    fn commands_before_init_are_refused() {
        let host = Host::new();
        let error = host
            .dispatch(1, 1, r#"{"command":"graph_status"}"#)
            .expect_err("not initialized");
        assert_eq!(error.code, "not_initialized");
    }
}
