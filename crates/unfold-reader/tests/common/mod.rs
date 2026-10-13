//! Shared helpers for the integration tests: the golden graphs committed to a temporary store the
//! way the workspace commits them, and the testdata folders their sources live in.

use std::path::PathBuf;

use unfold_core::testkit::testdata_dir;
use unfold_core::{Launches, canonical};
use unfold_reader::project;
use unfold_store::{DiskStamp, EntranceChanges, FileChange, Store, StoreConfig, Update};

/// A golden graph in a temporary store.
pub struct Loaded {
    _dir: tempfile::TempDir,
    pub store: Store,
}

/// Loads `goldens/<playground>.jsonl` into a new store, with projections.
pub fn load(playground: &str) -> Loaded {
    let text = std::fs::read_to_string(testdata_dir().join(format!("goldens/{playground}.jsonl")))
        .expect("read the golden graph");
    let graph = canonical::read(&text).expect("parse the golden graph");
    let launches = Launches::default();
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
    store.writer.commit(&update).expect("commit the golden graph");
    Loaded { _dir: dir, store }
}

/// The testdata folder a playground's sources live in.
pub fn playground_root(playground: &str) -> PathBuf {
    testdata_dir().join("playgrounds").join(playground)
}
