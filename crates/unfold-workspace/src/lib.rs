//! The workspace end to end: the projects a person connected and which of them are live
//! ([`projects`]), and for each live project its `.unfold/` layout, the folder lock, the engine
//! thread that compiles and commits, the watcher and the update scheduler, cancellation, events,
//! problems, settings edits, remembered state, and path confinement
//! (docs/ipc.md#the-addons-surface).
//!
//! # Shape
//!
//! ```text
//! addon ──► Host ──► Projects (workspace.json; one entry per ProjectId; the liveness reaper)
//!            │          │ wake / rest
//!            │          ▼
//!            │       Workspace (one per live project, on its current folder)
//!            │          │ status(), compile(), edit_settings(), readers(), ...
//!            │ EventSink ◄── events (with `project`, to every window)
//!            │                │
//!            │       ┌────────┼─────────────────────┐
//!            │       ▼        ▼                     ▼
//!            │  watcher    engine thread          any thread
//!            │  notify     Engine + Writer:       ReadPool, ReaderCache
//!            │             update ─► commit ─► ReaderIndex ─► events
//!            ▼
//!         commands (one table; every project command names its project)
//! ```
//!
//! # Lifecycle of a project
//!
//! 1. **Added.** `add_project` records a folder in `workspace.json` and compiles it at once. A
//!    folder Unfold can't write `.unfold/` into (read-only, or on a network volume) is refused.
//! 2. **Woken.** A command that uses the project's engine, a window selecting it on Home, or a
//!    board of it opening opens its [`Workspace`]. Opening writes nothing into a folder that was
//!    never compiled. A folder compiled before takes its lock (a second copy of Unfold gets
//!    `open_elsewhere`), opens its store, and is readable at once; the engine thread restores the
//!    stored graph and checks every file by size and modification time in the background.
//! 3. **First Compile.** Creates `.unfold/local/` (with its `.gitignore` and `cache/`) unless
//!    remembered state already did; takes the lock; opens the store; compiles; and commits.
//! 4. **Background updates.** The watcher feeds changed paths to the scheduler; an update
//!    starts about 300 ms after the last change, and newer changes cancel and restart it.
//!    Without usable file-system events the folder is scanned periodically.
//! 5. **Every commit** writes only what changed in one transaction, installs a new
//!    `ReaderIndex`, and sends `graph_updated`, `problems_changed`, and `project_state`, and
//!    `entrances_changed` when the apps and entrances changed. An edit of only `unfold.toml`'s
//!    `[[app]]` tables resolves the entrances again without compiling and sends
//!    `entrances_changed` alone.
//! 6. **Rest.** Once nothing keeps it live and its lease ran out, the project remembers its state
//!    and closes: no watcher, no engine thread, no lock. Commands that can read from disk serve
//!    it from its stored graph, read-only.
//!
//! # Commands
//!
//! On top of this API, [`ipc`] defines the typed command table (every command, its argument,
//! result, and stream types, generated into TypeScript by `cargo xtask gen-ipc`), [`commands`]
//! holds the handlers, and [`host::Host`] is the facade the napi-rs addon wraps: `init`,
//! `subscribe`, `dispatch`, `cancel`, `detach_window`, and `shutdown`. `docs/ipc.md` catalogues
//! the commands and events.

#![forbid(unsafe_code)]

mod announce;
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
pub mod open_boards;
pub mod options;
pub mod problems;
pub mod projects;
mod shared;
pub mod state;
pub mod status;
mod storage;
mod volume;
mod watcher;
pub mod workspace;

pub use compile::{CompileOutcome, CompileProgress, CompileTicket, ProgressFn};
pub use confine::{ConfinedPath, confine};
pub use error::{Result, WorkspaceError};
pub use events::{EventSink, MAX_LISTED, WindowId, WorkspaceEvent};
pub use facts_cache::StoreFactsCache;
pub use host::{Host, MessageSink, SinkMessage};
pub use ipc::{CommandError, CommandResult, Request};
pub use layout::{BOARDS_PATH, CONFIG_PATH, LOCAL_PATH, Layout, UNFOLD_DIR, Unwritable};
pub use lock::FolderLock;
pub use open_boards::OpenBoards;
pub use options::{AppConfig, WorkspaceOptions, default_languages, store_version};
pub use problems::{ProblemCategory, ProblemItem};
pub use projects::{ProjectHandle, ProjectId, ProjectRow, Projects, WorkspaceView};
pub use state::{AppData, AppSettings, LocalState, ShownBoard, WindowBounds, WindowState};
pub use status::{GraphState, GraphStatus, ProjectState, WatchMode};
pub use workspace::{OpenContext, SettingsEditOutcome, Workspace};
