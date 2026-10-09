//! Every command handler, by area (contract §4, architecture §12), with its argument and result
//! types.
//!
//! Handlers are plain functions `fn(&Ctx, Args) -> CommandResult<Result>` that the command
//! table ([`crate::ipc::table`]) dispatches to. They run on command threads, never on an engine
//! thread; they read the committed graph through reader pools and never wait for a compile
//! (except `compile` itself). Every command about one project takes `project` first: a handler
//! gets the project with `ctx.project(&args.project)` (never waking it) or its engine with
//! `ctx.live(&args.project)` (waking it), and confines every path it names to that project's
//! folder.
//!
//! [`ProjectExtras`] is the command-side state of one live project, shared by every window: the
//! cached file list that search and text search walk, rendered source files, and the board
//! commands' state ([`board_hooks`]).

pub mod board_hooks;
pub mod boards;
pub mod compiling;
pub mod diagnostics;
pub mod entrances;
pub mod export;
pub mod file_view;
pub mod files;
mod fileset;
pub mod graph;
pub mod projects;
pub mod relations;
pub mod search;
pub mod settings;
pub mod text_search;
pub mod windows;
pub mod workspace;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_engine::discovery::DiscoverOptions;

use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::shared::lock;

pub(crate) use fileset::FileSet;

/// The arguments of a command that takes none: `{}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(type = "Record<string, never>")]
pub struct NoArgs {}

/// The arguments of a command about one project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectArgs {
    /// The project.
    pub project: ProjectId,
}

/// The arguments of a command about one path of a project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectPathArgs {
    /// The project.
    pub project: ProjectId,
    /// The path relative to the project's folder, `/`-separated (`""` for the folder itself).
    pub path: String,
}

/// The arguments of a command about one block or entrance of a project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectKeyArgs {
    /// The project.
    pub project: ProjectId,
    /// The structural key.
    pub key: String,
}

/// A one-based, inclusive range of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineRange {
    /// The first line.
    pub start_line: u32,
    /// The last line.
    pub end_line: u32,
}

/// The command-side state of one live project. See the [module documentation](self). A new one
/// replaces it whenever the project wakes or switches worktree.
#[derive(Debug, Default)]
pub struct ProjectExtras {
    /// Bumped whenever files may have appeared, changed, or disappeared, and after every graph
    /// update.
    generation: AtomicU64,
    files: Mutex<Option<(u64, Arc<FileSet>)>>,
    /// Recently rendered source files.
    pub(crate) rendered: Mutex<file_view::RenderCache>,
    /// Board-health refreshes after graph updates, one at a time.
    pub(crate) health: HealthRefresh,
    /// The board commands' state of this project.
    pub boards: board_hooks::BoardExtras,
}

impl ProjectExtras {
    /// Marks the cached file list and rendered files stale (files or the graph changed).
    pub(crate) fn files_changed(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// The change generation now: anything derived from files or the graph before the next
    /// change is valid for it.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// The files of the folder at `root` under discovery's ignore rules, walked again after
    /// files changed.
    pub(crate) fn file_set(&self, root: &Path, options: &DiscoverOptions) -> CommandResult<Arc<FileSet>> {
        let generation = self.generation();
        if let Some((built, files)) = lock(&self.files).as_ref()
            && *built == generation
        {
            return Ok(Arc::clone(files));
        }
        let files = Arc::new(FileSet::walk(root, options)?);
        *lock(&self.files) = Some((generation, Arc::clone(&files)));
        Ok(files)
    }
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
