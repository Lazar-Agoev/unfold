//! How workspaces are opened: app folders from the desktop shell and tunable timings.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use unfold_engine::{EngineOptions, Language};

/// The app folders the desktop shell passes to the addon's `init` (brief §6, architecture §8.2).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// Preferences and recent folders.
    pub app_data_dir: PathBuf,
    /// Logs.
    pub logs_dir: PathBuf,
    /// The parent of each folder's fallback `local/` (`<fallbackCacheDir>/<hash>/`).
    pub fallback_cache_dir: PathBuf,
}

/// Timings and engine settings for a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOptions {
    /// Threads, stacks, and discovery for the engine.
    pub engine: EngineOptions,
    /// How long after the last change a background update starts.
    pub debounce: Duration,
    /// Whether to watch the folder at all (headless tools and some tests don't).
    pub watch: bool,
    /// How often the whole folder is checked when file-system events are unavailable.
    pub scan_interval: Duration,
    /// The least time between two `compile_progress` events.
    pub progress_interval: Duration,
    /// Whether background updates are on for a folder with no `state.json` yet.
    pub background_updates_default: bool,
}

impl Default for WorkspaceOptions {
    fn default() -> Self {
        Self {
            engine: EngineOptions::default(),
            debounce: Duration::from_millis(300),
            watch: true,
            scan_interval: Duration::from_secs(10),
            progress_interval: Duration::from_millis(50),
            background_updates_default: true,
        }
    }
}

/// The languages a workspace registers: TypeScript and Python.
pub fn default_languages() -> Vec<Arc<dyn Language>> {
    vec![
        Arc::new(unfold_lang_typescript::TypeScript::new()),
        Arc::new(unfold_lang_python::Python::new()),
    ]
}

/// The version string the store is keyed by: the engine version plus every front end's name
/// and version. A change to any of them recreates the database, and with it the facts cache,
/// so cached facts always come from the running front ends.
pub fn store_version(languages: &[Arc<dyn Language>]) -> String {
    let mut producers: Vec<String> = languages
        .iter()
        .map(|language| {
            let producer = language.producer();
            format!("{}={}@{}", language.id(), producer.name, producer.version)
        })
        .collect();
    producers.sort();
    format!("{};{}", unfold_engine::ENGINE_VERSION, producers.join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_version_names_every_front_end() {
        let version = store_version(&default_languages());
        assert!(version.starts_with(unfold_engine::ENGINE_VERSION));
        assert!(version.contains("typescript=unfold-lang-typescript@"));
        assert!(version.contains("python=unfold-lang-python@"));
    }

    #[test]
    fn app_config_reads_the_init_json() {
        let config: AppConfig =
            serde_json::from_str(r#"{"appDataDir":"/a","logsDir":"/l","fallbackCacheDir":"/c"}"#)
                .expect("parse");
        assert_eq!(config.fallback_cache_dir, PathBuf::from("/c"));
    }
}
