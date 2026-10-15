//! Every command handler, by area (docs/ipc.md#how-a-command-travels, docs/ipc.md), with its argument and result
//! types.
//!
//! Handlers are plain functions `fn(&Ctx, Args) -> CommandResult<Result>` that the command
//! table ([`crate::ipc::table`]) dispatches to. They run on command threads, never on an engine
//! thread; they read the committed graph through reader pools and never wait for a compile
//! (except `compile` itself, and `update_project_config` when it changed only `[[app]]` tables:
//! it waits for the engine thread to resolve the entrances again). Every command about one
//! project takes `project` first: a handler
//! gets the project with `ctx.project(&args.project)` (never waking it) or its engine with
//! `ctx.live(&args.project)` (waking it), and confines every path it names to that project's
//! folder.
//!
//! [`ProjectExtras`] is the command-side state of one live project, shared by every command:
//! the board commands' state ([`board_hooks`]).

pub mod board_hooks;
pub mod boards;
pub mod code;
pub mod compiling;
pub mod entrances;
pub mod files;
pub mod graph;
pub mod projects;
pub mod relations;
pub mod search;
pub mod settings;
pub mod types;
pub mod windows;
pub mod workspace;

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use unfold_core::Key;

use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// The arguments of a command that takes none: `{}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(type = "Record<string, never>"))]
pub struct NoArgs {}

/// The arguments of a command about one project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ProjectArgs {
    /// The project.
    pub project: ProjectId,
}

/// The arguments of a command about one path of a project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ProjectPathArgs {
    /// The project.
    pub project: ProjectId,
    /// The path relative to the project's folder, `/`-separated (`""` for the folder itself).
    pub path: String,
}

/// The arguments of a command about one block or entrance of a project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ProjectKeyArgs {
    /// The project.
    pub project: ProjectId,
    /// The structural key.
    pub key: String,
}

/// The command-side state of one live project. See the [module documentation](self). A new one
/// replaces it whenever the project wakes.
#[derive(Debug, Default)]
pub struct ProjectExtras {
    /// Board-health refreshes after graph updates, one at a time.
    pub(crate) health: HealthRefresh,
    /// The board commands' state of this project.
    pub boards: board_hooks::BoardExtras,
}

/// Coalesces board-health refreshes: graph updates can arrive faster than a project's boards'
/// health is recomputed, so at most one refresh runs per project, and requests made while it
/// runs make it run once more (with the newest graphs) instead of starting threads that race each
/// other.
#[derive(Debug, Default)]
pub(crate) struct HealthRefresh {
    pending: AtomicBool,
    running: AtomicBool,
}

impl HealthRefresh {
    /// Asks for a refresh; whether the caller must start the refreshing thread (none runs).
    pub(crate) fn request(&self) -> bool {
        self.pending.store(true, Ordering::SeqCst);
        !self.running.swap(true, Ordering::SeqCst)
    }

    /// Gives up the running slot (the thread couldn't start, or the refresh ended or panicked).
    pub(crate) fn release(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Takes the pending request at the start of a refresh.
    pub(crate) fn begin(&self) {
        self.pending.store(false, Ordering::SeqCst);
    }

    /// After a refresh: whether another was requested meanwhile and this thread takes it.
    pub(crate) fn again(&self) -> bool {
        self.release();
        self.pending.load(Ordering::SeqCst) && !self.running.swap(true, Ordering::SeqCst)
    }
}

/// Parses a structural key argument, refusing empty text.
pub(crate) fn key_arg(key: &str) -> CommandResult<&str> {
    if key.is_empty() {
        Err(CommandError::invalid_args("the key is empty"))
    } else {
        Ok(key)
    }
}

/// A structural key argument as a key, refusing empty text.
pub(crate) fn key_of(key: &str) -> CommandResult<Key> {
    Ok(Key::from_text(key_arg(key)?))
}

/// Converts a count to `u32`, saturating.
pub(crate) fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_coalesce_into_one_running_thread() {
        let refresh = HealthRefresh::default();
        assert!(refresh.request(), "the first request starts a thread");
        assert!(
            !refresh.request(),
            "a second request while it runs only marks it pending"
        );
        refresh.begin();
        assert!(!refresh.request());
        assert!(
            refresh.again(),
            "the pending request runs once more on the same thread"
        );
        refresh.begin();
        assert!(!refresh.again(), "nothing pending: the thread ends");
        assert!(refresh.request(), "a later request starts a thread again");
        refresh.release();
    }
}
