//! The process-wide registry of open folders (architecture §11.2, decision U-37), owned by the
//! addon.
//!
//! Windows attach to folders. Windows on the same folder share one [`Workspace`]: one engine,
//! one graph, one watcher, one SQLite writer. Folder paths are canonicalized, so the same folder
//! reached through a symlink or a relative path is one workspace. A window shows one folder:
//! attaching it elsewhere detaches it first. When the last window detaches, the close hook runs
//! (part 2 saves drafts there) and the workspace closes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{Result, WorkspaceError};
use crate::events::{EventSink, WindowId, WorkspaceEvent};
use crate::options::{AppConfig, WorkspaceOptions, default_languages};
use crate::shared::lock;
use crate::state::{AppData, FolderState};
use crate::status::GraphStatus;
use crate::workspace::{OpenContext, Workspace, canonical_folder};

/// Called with a workspace whose last window detached, before it closes.
pub type CloseHook = Arc<dyn Fn(&Workspace) + Send + Sync>;

/// What `openWorkspace` resolves with.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpenedFolder {
    /// The folder's canonical path.
    pub folder: String,
    /// The folder's name.
    pub name: String,
    /// Whether the folder has a graph (or one is being built).
    pub compiled: bool,
    /// The graph status.
    pub status: GraphStatus,
    /// The folder's remembered windows and background-update switch.
    pub state: FolderState,
    /// The windows now attached, sorted.
    pub windows: Vec<WindowId>,
}

#[derive(Default)]
struct Inner {
    by_root: BTreeMap<PathBuf, Arc<Workspace>>,
    by_window: BTreeMap<WindowId, PathBuf>,
}

/// Every open folder of this process. See the [module documentation](self).
pub struct Registry {
    config: AppConfig,
    app: AppData,
    sink: Arc<dyn EventSink>,
    options: WorkspaceOptions,
    close_hook: Mutex<Option<CloseHook>>,
    inner: Mutex<Inner>,
    /// Serializes opens, so two windows opening one folder at once get one workspace, without
    /// holding `inner` (and blocking lookups) while a folder opens.
    opening: Mutex<()>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Registry {
    /// A registry for the app folders in `config`, sending events to `sink`.
    pub fn new(config: AppConfig, sink: Arc<dyn EventSink>) -> Self {
        Self::with_options(config, sink, WorkspaceOptions::default())
    }

    /// A registry whose workspaces use `options` (the background default and thread limit
    /// come from the app preferences when they are set).
    pub fn with_options(config: AppConfig, sink: Arc<dyn EventSink>, options: WorkspaceOptions) -> Self {
        Self {
            app: AppData::new(config.app_data_dir.clone()),
            config,
            sink,
            options,
            close_hook: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            opening: Mutex::new(()),
        }
    }

    /// The app folders.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// App preferences and recent folders.
    pub fn app(&self) -> &AppData {
        &self.app
    }

    /// Sets the hook that runs when a folder's last window detaches (drafts are saved there).
    pub fn set_close_hook(&self, hook: CloseHook) {
        *lock(&self.close_hook) = Some(hook);
    }

    /// Attaches `window` to the folder at `root`, opening its workspace if no window has it.
    /// The window is detached from any other folder first. Sends `folder_opened` to the window
    /// and records the folder in the recent folders.
    pub fn open(&self, window: WindowId, root: &Path) -> Result<OpenedFolder> {
        let canonical = canonical_folder(root)?;
        let _opening = lock(&self.opening);
        let attached = lock(&self.inner).by_window.get(&window).cloned();
        if let Some(previous) = attached
            && previous != canonical
        {
            self.detach(window);
        }
        let existing = lock(&self.inner).by_root.get(&canonical).cloned();
        let workspace = match existing {
            Some(workspace) => workspace,
            None => {
                let workspace = Workspace::open(&canonical, self.context())?;
                lock(&self.inner)
                    .by_root
                    .insert(canonical.clone(), Arc::clone(&workspace));
                workspace
            }
        };
        workspace.attach(window);
        lock(&self.inner).by_window.insert(window, canonical);
        let opened = OpenedFolder {
            folder: workspace.folder().to_owned(),
            name: workspace.name().to_owned(),
            compiled: workspace.is_compiled(),
            status: workspace.status(),
            state: workspace.folder_state(),
            windows: workspace.windows(),
        };
        self.sink.deliver(
            &[window],
            &WorkspaceEvent::FolderOpened {
                folder: opened.folder.clone(),
                name: opened.name.clone(),
                compiled: opened.compiled,
            },
        );
        // The recent list is a convenience; failing to save it doesn't fail the open.
        let _ = self
            .app
            .add_recent(&opened.folder, &opened.name, crate::commit::now_ms());
        Ok(opened)
    }

    fn context(&self) -> OpenContext {
        let preferences = self.app.preferences();
        let mut options = self.options.clone();
        options.background_updates_default = preferences.background_updates;
        if let Some(limit) = preferences.thread_limit.filter(|limit| *limit > 0) {
            options.engine.compile_threads = limit as usize;
        }
        OpenContext {
            fallback_cache_dir: self.config.fallback_cache_dir.clone(),
            options,
            sink: Arc::clone(&self.sink),
            languages: default_languages(),
        }
    }

    /// Detaches `window`. When it was the folder's last window, runs the close hook and closes
    /// the workspace. Detaching a window that isn't attached does nothing.
    pub fn close(&self, window: WindowId) {
        // Serialized with opens, so an open never attaches to a workspace that is closing.
        let _opening = lock(&self.opening);
        self.detach(window);
    }

    fn detach(&self, window: WindowId) {
        let closing = {
            let mut inner = lock(&self.inner);
            let Some(root) = inner.by_window.remove(&window) else {
                return;
            };
            let Some(workspace) = inner.by_root.get(&root).cloned() else {
                return;
            };
            if workspace.detach(window) > 0 {
                return;
            }
            inner.by_root.remove(&root);
            workspace
        };
        let hook = lock(&self.close_hook).clone();
        if let Some(hook) = hook {
            hook(&closing);
        }
        closing.close();
    }

    /// The workspace `window` is attached to.
    pub fn workspace(&self, window: WindowId) -> Result<Arc<Workspace>> {
        let inner = lock(&self.inner);
        inner
            .by_window
            .get(&window)
            .and_then(|root| inner.by_root.get(root))
            .cloned()
            .ok_or(WorkspaceError::NotAttached(window))
    }

    /// Every open workspace, by folder.
    pub fn workspaces(&self) -> Vec<Arc<Workspace>> {
        lock(&self.inner).by_root.values().cloned().collect()
    }

    /// Detaches every window and closes every workspace (the app is quitting).
    pub fn shutdown(&self) {
        let windows: Vec<WindowId> = lock(&self.inner).by_window.keys().copied().collect();
        for window in windows {
            self.close(window);
        }
    }
}
