//! Test helpers for the reader's unit tests: a playground's golden graph with its sources, the
//! same graph committed to a temporary store the way the workspace commits it, and hand-built
//! graphs with their sources written to a temporary folder.
//!
//! The reader compiles nothing: a playground's graph is `testdata/goldens/<name>.jsonl`, read
//! through the canonical reader, and its verified source is `testdata/playgrounds/<name>/`, the
//! files the golden was compiled from.
//!
//! ```ignore
//! let playground = Playground::load("ts-orders");
//! let (tree, id) = playground.block("src/routes/orders.ts#createOrder");
//! let index = playground.index();
//! let reader = playground.reader();
//! ```

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use unfold_core::testkit::{GraphBuilder, testdata_dir};
use unfold_core::{FileGraph, Graph, canonical};
use unfold_store::{DiskStamp, FileChange, Reader, Store, StoreConfig, Update};

use crate::catalog;
use crate::reader_index::ReaderIndex;
use crate::tree::{FileTree, Trees};

/// A graph committed to a temporary store, with the folder its sources live in.
pub(crate) struct Playground {
    /// The graph.
    pub graph: Graph,
    /// The folder its sources live in.
    pub root: PathBuf,
    store: Store,
    _store_dir: TempDir,
    _sources: Option<TempDir>,
}

impl Playground {
    /// The golden graph of `testdata/playgrounds/<name>`, with that folder as its source.
    pub fn load(name: &str) -> Self {
        Self::golden(name, &testdata_dir().join("playgrounds").join(name))
    }

    /// The golden graph of the mixed playground (`testdata/playgrounds`, both playgrounds).
    pub fn mixed() -> Self {
        Self::golden("mixed-playground", &testdata_dir().join("playgrounds"))
    }

    /// The golden graph `testdata/goldens/<name>.jsonl`, with `root` as its source.
    fn golden(name: &str, root: &Path) -> Self {
        let text = std::fs::read_to_string(testdata_dir().join(format!("goldens/{name}.jsonl")))
            .expect("read the golden graph");
        let graph = canonical::read(&text).expect("parse the golden graph");
        Self::new(graph, root.to_path_buf(), None)
    }

    /// A hand-built graph, with its sources written to a temporary folder.
    pub fn built(builder: &GraphBuilder) -> Self {
        let sources = tempfile::tempdir().expect("create a temporary directory");
        for snapshot in builder.snapshots() {
            let path = sources.path().join(snapshot.path().as_str());
            std::fs::create_dir_all(path.parent().expect("a file has a folder")).expect("create the folder");
            std::fs::write(path, snapshot.bytes()).expect("write the source");
        }
        let root = sources.path().to_path_buf();
        Self::new(builder.build(), root, Some(sources))
    }

    fn new(graph: Graph, root: PathBuf, sources: Option<TempDir>) -> Self {
        let dir = tempfile::tempdir().expect("create a temporary directory");
        let mut store =
            Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("test")).expect("open the store");
        let definitions: Vec<_> = graph
            .files
            .iter()
            .map(|file| catalog::definitions(file))
            .collect();
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
        store.writer.commit(&update).expect("commit the graph");
        Self {
            graph,
            root,
            store,
            _store_dir: dir,
            _sources: sources,
        }
    }

    /// A reader of the committed graph.
    pub fn reader(&self) -> Reader {
        self.store.readers.get().expect("get a reader")
    }

    /// The graph's index.
    pub fn index(&self) -> ReaderIndex {
        ReaderIndex::from_graph(&self.graph)
    }

    /// The tree of the file at `path`.
    pub fn tree(&self, path: &str) -> FileTree {
        FileTree::new(self.graph.file(path).expect("the file is in the graph").clone())
    }

    /// The tree of the file holding the block with `key`, and the block's ID.
    pub fn block(&self, key: &str) -> (FileTree, u32) {
        let file = self
            .graph
            .files
            .iter()
            .find(|file| file.local_id(key).is_some())
            .unwrap_or_else(|| panic!("{key} is in the graph"));
        let tree = FileTree::new(FileGraph::clone(file));
        let id = tree.id(key).expect("the key is in its file");
        (tree, id)
    }

    /// Every file's tree.
    pub fn trees(&self) -> Trees {
        let mut trees = Trees::new();
        for file in &self.graph.files {
            trees.insert(FileTree::new(FileGraph::clone(file)));
        }
        trees
    }
}
