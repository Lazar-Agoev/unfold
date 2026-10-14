//! Shared helpers for the integration tests: a playground's golden graph committed to a
//! temporary store the way the workspace commits it, and the testdata folder its sources live
//! in. The reader compiles nothing: the graph is `testdata/goldens/<playground>.jsonl`, read
//! through the canonical reader, and its verified source is `testdata/playgrounds/<playground>/`.

use std::path::PathBuf;

use unfold_core::canonical;
use unfold_core::testkit::testdata_dir;
use unfold_reader::catalog;
use unfold_store::{DiskStamp, FileChange, Store, StoreConfig, Update};

/// A golden graph in a temporary store.
pub struct Loaded {
    _dir: tempfile::TempDir,
    pub store: Store,
}

/// Loads `goldens/<playground>.jsonl` into a new store, with its catalog.
pub fn load(playground: &str) -> Loaded {
    let text = std::fs::read_to_string(testdata_dir().join(format!("goldens/{playground}.jsonl")))
        .expect("read the golden graph");
    let graph = canonical::read(&text).expect("parse the golden graph");
    let dir = tempfile::tempdir().expect("create a temporary directory");
    let mut store =
        Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("test")).expect("open the store");
    let definitions: Vec<_> = graph.files.iter().map(catalog::definitions).collect();
    let files = graph
        .files
        .iter()
        .zip(&definitions)
        .map(|(file, definitions)| FileChange {
            definitions: Some(definitions),
            ..FileChange::full(file, DiskStamp::default())
        })
        .collect();
    let update = Update {
        files,
        inputs: Some(&graph.inputs),
        apps: Some(&graph.apps),
        ..Update::default()
    };
    store.writer.commit(&update).expect("commit the golden graph");
    Loaded { _dir: dir, store }
}

/// The testdata folder a playground's sources live in.
pub fn playground_root(playground: &str) -> PathBuf {
    testdata_dir().join("playgrounds").join(playground)
}
