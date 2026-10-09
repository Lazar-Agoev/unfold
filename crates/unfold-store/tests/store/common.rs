//! Shared helpers: temporary stores, the reader projections a test graph needs, and a dump of
//! everything a reader can see.

use std::fmt::Write as _;

use tempfile::TempDir;
use unfold_core::vocab::{BlockKind, EntranceKind, Resolution};
use unfold_core::{ContentHash, FileGraph, Graph};
use unfold_store::{
    Application, CandidateKind, Definition, DiskStamp, EntranceChanges, FileChange, GraphMeta, Membership,
    Reader, Store, StoreConfig, Update,
};

pub const ENGINE: &str = "engine-1";

/// A store in a fresh temporary directory.
pub fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create a temporary directory");
    let store =
        Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new(ENGINE)).expect("open the store");
    (dir, store)
}

/// A graph with the projections a commit carries.
pub struct Prepared {
    pub graph: Graph,
    pub definitions: Vec<Vec<Definition>>,
    pub applications: Vec<Application>,
    pub memberships: Vec<Membership>,
}

impl Prepared {
    pub fn new(graph: Graph) -> Self {
        let definitions = graph.files.iter().map(definitions_of).collect();
        let applications: Vec<Application> = graph
            .entrances
            .iter()
            .filter(|entrance| entrance.kind == EntranceKind::Process)
            .filter_map(|entrance| {
                let target = entrance.targets.first()?;
                let startup = target.target.clone()?;
                let file = graph
                    .files
                    .iter()
                    .find(|file| startup.as_str().starts_with(file.file.path.as_str()))?;
                Some(Application {
                    startup,
                    name: entrance.trigger.clone().unwrap_or_else(|| "app".into()),
                    language: file.file.language,
                })
            })
            .collect();
        // Every non-process entrance with a resolved target belongs to every application.
        let mut memberships: Vec<Membership> = graph
            .entrances
            .iter()
            .filter(|entrance| entrance.kind != EntranceKind::Process)
            .filter(|entrance| {
                entrance
                    .targets
                    .iter()
                    .any(|target| target.resolution == Resolution::Resolved)
            })
            .flat_map(|entrance| {
                applications.iter().map(|application| Membership {
                    entrance: entrance.key.clone(),
                    application: application.startup.clone(),
                })
            })
            .collect();
        memberships.sort();
        Self {
            graph,
            definitions,
            applications,
            memberships,
        }
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
            entrances: EntranceChanges::all(&self.graph.entrances),
            applications: Some(&self.applications),
            memberships: Some(&self.memberships),
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
            entrances: EntranceChanges::between(&previous.graph.entrances, &self.graph.entrances),
            applications: (previous.applications != self.applications).then_some(&self.applications[..]),
            memberships: (previous.memberships != self.memberships).then_some(&self.memberships[..]),
            meta: Some(meta(2)),
        }
    }
}

/// Named function-like and class blocks, qualified by their named ancestors.
pub fn definitions_of(file: &FileGraph) -> Vec<Definition> {
    let definable = |kind: BlockKind| kind.is_function_like() || kind == BlockKind::Class;
    file.blocks
        .iter()
        .filter(|block| definable(block.kind) && block.name.is_some())
        .map(|block| {
            let mut names = vec![block.name.as_deref().unwrap_or_default()];
            let mut parent = block.parent;
            while let Some(id) = parent {
                let ancestor = &file.blocks[id as usize];
                if definable(ancestor.kind) {
                    names.push(ancestor.name.as_deref().unwrap_or("(anonymous)"));
                }
                parent = ancestor.parent;
            }
            names.reverse();
            Definition {
                key: block.key.clone(),
                local_id: block.local_id,
                kind: block.kind,
                qualified_name: names.join(".").into(),
                label: block.name.clone().unwrap_or_default(),
            }
        })
        .collect()
}

/// A disk stamp derived from the file record.
pub fn stamp(file: &FileGraph) -> DiskStamp {
    DiskStamp {
        hash: file.file.hash,
        mtime_ns: Some(1_700_000_000_000_000_000),
        size: file.blocks.first().map(|root| u64::from(root.range.end)),
    }
}

/// Graph metadata stamped with `compiled_at_ms`.
pub fn meta(compiled_at_ms: i64) -> GraphMeta {
    GraphMeta {
        producers: r#"{"typescript":"test"}"#.into(),
        settings_hash: ContentHash::of(b"settings"),
        compiled_at_ms,
        complete: true,
    }
}

/// Everything a reader can see except the metadata's compile time, as text, so two stores can
/// be compared.
pub fn dump(reader: &Reader) -> String {
    let mut out = String::new();
    let files = reader.files().expect("files");
    for file in &files {
        let _ = writeln!(out, "{file:?}");
        let _ = writeln!(
            out,
            "{:?}",
            reader.file_graph(file.path.as_str()).expect("file graph")
        );
        let _ = writeln!(
            out,
            "{:?}",
            reader.definitions_in(file.path.as_str()).expect("definitions")
        );
    }
    let _ = writeln!(out, "{:?}", reader.inputs().expect("inputs"));
    let _ = writeln!(out, "{:?}", reader.entrances().expect("entrances"));
    let _ = writeln!(out, "{:?}", reader.applications().expect("applications"));
    let _ = writeln!(out, "{:?}", reader.memberships().expect("memberships"));
    let mut stats = reader.stats().expect("stats");
    // Interned keys are never deleted and the file size depends on history.
    stats.keys = 0;
    stats.database_bytes = 0;
    let _ = writeln!(out, "{stats:?}");
    for kind in [
        CandidateKind::Definition,
        CandidateKind::Entrance,
        CandidateKind::File,
    ] {
        for query in ["e", "or", "src", "order", "run", "start"] {
            // Ties are ordered by row ID, which depends on history, so compare as sets.
            let mut candidates = reader.search(query, kind, 50).expect("search");
            candidates.sort_by(|a, b| a.key.cmp(&b.key));
            let _ = writeln!(out, "{query} {kind:?} {candidates:?}");
        }
    }
    out
}
