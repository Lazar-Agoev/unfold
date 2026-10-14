//! Apps and entrances end to end: `unfold.toml`'s `[[app]]` tables compiled through the engine
//! with the real Python and TypeScript front ends, over temporary folders. Each `code` key gets
//! its state, a missing one its one-candidate fix, and changing only `[[app]]` tables never
//! recompiles or relinks.

mod fixes;
mod settings;
mod states;

use std::cell::Cell;
use std::fs::FileTimes;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;
use unfold_core::vocab::{NodeKind, TargetState};
use unfold_core::{App, Graph, Key, RepoPath};
use unfold_engine::discovery::DiscoverOptions;
use unfold_engine::update::MemoryFactsCache;
use unfold_engine::{Engine, EngineOptions, UpdateOutcome, UpdateRequest};
use unfold_lang_python::Python;
use unfold_lang_typescript::TypeScript;

/// A temporary folder whose writes get strictly increasing modification times, so the cheap
/// size-and-time check always notices them.
pub struct Folder {
    dir: TempDir,
    clock: Cell<u64>,
}

impl Folder {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let folder = Self {
            dir: tempfile::tempdir().expect("temporary folder"),
            clock: Cell::new(0),
        };
        for (path, text) in files {
            folder.write(path, text);
        }
        folder
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, path: &str, text: &str) {
        let full = self.path().join(path);
        std::fs::create_dir_all(full.parent().expect("a parent folder")).expect("create folders");
        std::fs::write(&full, text).expect("write file");
        let tick = self.clock.get() + 1;
        self.clock.set(tick);
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + tick);
        let file = std::fs::File::options()
            .write(true)
            .open(&full)
            .expect("open file");
        file.set_times(FileTimes::new().set_modified(time))
            .expect("set times");
    }

    pub fn remove(&self, path: &str) {
        std::fs::remove_file(self.path().join(path)).expect("remove file");
    }

    /// An engine for the folder with both front ends.
    pub fn engine(&self) -> Engine {
        let options = EngineOptions {
            discover: DiscoverOptions {
                threads: 1,
                global_ignore: false,
            },
            compile_threads: 1,
            background_threads: 1,
            ..EngineOptions::default()
        };
        Engine::new(
            self.path(),
            vec![Arc::new(Python), Arc::new(TypeScript::default())],
            Arc::new(MemoryFactsCache::new()),
            options,
        )
        .expect("an engine for the folder")
    }
}

pub fn compile(engine: &mut Engine) -> UpdateOutcome {
    engine.update(UpdateRequest::compile()).expect("compile")
}

/// A background update for the paths a watcher would report.
pub fn background(engine: &mut Engine, paths: &[&str]) -> UpdateOutcome {
    let paths = paths
        .iter()
        .map(|path| RepoPath::new(path).expect("valid path"))
        .collect();
    engine
        .update(UpdateRequest::background(paths))
        .expect("background update")
}

/// The key of the first node in `path` of `kind` whose key passes `matches`.
pub fn key_of(graph: &Graph, path: &str, kind: NodeKind, matches: impl Fn(&str) -> bool) -> Key {
    let file = graph.file(path).unwrap_or_else(|| panic!("{path} is compiled"));
    file.nodes
        .iter()
        .find(|node| node.kind() == kind && matches(node.key.as_str()))
        .unwrap_or_else(|| panic!("a {kind:?} in {path}"))
        .key
        .clone()
}

/// Every target of the graph's apps as (code, state), in order.
pub fn states(apps: &[App]) -> Vec<(String, TargetState)> {
    apps.iter()
        .flat_map(|app| {
            app.code
                .iter()
                .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
        })
        .map(|target| (target.code.as_str().to_owned(), target.state))
        .collect()
}

/// An `unfold.toml` with one app whose entrances start at `codes`, named by position.
pub fn one_app(prefix: &str, codes: &[&str]) -> String {
    let mut text = format!("{prefix}[[app]]\nname = \"App\"\nentrances = [\n");
    for (index, code) in codes.iter().enumerate() {
        text.push_str(&format!("  {{ name = \"E{index}\", code = '{code}' }},\n"));
    }
    text.push_str("]\n");
    text
}
