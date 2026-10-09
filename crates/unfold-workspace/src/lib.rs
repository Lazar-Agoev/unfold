//! One open folder end to end: the `.unfold/` layout and its fallback location, the folder
//! lock, the engine thread that compiles and commits, the watcher and the update scheduler,
//! cancellation, events, problems and the Index log, settings edits, remembered state, path
//! confinement, and board claims per window (architecture §4.2, §7, §8, §11.2, §12, §13).
//!
//! # Shape
//!
//! ```text
//! addon ──► Registry ──open(window, root)──► Workspace (one per canonical folder, shared by its windows)
//!              │                                 │
//!              │ EventSink ◄── events ───────────┤  status(), compile(), edit_settings(), readers(), ...
//!                                                │
//!                       ┌────────────────────────┼─────────────────────────┐
//!                       ▼                        ▼                         ▼
//!                watcher thread            engine thread               any thread
//!              notify + debouncer   Engine + Writer: update ─► commit   ReadPool, ReaderCache
//!              ─► pending paths      ─► ReaderIndex ─► events
//! ```
//!
//! # Lifecycle of a folder
//!
//! 1. **Open.** [`Registry::open`] canonicalizes the path and attaches the window to the
//!    folder's workspace, opening it if needed. Opening writes nothing into a folder that was
//!    never compiled. A folder compiled before takes its lock (a second copy of Unfold gets
//!    `open_elsewhere`), opens its store, and is readable at once; the engine thread restores
//!    the stored graph and checks every file by size and modification time in the background.
//!    A store recreated because the engine version changed (or found missing or damaged) is
//!    compiled again in the background.
//! 2. **First Compile.** Creates `.unfold/local/` (with its `.gitignore`, `cache/`, and
//!    `drafts/`), or the fallback location for a read-only folder or a network volume; takes
//!    the lock; opens the store; compiles; commits; and writes `state.json`.
//! 3. **Background updates.** The watcher feeds changed paths to the scheduler; an update
//!    starts about 300 ms after the last change, newer changes cancel and restart it, and a
//!    pause switch per folder holds them (counting what changed). Without usable file-system
//!    events the folder is scanned periodically.
//! 4. **Every commit** writes only what changed in one transaction, installs a new
//!    `ReaderIndex`, and sends `graph_updated`, `problems_changed`, and `graph_status` to every
//!    window of the folder.
//! 5. **Close.** The last window's detach runs the close hook (drafts) and stops the threads.
//!
//! # Commands
//!
//! On top of this API, [`ipc`] defines the typed command table (every command, its argument,
//! result, and stream types, generated into TypeScript by `cargo xtask gen-ipc`), [`commands`]
//! holds the handlers (folders, compile, files, graph, search, text search, boards, export,
//! settings, window state), and [`host::Host`] is the facade the napi-rs addon wraps: `init`,
//! `subscribe`, `open_workspace`, `close_workspace`, `dispatch`, and `cancel` (brief §6).
//! `docs/ipc.md` catalogues the commands and events.

#![forbid(unsafe_code)]

pub mod claims;
pub mod commands;
mod commit;
pub mod compile;
pub mod confine;
mod engine_thread;
pub mod error;
pub mod events;
pub mod facts_cache;
pub mod host;
pub mod ipc;
pub mod layout;
pub mod lock;
pub mod options;
pub mod problems;
mod publish;
pub mod registry;
pub mod settings_edit;
mod shared;
pub mod state;
pub mod status;
mod storage;
mod volume;
mod watcher;
pub mod workspace;

pub use claims::BoardClaims;
pub use compile::{CompileMode, CompileOutcome, CompileProgress, CompileTicket, ProgressFn};
pub use confine::{ConfinedPath, confine};
pub use error::{Result, WorkspaceError};
pub use events::{EventSink, MAX_LISTED, WindowId, WorkspaceEvent};
pub use facts_cache::StoreFactsCache;
pub use host::{Host, MessageSink, SinkMessage};
pub use ipc::{CommandError, CommandResult, Request};
pub use layout::{BOARDS_PATH, CONFIG_PATH, FallbackReason, LOCAL_PATH, Layout, Location, UNFOLD_DIR};
pub use lock::FolderLock;
pub use options::{AppConfig, WorkspaceOptions, default_languages, store_version};
pub use problems::{IndexLogEntry, ProblemCategory, ProblemItem};
pub use registry::{CloseHook, OpenedFolder, Registry};
pub use settings_edit::{
    EditedSettings, EntranceSpec, PythonProjectSpec, ScopeList, SettingsEdit, apply_edits,
};
pub use state::{
    AppData, FolderState, MAX_RECENT, Preferences, RecentFolder, RecentState, Theme, UpdatePreferences,
    WindowBounds, WindowState,
};
pub use status::{GraphState, GraphStatus, WatchMode};
pub use workspace::{OpenContext, SettingsEditOutcome, Workspace};
