//! Shared helpers: temporary stores, the definition catalog a test graph needs, and a dump of
//! everything a reader can see.

use std::fmt::Write as _;

use tempfile::TempDir;
use unfold_core::{ContentHash, FileGraph, Graph};
use unfold_store::{
    CandidateKind, Definition, DiskStamp, FileChange, GraphMeta, Reader, Store, StoreConfig, Update,
};

pub const ENGINE: &str = "engine-1";

/// A store in a fresh temporary directory.
pub fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create a temporary directory");
    let store =
        Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new(ENGINE)).expect("open the store");
    (dir, store)
}

/// A graph with the definition catalog a commit carries, per file.
pub struct Prepared {
    pub graph: Graph,
    pub definitions: Vec<Vec<Definition>>,
}

impl Prepared {
    pub fn new(graph: Graph) -> Self {
        let definitions = graph.files.iter().map(|file| definitions_of(file)).collect();
        Self { graph, definitions }
    }

    /// The update a first compile commits.
    pub fn first_compile(&self) -> Update<'_> {
        Update {
            files: self
                .graph
                .files
                .iter()
                .zip(&self.definitions)
                .map(|(file, definitions)| FileChange {
                    definitions: Some(definitions),
                    ..FileChange::full(file, stamp(file))
                })
                .collect(),
            removed_files: Vec::new(),
            inputs: Some(&self.graph.inputs),
            apps: Some(&self.graph.apps),
            meta: Some(meta(1)),
        }
    }

    /// The update that turns `previous` into this graph, as the engine would compute it.
    pub fn since<'a>(&'a self, previous: &Prepared) -> Update<'a> {
        let mut files = Vec::new();
        for (index, file) in self.graph.files.iter().enumerate() {
            let definitions = &self.definitions[index];
            let old = previous
                .graph
                .files
                .iter()
                .position(|old| old.file.path == file.file.path);
            let change = match old {
                None => FileChange {
                    definitions: Some(definitions),
                    ..FileChange::full(file, stamp(file))
                },
                Some(old_index) => {
                    let mut change = FileChange::between(&previous.graph.files[old_index], file, stamp(file));
                    if previous.definitions[old_index] != *definitions {
                        change.definitions = Some(definitions);
                    }
                    let unchanged_row = previous.graph.files[old_index].file == file.file;
                    if unchanged_row && !change.has_parts() {
                        continue;
                    }
                    change
                }
            };
            files.push(change);
        }
        let removed_files = previous
            .graph
            .files
            .iter()
            .filter(|old| self.graph.file(old.file.path.as_str()).is_none())
            .map(|old| old.file.path.clone())
            .collect();
        Update {
            files,
            removed_files,
            inputs: (previous.graph.inputs != self.graph.inputs).then_some(&self.graph.inputs[..]),
            apps: (previous.graph.apps != self.graph.apps).then_some(&self.graph.apps[..]),
            meta: Some(meta(2)),
        }
    }
}

/// Every definition node, labeled by its name (an inline function by its form) and qualified
/// by the named definitions between it and its module.
pub fn definitions_of(file: &FileGraph) -> Vec<Definition> {
    file.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind().is_definition())
        .map(|(id, node)| {
            let label: Box<str> = match &node.name {
                Some(name) => name.clone(),
                None if node.parent.is_none() => file.file.path.file_name().into(),
                None => node.form.as_str().into(),
            };
            let mut names = vec![label.to_string()];
            let mut parent = node.parent;
            while let Some(ancestor) = parent.map(|id| &file.nodes[id as usize]) {
                if ancestor.kind().is_definition()
                    && let Some(name) = &ancestor.name
                {
                    names.push(name.to_string());
                }
                parent = ancestor.parent;
            }
            names.reverse();
            Definition {
                key: node.key.clone(),
                node: u32::try_from(id).expect("test files are small"),
                kind: node.kind(),
                qualified_name: names.join(".").into(),
                label,
            }
        })
        .collect()
}

/// A disk stamp derived from the file record.
pub fn stamp(file: &FileGraph) -> DiskStamp {
    DiskStamp {
        hash: file.file.hash,
        mtime_ns: Some(1_700_000_000_000_000_000),
        size: file.nodes.first().map(|module| u64::from(module.range.end)),
    }
}

/// Graph metadata stamped with `compiled_at_ms`.
pub fn meta(compiled_at_ms: i64) -> GraphMeta {
    GraphMeta {
        producers: r#"{"python":"test"}"#.into(),
        settings_hash: ContentHash::of(b"settings"),
        compiled_at_ms,
        complete: true,
        link_basis: r#"{"fingerprint":"test"}"#.into(),
    }
}

/// The keys of every node of every file, for the reverse-index and lookup queries a dump runs.
fn every_key(reader: &Reader) -> Vec<String> {
    let mut keys = Vec::new();
    for file in reader.files().expect("files") {
        let graph = reader
            .file_graph(file.path.as_str())
            .expect("file graph")
            .expect("a listed file");
        keys.extend(graph.nodes.iter().map(|node| node.key.to_string()));
    }
    keys
}

/// Everything a reader can see except the metadata's compile time, as text, so two stores can
/// be compared.
pub fn dump(reader: &Reader) -> String {
    let mut out = String::new();
    for file in &reader.files().expect("files") {
        let _ = writeln!(out, "{file:?}");
        let _ = writeln!(
            out,
            "{:?}",
            reader.file_graph(file.path.as_str()).expect("file graph")
        );
    }
    let _ = writeln!(
        out,
        "{:?}",
        reader.definitions(None, 10_000).expect("definitions")
    );
    for key in every_key(reader) {
        let callers = reader.callers(&key, None, 100).expect("callers");
        let uses = reader.type_uses(&key, None, 100).expect("type uses");
        if !callers.items.is_empty() || !uses.items.is_empty() {
            let _ = writeln!(out, "{key} {callers:?} {uses:?}");
        }
    }
    let _ = writeln!(out, "{:?}", reader.inputs().expect("inputs"));
    let _ = writeln!(out, "{:?}", reader.apps().expect("apps"));
    let mut stats = reader.stats().expect("stats");
    // The file size depends on history.
    stats.database_bytes = 0;
    let _ = writeln!(out, "{stats:?}");
    for kind in [
        CandidateKind::Definition,
        CandidateKind::App,
        CandidateKind::Entrance,
        CandidateKind::File,
    ] {
        for query in ["e", "or", "shop", "order", "card", "post"] {
            let candidates = reader.search(query, kind, &[], 50).expect("search");
            let _ = writeln!(out, "{query} {kind:?} {candidates:?}");
        }
    }
    out
}
