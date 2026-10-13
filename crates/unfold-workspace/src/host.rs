//! [`Host`]: the facade the napi-rs addon wraps, one per engine-host process
//! (docs/ipc.md#how-a-command-travels, docs/ipc.md).
//!
//! | Addon function | Host |
//! | --- | --- |
//! | `init(options)` | [`Host::init`] with the JSON `{"appDataDir","logsDir"}`: loads `workspace.json` (every project resting) and starts the liveness reaper |
//! | `subscribe(sink)` | [`Host::subscribe`]: every event and stream message, as one JSON string each |
//! | `dispatch(windowId, requestId, command)` | [`Host::dispatch_async`] (or [`Host::dispatch`]) |
//! | `cancel(requestId)` | [`Host::cancel`] |
//! | `detachWindow(windowId)` | [`Host::detach_window_async`] (or [`Host::detach_window`]) |
//! | `shutdown()` | [`Host::shutdown_async`] (or [`Host::shutdown`]) |
//!
//! The `_async` entry points return at once and run the work on a new Rust thread, calling
//! `done` there with the result, so the addon can resolve a promise through a thread-safe
//! function and never blocks the JavaScript thread. Results are JSON strings; errors are
//! [`CommandError`]s whose [`CommandError::to_json`] is the rejected promise's message.
//!
//! The window attaches to nothing here: it is known by the id its commands carry, and every
//! command that concerns a project names it ([`crate::projects`]). A command is
//! `{"command": "<name>", "args": {…}}` (the [`Request`] table). The engine host may add
//! `"trusted": true` to a command **the main process** sent (never to one a window sent):
//! commands that take an absolute path or keep the window's state (`add_project`,
//! `window_state`, `save_window_state`) require it.
//!
//! **Closing while a board is saved.** A window's final saves (its boards, the remembered window)
//! and its detach travel on different channels (the window's port and the main process's
//! lifecycle message), so `detachWindow` can arrive while a save it should follow is still
//! running, or just after the save was dispatched. [`CLOSING_WRITES`] are counted per window from
//! the moment `dispatch` is called; [`Host::detach_window`] waits (up to [`CLOSE_WAIT`]) until the
//! window has none in flight, so those saves land while the window's boards are still open. Then
//! it closes the window's boards (their sessions go, so their projects can rest) and releases its
//! Home selection. A command of the window that finishes later (an agent's open in the
//! background) can't open a board or select a project for it (`window_closed`); a window that
//! opens again has a new id.
//!
//! Sink messages are `{"kind":"broadcast","event":{…}}` (every event but one is for every window),
//! `{"kind":"event","windows":[w],"event":{…}}` (`board_reloaded`, for the window that opened
//! the board), and `{"kind":"stream","windowId":…,"requestId":…,"message":{…}}` ([`SinkMessage`]). A
//! panic in a command is caught and becomes an `internal` error; the process keeps running.

use std::any::Any;
use std::borrow::Cow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, RwLock, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_engine::CancelToken;

use crate::commands::board_hooks;
use crate::events::{EventSink, WindowId, WorkspaceEvent};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::ipc::table::Request;
use crate::options::{AppConfig, WorkspaceOptions};
use crate::projects::{ProjectId, Projects};
use crate::shared::lock;

/// Where every sink message goes: the addon's thread-safe JavaScript callback.
pub type MessageSink = Arc<dyn Fn(String) + Send + Sync>;

/// The longest [`Host::detach_window`] waits for the window's closing writes to finish. A save
/// takes milliseconds; the bound only keeps a stuck write from blocking a detach forever.
pub const CLOSE_WAIT: Duration = Duration::from_secs(15);

/// Commands a closing window's detach waits for: board and board-folder writes, and the
/// remembered window (saved one last time as the window closes, e.g. when the app quits). Keep in
/// sync with `CLOSING_WRITES` in `desktop/src/engine-host/router.ts`.
const CLOSING_WRITES: &[&str] = &[
    "save_board",
    "repair_board",
    "delete_board",
    "set_board_folder",
    "create_folder",
    "rename_folder",
    "delete_folder",
    "save_window_state",
];

/// One message to the engine host's sink.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SinkMessage {
    /// An event for some windows only (`board_reloaded`, for the window that opened the board).
    Event {
        /// The windows to deliver it to.
        windows: Vec<WindowId>,
        /// The event.
        event: WorkspaceEvent,
    },
    /// An event for every window. Only the engine host knows every window, so it posts the
    /// event to each window port it has.
    Broadcast {
        /// The event.
        event: WorkspaceEvent,
    },
    /// A message of a streamed command (compile progress). The command's
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
    projects: OnceLock<Projects>,
    sink: RwLock<Option<MessageSink>>,
    requests: Mutex<FxHashMap<u64, CancelToken>>,
    writes: Arc<ClosingWrites>,
    reaper: Mutex<Option<Reaper>>,
}

/// The liveness reaper thread ("unfold-liveness"), which rests projects nothing keeps live.
struct Reaper {
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: JoinHandle<()>,
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

/// Sends projects' events to every window, and lets the host react to them first.
struct HostSink(Weak<HostState>);

impl EventSink for HostSink {
    fn emit(&self, event: &WorkspaceEvent) {
        if let Some(state) = self.0.upgrade() {
            // Observe first: the windows react to the event with new requests, which must find
            // the command-side caches already marked stale.
            HostState::observe(&state, event);
            state.broadcast(event);
        }
    }
}

impl Host {
    /// A host whose projects use the default options.
    pub fn new() -> Self {
        Self::with_options(WorkspaceOptions::default())
    }

    /// A host whose projects use `options` (tests turn watching off and shorten timings).
    pub fn with_options(options: WorkspaceOptions) -> Self {
        Self {
            state: Arc::new(HostState {
                options,
                projects: OnceLock::new(),
                sink: RwLock::new(None),
                requests: Mutex::new(FxHashMap::default()),
                writes: Arc::default(),
                reaper: Mutex::new(None),
            }),
        }
    }

    /// Sets the app folders from the JSON the main process chose, loads the workspace (every
    /// project resting), and starts the liveness reaper. Call once, before anything else.
    pub fn init(&self, options_json: &str) -> CommandResult<()> {
        let config: AppConfig = serde_json::from_str(options_json)
            .map_err(|error| CommandError::invalid_args(format!("invalid init options: {error}")))?;
        let sink = Arc::new(HostSink(Arc::downgrade(&self.state)));
        let weak = Arc::downgrade(&self.state);
        let projects = Projects::new(
            config,
            sink,
            self.state.options.clone(),
            Box::new(move |project: &ProjectId| {
                if let Some(state) = weak.upgrade() {
                    state.refresh_health(project);
                }
            }),
        );
        self.state
            .projects
            .set(projects)
            .map_err(|_| CommandError::new("already_initialized", "the engine is already initialized"))?;
        HostState::start_reaper(&self.state);
        Ok(())
    }

    /// Registers the sink that receives every event and stream message (replacing any earlier
    /// one). Called from any thread.
    pub fn subscribe(&self, sink: impl Fn(String) + Send + Sync + 'static) {
        *self.state.sink.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(sink));
    }

    /// Forgets `window` (it closed), after its closing writes in flight finish (see the
    /// [module documentation](self)): closes the boards it opened, with their sessions, and
    /// releases its Home selection. Projects losing interest start their lease.
    pub fn detach_window(&self, window: WindowId) -> CommandResult<()> {
        let projects = self.state.projects()?;
        // A write still running after the wait fails cleanly once the window's boards are closed.
        let _ = self.state.writes.wait_idle(window, CLOSE_WAIT);
        guarded(|| {
            projects.detach_window(window, |project, boards| {
                board_hooks::released(&self.state, project, boards);
            });
            Ok(())
        })
    }

    /// [`Host::detach_window`] on a new thread; `done` runs there.
    pub fn detach_window_async(
        &self,
        window: WindowId,
        done: impl FnOnce(CommandResult<()>) + Send + 'static,
    ) {
        let host = self.clone();
        spawn("unfold-detach", done, move || host.detach_window(window));
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

    /// Stops the reaper and closes every live project, remembering each one's state in
    /// `workspace.json` (the app is quitting; called once, after every window detached).
    pub fn shutdown(&self) -> CommandResult<()> {
        let reaper = lock(&self.state.reaper).take();
        if let Some(reaper) = reaper {
            let (stopped, wake) = &*reaper.stop;
            *lock(stopped) = true;
            wake.notify_all();
            let _ = reaper.thread.join();
        }
        let projects = self.state.projects()?;
        guarded(|| {
            projects.shutdown();
            Ok(())
        })
    }

    /// [`Host::shutdown`] on a new thread; `done` runs there.
    pub fn shutdown_async(&self, done: impl FnOnce(CommandResult<()>) + Send + 'static) {
        let host = self.clone();
        spawn("unfold-shutdown", done, move || host.shutdown());
    }

    /// The workspace's projects, once initialized.
    pub fn projects(&self) -> Option<&Projects> {
        self.state.projects.get()
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
    /// The workspace's projects, once initialized.
    pub(crate) fn projects(&self) -> CommandResult<&Projects> {
        self.projects
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
        let projects = self.projects()?;
        guarded(|| {
            let (parsed, trusted) = parse_command(command)?;
            let ctx = Ctx {
                host: self,
                projects,
                window,
                request,
                cancel,
                trusted,
            };
            parsed.run(&ctx)
        })
    }

    /// Starts the reaper: every `reap_interval` it rests the projects nothing keeps live. It
    /// holds the host weakly, so it ends with the host even without [`Host::shutdown`].
    fn start_reaper(state: &Arc<Self>) {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = Arc::clone(&stop);
        let weak = Arc::downgrade(state);
        let interval = state.options.reap_interval;
        let thread = std::thread::Builder::new()
            .name("unfold-liveness".to_owned())
            .spawn(move || {
                loop {
                    {
                        let (stopped, wake) = &*signal;
                        let guard = wake
                            .wait_timeout_while(lock(stopped), interval, |stopped| !*stopped)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0;
                        if *guard {
                            return;
                        }
                    }
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    if let Some(projects) = state.projects.get() {
                        // A panic here must not end liveness for the rest of the session.
                        let _ = catch_unwind(AssertUnwindSafe(|| projects.reap()));
                    }
                }
            });
        // Without the thread nothing rests, which costs memory but loses nothing.
        if let Ok(thread) = thread {
            *lock(&state.reaper) = Some(Reaper { stop, thread });
        }
    }

    fn send<T: Serialize>(&self, message: &Outgoing<'_, T>) {
        let sink = self.sink.read().unwrap_or_else(PoisonError::into_inner).clone();
        if let (Some(sink), Ok(text)) = (sink, serde_json::to_string(message)) {
            sink(text);
        }
    }

    /// Sends `event` to `windows` only (`board_reloaded`, to the window that opened the board).
    pub(crate) fn deliver(&self, windows: &[WindowId], event: &WorkspaceEvent) {
        if windows.is_empty() {
            return;
        }
        self.send(&Outgoing::<()>::Event { windows, event });
    }

    /// Sends `event` to every window.
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

    /// Keeps command-side state in step with a project: its file list goes stale when files
    /// change, its boards' health is recomputed after its graph changes, and open boards are
    /// reloaded when their files change on disk. Board work runs on its own
    /// threads: events arrive on the engine and watcher threads, which must not wait for it.
    fn observe(state: &Arc<Self>, event: &WorkspaceEvent) {
        let (Some(project), Some(projects)) = (event.project(), state.projects.get()) else {
            return;
        };
        let extras = || projects.handle(project).ok().map(|handle| handle.extras());
        match event {
            WorkspaceEvent::FilesChanged { .. } => {
                if let Some(extras) = extras() {
                    extras.files_changed();
                }
            }
            WorkspaceEvent::ConfigChanged { .. } => {
                if let Some(extras) = extras() {
                    extras.files_changed();
                }
            }
            WorkspaceEvent::GraphUpdated { .. } => {
                if let Some(extras) = extras() {
                    extras.files_changed();
                }
                state.refresh_health(project);
            }
            WorkspaceEvent::BoardsChanged { boards, .. } => state.boards_changed(project, boards.clone()),
            _ => {}
        }
    }

    /// Recomputes the health of a live project's boards on its own thread, coalescing requests
    /// (at most one refresh runs per project; one made meanwhile runs it once more).
    pub(crate) fn refresh_health(self: &Arc<Self>, project: &ProjectId) {
        let Some(projects) = self.projects.get() else {
            return;
        };
        let Ok(handle) = projects.handle(project) else {
            return;
        };
        let Some(workspace) = handle.live() else {
            return;
        };
        let extras = handle.extras();
        if !extras.health.request() {
            return;
        }
        let worker = Arc::clone(&extras);
        let started = std::thread::Builder::new()
            .name("unfold-boards".to_owned())
            .spawn(move || {
                loop {
                    worker.health.begin();
                    // A failure only leaves health stale until the next refresh; it must not take
                    // the process down, nor keep later refreshes from starting.
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        board_hooks::refresh_health(&workspace, &worker);
                    }));
                    if !worker.health.again() {
                        break;
                    }
                }
            });
        if started.is_err() {
            extras.health.release();
        }
    }

    /// Lets the board commands check a live project's open boards against the disk, on its own
    /// thread.
    fn boards_changed(self: &Arc<Self>, project: &ProjectId, boards: Vec<String>) {
        let Some(projects) = self.projects.get() else {
            return;
        };
        let Ok(handle) = projects.handle(project) else {
            return;
        };
        let Some(workspace) = handle.live() else {
            return;
        };
        let extras = handle.extras();
        let state = Arc::clone(self);
        // Without a thread, the board isn't reloaded: its next save writes the window's version.
        let _ = std::thread::Builder::new()
            .name("unfold-boards".to_owned())
            .spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    board_hooks::boards_changed(&state, &workspace, &extras, &boards);
                }));
            });
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
        let (request, trusted) = parse_command(r#"{"command":"app_info"}"#).expect("parse");
        assert_eq!(request.name(), "app_info");
        assert!(!trusted);
        let (_, trusted) =
            parse_command(r#"{"command":"app_info","args":{},"trusted":true}"#).expect("parse");
        assert!(trusted);
        for (bad, code) in [
            ("nope", "invalid_command"),
            ("[]", "invalid_command"),
            (r#"{"command":"no_such_command"}"#, "invalid_command"),
            (r#"{"command":"app_info","trusted":"yes"}"#, "invalid_command"),
            (r#"{"command":"card","args":{}}"#, "invalid_args"),
            (r#"{"command":"app_info","args":{"extra":1}}"#, "invalid_args"),
            (r#"{"command":"problems","args":{}}"#, "invalid_args"),
        ] {
            let error = parse_command(bad).expect_err(bad);
            assert_eq!(error.code, code, "{bad}");
        }
    }

    #[test]
    fn outgoing_messages_serialize_like_sink_messages() {
        let event = WorkspaceEvent::ConfigChanged {
            project: ProjectId::new_unchecked("p-aaaaaaaaaa"),
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
        let event = WorkspaceEvent::ProjectReset {
            project: ProjectId::new_unchecked("p-aaaaaaaaaa"),
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
            .dispatch(1, 1, r#"{"command":"app_info"}"#)
            .expect_err("not initialized");
        assert_eq!(error.code, "not_initialized");
    }

    #[test]
    fn closing_writes_match_the_routers() {
        let router = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/src/engine-host/router.ts"),
        )
        .expect("router.ts");
        let start = router
            .find("export const CLOSING_WRITES")
            .expect("CLOSING_WRITES in router.ts");
        let list = &router[start..];
        let list = &list[..list.find("]);").expect("the end of the list")];
        let names: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
        assert_eq!(names, CLOSING_WRITES);
    }
}
