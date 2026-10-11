//! Shared helpers for the integration tests: the reference graphs committed to a temporary
//! store the way the workspace commits them, and the testdata folders their sources live in.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::path::PathBuf;

use serde_json::Value;
use unfold_core::testkit::testdata_dir;
use unfold_core::vocab::DetectorId;
use unfold_core::{Graph, Key, Launch, LaunchMode, Launches, canonical};
use unfold_reader::project;
use unfold_store::{DiskStamp, EntranceChanges, FileChange, Store, StoreConfig, Update};

/// A reference graph in a temporary store.
pub struct Loaded {
    _dir: tempfile::TempDir,
    pub store: Store,
    pub graph: Graph,
}

/// Loads `canonical/<corpus>.jsonl` into a new store, with projections.
pub fn load(corpus: &str) -> Loaded {
    let text = std::fs::read_to_string(testdata_dir().join(format!("reference/canonical/{corpus}.jsonl")))
        .expect("read the canonical reference");
    let graph = canonical::read(&text).expect("parse the canonical reference");
    let launches = console_scripts(&graph);
    let projections = project(&graph, &launches);
    let dir = tempfile::tempdir().expect("create a temporary directory");
    let mut store =
        Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("test")).expect("open the store");
    let files = graph
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| FileChange {
            definitions: Some(projections.file_definitions(index)),
            ..FileChange::full(file, DiskStamp::default())
        })
        .collect();
    let update = Update {
        files,
        inputs: Some(&graph.inputs),
        entrances: EntranceChanges::all(&graph.entrances),
        applications: Some(&projections.applications),
        memberships: Some(&projections.memberships),
        launches: Some(&launches),
        ..Update::default()
    };
    store.writer.commit(&update).expect("commit the reference graph");
    Loaded {
        _dir: dir,
        store,
        graph,
    }
}

/// The launch records the engine writes for console scripts: each runs its callable's module
/// first.
fn console_scripts(graph: &Graph) -> Launches {
    let launches = graph
        .entrances
        .iter()
        .filter(|entrance| entrance.detector == DetectorId::PyprojectScript)
        .map(|entrance| {
            let module = entrance
                .targets
                .first()
                .and_then(|target| target.target.as_ref())
                .and_then(|key| key.as_str().split_once('#'))
                .filter(|(_, rest)| !rest.is_empty())
                .map(|(path, _)| Key::from_text(&format!("{path}#")));
            Launch {
                entrance: entrance.key.clone(),
                application: None,
                label: None,
                description: None,
                schedule: None,
                folder: ".".into(),
                command: entrance.trigger.clone(),
                mode: Some(LaunchMode::PythonCallable),
                line: 1,
                target_set_by_hand: false,
                import_paths: Vec::new(),
                problem: None,
                seeds: module.into_iter().collect(),
            }
        })
        .collect();
    Launches {
        launches,
        ..Launches::default()
    }
}

/// The testdata folder a playground's sources live in.
pub fn playground_root(playground: &str) -> PathBuf {
    testdata_dir().join("playgrounds").join(playground)
}

/// One file of the PoC reader reference, parsed.
pub fn reference(playground: &str, name: &str) -> Value {
    let path = testdata_dir().join(format!("reference/poc-reader/{playground}/{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("read the reader reference"))
        .expect("parse it")
}

/// Records each difference as one line; a test fails listing them all.
#[derive(Default)]
pub struct Differences(pub Vec<String>);

impl Differences {
    pub fn compare(&mut self, what: &str, expected: &Value, actual: &Value) {
        if expected != actual {
            self.0
                .push(format!("{what}:\n    poc:    {expected}\n    unfold: {actual}"));
        }
    }

    pub fn assert_none(self, what: &str) {
        assert!(
            self.0.is_empty(),
            "{what}: {} differences from the PoC reader\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}
