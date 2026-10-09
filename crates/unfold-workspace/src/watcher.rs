//! The file watcher (architecture §7.5): `notify` with `notify-debouncer-full`, filtered through
//! discovery's ignore rules.
//!
//! The folder is watched recursively. Each debounced batch of events is sorted into:
//!
//! - **source changes**: paths that pass the always-excluded folders and the ignore files
//!   (`IgnoreRules`, rebuilt when an ignore file changes). They feed the scheduler's pending
//!   set (the background update) and a `files_changed` event (the Files view);
//! - **`.unfold/config.toml`**: `config_changed`, and a pending path so the next update reloads
//!   the settings;
//! - **`.unfold/boards/`** and **`local/drafts/`**: `boards_changed`, for edits made outside the
//!   app (writes the app noted itself are not reported);
//! - **`local/cache/` disappearing**: a cache check, which recreates it and recompiles.
//!
//! `.git/` and everything else under `.unfold/` are dropped on arrival. (Recursive watching
//! can't exclude subtrees on every platform, so "never watched" means their events are never
//! acted on.) An event that says the back end dropped events asks for a full rescan. Errors
//! that mean events can't be trusted (inotify's watch limit, an unsupported file system) and
//! network volumes switch the folder to a periodic scan, which the status reports.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, NoCache, new_debouncer_opt};
use unfold_core::RepoPath;
use unfold_engine::discovery::IgnoreRules;

use crate::events::{MAX_LISTED, WorkspaceEvent};
use crate::layout::{BOARDS_PATH, CONFIG_PATH, Layout, UNFOLD_DIR};
use crate::shared::{Job, Shared, lock};
use crate::status::WatchMode;
use crate::volume::is_network_volume;

/// How long the debouncer waits for a path to go quiet before reporting it. The scheduler's own
/// quiet period (about 300 ms) follows.
const SETTLE: Duration = Duration::from_millis(50);

/// Starts watching the folder; returns how it is watched.
pub(crate) fn start(shared: &Arc<Shared>) -> WatchMode {
    if !shared.options.watch {
        return WatchMode::Off;
    }
    if is_network_volume(&shared.root) {
        return WatchMode::Scanning;
    }
    let mut filter = Filter::new(shared);
    let weak: Weak<Shared> = Arc::downgrade(shared);
    let handler = move |result: DebounceEventResult| {
        if let Some(shared) = weak.upgrade() {
            filter.handle(&shared, result);
        }
    };
    let debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        SETTLE,
        None,
        handler,
        NoCache::new(),
        notify::Config::default(),
    );
    let Ok(mut debouncer) = debouncer else {
        return WatchMode::Scanning;
    };
    if debouncer.watch(&shared.root, RecursiveMode::Recursive).is_err() {
        return WatchMode::Scanning;
    }
    let drafts = lock(&shared.layout).drafts();
    if !drafts.starts_with(&shared.root) && drafts.is_dir() {
        // Drafts in the fallback location live outside the folder.
        let _ = debouncer.watch(&drafts, RecursiveMode::Recursive);
    }
    *lock(&shared.watcher) = Some(debouncer);
    WatchMode::Events
}

/// Also watches `dir`, outside the folder (drafts in the fallback location).
pub(crate) fn watch_outside(shared: &Shared, dir: &Path) {
    if let Some(debouncer) = lock(&shared.watcher).as_mut()
        && dir.is_dir()
    {
        let _ = debouncer.watch(dir, RecursiveMode::Recursive);
    }
}

/// Stops the watcher and waits for its thread.
pub(crate) fn stop(shared: &Shared) {
    let debouncer = lock(&shared.watcher).take();
    if let Some(debouncer) = debouncer {
        debouncer.stop();
    }
}

/// Sends `files_changed` for sorted folder-relative `paths`.
pub(crate) fn emit_files_changed(shared: &Shared, mut paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    let truncated = paths.len() > MAX_LISTED;
    paths.truncate(MAX_LISTED);
    shared.emit(&WorkspaceEvent::FilesChanged {
        folder: shared.folder.clone(),
        paths,
        truncated,
    });
}

/// What one batch of events amounts to.
#[derive(Default)]
struct Batch {
    pending: BTreeSet<RepoPath>,
    files: BTreeSet<String>,
    published: BTreeSet<String>,
    drafts: BTreeSet<String>,
    config: bool,
    ignore_changed: bool,
    cache_gone: bool,
    rescan: bool,
}

/// Sorts event paths; owned by the watcher's handler.
struct Filter {
    root: PathBuf,
    ignore: IgnoreRules,
}

impl Filter {
    fn new(shared: &Shared) -> Self {
        Self {
            root: shared.root.clone(),
            ignore: IgnoreRules::new(&shared.root, &shared.options.engine.discover),
        }
    }

    fn handle(&mut self, shared: &Shared, result: DebounceEventResult) {
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                let untrustworthy = errors
                    .iter()
                    .any(|error| matches!(error.kind, notify::ErrorKind::MaxFilesWatch));
                if untrustworthy {
                    shared.fall_back_to_scanning();
                } else {
                    shared.request_rescan();
                }
                return;
            }
        };
        let layout = lock(&shared.layout).clone();
        let mut batch = Batch::default();
        for event in &events {
            if event.need_rescan() {
                batch.rescan = true;
            }
            if matches!(event.kind, EventKind::Access(_)) {
                continue;
            }
            for path in &event.paths {
                self.classify(shared, path, &layout, &mut batch);
            }
        }
        if batch.ignore_changed {
            self.ignore = IgnoreRules::new(&self.root, &shared.options.engine.discover);
        }
        if batch.cache_gone {
            shared.push_job(Job::CheckCache);
        }
        if batch.rescan {
            shared.request_rescan();
        }
        if !batch.pending.is_empty() {
            shared.add_changes(batch.pending);
        }
        emit_files_changed(shared, batch.files.into_iter().collect());
        if batch.config {
            shared.emit(&WorkspaceEvent::ConfigChanged {
                folder: shared.folder.clone(),
            });
        }
        if !batch.published.is_empty() || !batch.drafts.is_empty() {
            shared.emit(&WorkspaceEvent::BoardsChanged {
                folder: shared.folder.clone(),
                published: batch.published.into_iter().collect(),
                drafts: batch.drafts.into_iter().collect(),
            });
        }
    }

    fn classify(&mut self, shared: &Shared, path: &Path, layout: &Layout, batch: &mut Batch) {
        // Board files are written through a temporary sibling (`.<name>.<…>.tmp`) renamed over
        // them; the temporary file is never a board.
        let temporary = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.') && name.ends_with(".tmp"));
        let own = || temporary || shared.own_writes.contains(path);
        let drafts = layout.drafts();
        if let Ok(rest) = path.strip_prefix(&drafts) {
            if !own() && !rest.as_os_str().is_empty() {
                batch.drafts.insert(rest.to_string_lossy().replace('\\', "/"));
            }
            return;
        }
        let local = layout.local();
        if path.starts_with(&local) {
            if (path == local || path.starts_with(layout.cache())) && !layout.database().exists() {
                batch.cache_gone = true;
            }
            return;
        }
        let Some(text) = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(Path::to_str)
            .map(|text| text.replace('\\', "/"))
        else {
            return;
        };
        let Ok(relative) = RepoPath::new(&text) else {
            return;
        };
        if text == CONFIG_PATH {
            batch.config |= !own();
            batch.pending.insert(relative);
            return;
        }
        if text.starts_with(BOARDS_PATH) && text[BOARDS_PATH.len()..].starts_with('/') {
            if !own() {
                batch.published.insert(text);
            }
            return;
        }
        if text == UNFOLD_DIR || text.starts_with(".unfold/") {
            return;
        }
        if text == ".git/info/exclude" {
            batch.ignore_changed = true;
            batch.pending.insert(relative);
            return;
        }
        if text.split('/').any(|part| part == ".git") {
            return;
        }
        if self.ignore.is_ignored(&relative, path.is_dir()) {
            return;
        }
        if matches!(relative.file_name(), ".gitignore" | ".ignore") {
            batch.ignore_changed = true;
        }
        batch.files.insert(text);
        batch.pending.insert(relative);
    }
}
