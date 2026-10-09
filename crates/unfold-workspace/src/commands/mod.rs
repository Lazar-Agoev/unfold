//! Every command handler, by area (architecture §12), with its argument and result types.
//!
//! Handlers are plain functions `fn(&Ctx, Args) -> CommandResult<Result>` that the command
//! table ([`crate::ipc::table`]) dispatches to. They run on command threads, never on the engine
//! thread; they read the committed graph through the workspace's reader pool and never wait for
//! a compile (except `compile` itself).
//!
//! [`FolderExtras`] is the command-side state of one open folder, shared by its windows: the open
//! board sessions and the cached file list that search and text search walk.

pub mod boards;
pub mod compiling;
pub mod diagnostics;
pub mod entrances;
pub mod export;
pub mod file_view;
pub mod files;
mod fileset;
pub mod folders;
pub mod graph;
pub mod relations;
pub mod search;
pub mod settings;
pub mod text_search;
pub mod windows;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ipc::error::{CommandError, CommandResult};
use crate::shared::lock;
use crate::workspace::Workspace;

pub(crate) use boards::BoardSession;
pub(crate) use fileset::FileSet;

/// The arguments of a command that takes none: `{}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(type = "Record<string, never>")]
pub struct NoArgs {}

/// The arguments of a command about one folder-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    /// The folder-relative path, `/`-separated.
    pub path: String,
}

/// The arguments of a command about one block or entrance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KeyArgs {
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

/// The command-side state of one open folder. See the [module documentation](self).
#[derive(Default)]
pub struct FolderExtras {
    /// Bumped whenever files may have appeared, changed, or disappeared, and after every graph
    /// update.
    generation: AtomicU64,
    files: Mutex<Option<(u64, Arc<FileSet>)>>,
    /// Recently rendered source tabs.
    pub(crate) rendered: Mutex<file_view::RenderCache>,
    /// Open boards by ID.
    pub(crate) boards: Mutex<BTreeMap<String, BoardSession>>,
    /// Open boards' health refreshes after graph updates, one at a time.
    pub(crate) health: boards::HealthRefresh,
    /// Held by each check of open boards against the disk, so the checks (and the conflicts
    /// they deliver) follow the order of the file changes.
    pub(crate) disk_check: Mutex<()>,
}

impl FolderExtras {
    /// Marks the cached file list and rendered files stale (files or the graph changed).
    pub(crate) fn files_changed(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// The change generation now: anything derived from files or the graph before the next
    /// change is valid for it.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Whether any window has a board of this folder open.
    pub(crate) fn has_open_boards(&self) -> bool {
        !lock(&self.boards).is_empty()
    }

    /// The folder's files under discovery's ignore rules, walked again after files changed.
    pub(crate) fn file_set(&self, workspace: &Workspace) -> CommandResult<Arc<FileSet>> {
        let generation = self.generation();
        if let Some((built, files)) = lock(&self.files).as_ref()
            && *built == generation
        {
            return Ok(Arc::clone(files));
        }
        let files = Arc::new(FileSet::walk(
            workspace.root(),
            &workspace.options().engine.discover,
        )?);
        *lock(&self.files) = Some((generation, Arc::clone(&files)));
        Ok(files)
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
