//! Turning an engine update into one store commit, and loading the committed graph back on
//! reopen (docs/engine.md#updates).
//!
//! After each successful update the engine thread holds the previous and the new graph. The
//! commit describes only what differs from what the store holds ([`Committed`]):
//!
//! - one `FileChange` per file whose record, disk stamp, nodes, links, or typed records changed
//!   (`FileChange::between` compares the parts; stamps are compared here against the committed
//!   ones). A file whose nodes changed also carries its definitions, the reader's catalog
//!   entries (`unfold_reader::catalog::definitions`), which depend on its nodes alone;
//! - removed files, and the inputs and apps when they changed;
//! - the graph metadata, every time.
//!
//! When the workspace doesn't know what the store holds (a first compile, a recreated or cleared
//! cache), every file is written in full with `Writer::replace`, which also drops whatever else
//! the store held. An update that kept the graph, every disk stamp, and the metadata (a reopen
//! check that found nothing changed) isn't committed at all ([`unchanged`]). Disk stamps come
//! from `Engine::disk_state`. Everything goes in one transaction, so a failed commit leaves the
//! previous graph readable.
//!
//! An edit of only the `[[app]]` tables re-resolves entrances without an update
//! (`Engine::resolve_entrances`); [`commit_entrances`] writes what that changed: the apps, the
//! settings file's input, and the metadata's completeness and link basis.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::{ContentHash, FileGraph, Graph, RepoPath};
use unfold_engine::discovery::FileStamp;
use unfold_engine::update::{DiskState, EntrancesOutcome};
use unfold_engine::{CancelToken, Engine, LinkBasis, PreparedUpdate, UpdateOutcome};
use unfold_reader::catalog;
use unfold_store::{Definition, DiskStamp, FileChange, GraphMeta, Reader, StoreError, Update, Writer};

use crate::error::{Result, WorkspaceError};

/// What the store holds besides the graph the engine remembers: each file's disk stamp and the
/// graph metadata. It lets a commit skip rows that didn't change.
#[derive(Debug, Clone, Default)]
pub(crate) struct Committed {
    stamps: FxHashMap<RepoPath, DiskStamp>,
    meta: Option<GraphMeta>,
}

/// The engine's disk state as the store's stamp.
pub(crate) fn disk_stamp(state: Option<DiskState>) -> DiskStamp {
    let Some(state) = state else {
        return DiskStamp::default();
    };
    let stamp = state.stamp;
    DiskStamp {
        hash: state.hash,
        mtime_ns: stamp.and_then(|stamp| stamp.modified).map(nanos_since_epoch),
        size: stamp.map(|stamp| stamp.size),
    }
}

/// The store's stamp as the engine's disk state, for [`Engine::restore`].
pub(crate) fn disk_state(stamp: &DiskStamp) -> DiskState {
    DiskState {
        stamp: stamp.size.map(|size| FileStamp {
            size,
            modified: stamp.mtime_ns.map(time_from_nanos),
        }),
        hash: stamp.hash,
    }
}

fn nanos_since_epoch(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_nanos()).map_or(i64::MIN, |nanos| -nanos),
    }
}

fn time_from_nanos(nanos: i64) -> SystemTime {
    let magnitude = Duration::from_nanos(nanos.unsigned_abs());
    if nanos >= 0 {
        UNIX_EPOCH + magnitude
    } else {
        UNIX_EPOCH - magnitude
    }
}

/// Milliseconds since the Unix epoch, now.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// The graph metadata for a commit made now of `prepared`, an update of `engine`.
pub(crate) fn graph_meta(engine: &Engine, prepared: &PreparedUpdate) -> GraphMeta {
    let producers: Vec<serde_json::Value> = engine
        .producers()
        .into_iter()
        .map(|(language, producer)| {
            serde_json::json!({
                "language": language.as_str(),
                "name": producer.name,
                "version": producer.version,
            })
        })
        .collect();
    GraphMeta {
        producers: serde_json::Value::Array(producers).to_string().into(),
        settings_hash: ContentHash::from_bytes(*prepared.settings().hash().as_bytes()),
        compiled_at_ms: now_ms(),
        complete: prepared.outcome().complete,
        link_basis: prepared.outcome().basis.encode().into(),
    }
}

/// Whether committing `prepared` (an update of `engine`) would change nothing the store holds
/// (`committed`): it kept the engine's graph, every file's disk stamp is the stored one, and so
/// is the metadata but for its time. The update is then adopted without a commit, and the
/// store keeps the time of the commit that last changed it.
pub(crate) fn unchanged(engine: &Engine, prepared: &PreparedUpdate, committed: &Committed) -> bool {
    let outcome = prepared.outcome();
    let graph = &outcome.graph;
    let Some(stored) = &committed.meta else {
        return false;
    };
    let meta = GraphMeta {
        compiled_at_ms: stored.compiled_at_ms,
        ..graph_meta(engine, prepared)
    };
    Arc::ptr_eq(graph, &outcome.previous)
        && meta == *stored
        && committed.stamps.len() == graph.files.len()
        && graph.files.iter().all(|file| {
            let path = &file.file.path;
            committed.stamps.get(path) == Some(&disk_stamp(prepared.disk_state(path)))
        })
}

/// Commits `prepared` (an update of `engine` it hasn't adopted yet) against what the store
/// holds (`None`: unknown, so everything is replaced) and returns what it holds afterwards.
///
/// With a `cancel` token, a cancellation before the transaction commits abandons it with
/// [`WorkspaceError::Cancelled`](crate::WorkspaceError::Cancelled), writing nothing.
pub(crate) fn commit(
    writer: &mut Writer,
    engine: &Engine,
    prepared: &PreparedUpdate,
    committed: Option<&Committed>,
    cancel: Option<&CancelToken>,
) -> Result<Committed> {
    let cancelled = || cancel.is_some_and(CancelToken::is_cancelled);
    let outcome = prepared.outcome();
    let graph = &outcome.graph;
    let stamps: Vec<DiskStamp> = graph
        .files
        .iter()
        .map(|file| disk_stamp(prepared.disk_state(&file.file.path)))
        .collect();
    let definitions = definitions(outcome, committed.is_some());
    if cancelled() {
        return Err(WorkspaceError::Cancelled);
    }
    let meta = graph_meta(engine, prepared);
    let update = build(outcome, &definitions, &stamps, committed, meta.clone());
    let written = match committed {
        Some(_) => writer.commit_unless(&update, &cancelled),
        None => writer.replace_unless(&update, &cancelled),
    };
    written.map_err(|error| match error {
        StoreError::Cancelled => WorkspaceError::Cancelled,
        other => other.into(),
    })?;
    Ok(Committed {
        stamps: graph
            .files
            .iter()
            .map(|file| file.file.path.clone())
            .zip(stamps)
            .collect(),
        meta: Some(meta),
    })
}

/// The catalog entries of every file whose nodes the store doesn't hold yet, by file index:
/// all of them when the store's contents are unknown (`known` is false), otherwise the files
/// that are new or whose nodes changed.
fn definitions(outcome: &UpdateOutcome, known: bool) -> Vec<Option<Vec<Definition>>> {
    let previous: &Graph = &outcome.previous;
    let stale = |file: &FileGraph| {
        !known
            || previous
                .file(file.file.path.as_str())
                .is_none_or(|before| before.nodes != file.nodes)
    };
    outcome
        .graph
        .files
        .par_iter()
        .map(|file| stale(file).then(|| catalog::definitions(file)))
        .collect()
}

fn build<'a>(
    outcome: &'a UpdateOutcome,
    definitions: &'a [Option<Vec<Definition>>],
    stamps: &[DiskStamp],
    committed: Option<&Committed>,
    meta: GraphMeta,
) -> Update<'a> {
    let graph: &'a Graph = &outcome.graph;
    let Some(committed) = committed else {
        return Update {
            files: graph
                .files
                .iter()
                .enumerate()
                .map(|(index, file)| FileChange {
                    definitions: definitions[index].as_deref(),
                    ..FileChange::full(file, stamps[index])
                })
                .collect(),
            removed_files: Vec::new(),
            inputs: Some(&graph.inputs),
            apps: Some(&graph.apps),
            meta: Some(meta),
        };
    };
    let previous: &Graph = &outcome.previous;
    let mut files = Vec::new();
    for (index, file) in graph.files.iter().enumerate() {
        let path = &file.file.path;
        let stamp = stamps[index];
        let mut change = match previous.file(path.as_str()) {
            Some(before) if committed.stamps.contains_key(path) => {
                let change = FileChange::between(before, file, stamp);
                let row_changed = before.file != file.file || committed.stamps.get(path) != Some(&stamp);
                if !row_changed && !change.has_parts() {
                    continue;
                }
                change
            }
            _ => FileChange::full(file, stamp),
        };
        change.definitions = definitions[index].as_deref();
        files.push(change);
    }
    let mut removed_files: Vec<RepoPath> = committed
        .stamps
        .keys()
        .filter(|path| graph.file(path.as_str()).is_none())
        .cloned()
        .collect();
    removed_files.sort();
    Update {
        files,
        removed_files,
        inputs: (previous.inputs != graph.inputs).then_some(&graph.inputs[..]),
        apps: (previous.apps != graph.apps).then_some(&graph.apps[..]),
        meta: Some(meta),
    }
}

/// Commits what re-resolving the entrances changed (`outcome`, which the engine has adopted):
/// the apps and the settings file's input when they changed, and the metadata when the graph's
/// completeness (`complete`) or its basis did (the basis keeps the settings file's text). The
/// graph keeps its commit time. Returns what the store holds afterwards.
pub(crate) fn commit_entrances(
    writer: &mut Writer,
    outcome: &EntrancesOutcome,
    complete: bool,
    committed: &Committed,
) -> Result<Committed> {
    let graph = &outcome.graph;
    let link_basis: Box<str> = outcome.basis.encode().into();
    let meta = committed
        .meta
        .as_ref()
        .filter(|meta| meta.complete != complete || meta.link_basis != link_basis)
        .map(|meta| GraphMeta {
            complete,
            link_basis,
            ..meta.clone()
        });
    let update = Update {
        files: Vec::new(),
        removed_files: Vec::new(),
        inputs: outcome.inputs_changed.then_some(&graph.inputs[..]),
        apps: outcome.apps_changed.then_some(&graph.apps[..]),
        meta: meta.clone(),
    };
    if !update.is_empty() {
        writer.commit(&update)?;
    }
    Ok(Committed {
        stamps: committed.stamps.clone(),
        meta: meta.or_else(|| committed.meta.clone()),
    })
}

/// The committed graph, its files' disk states, what it was linked from, and what the store
/// holds, read in one snapshot: what reopening a folder restores the engine from.
pub(crate) struct Loaded {
    pub(crate) graph: Graph,
    pub(crate) disks: Vec<(RepoPath, DiskState)>,
    pub(crate) basis: Option<LinkBasis>,
    pub(crate) committed: Committed,
}

/// Reads everything [`Loaded`] holds.
pub(crate) fn load(reader: &Reader) -> Result<Loaded> {
    Ok(reader.snapshot(|reader| {
        let graph = reader.graph()?;
        let rows = reader.files()?;
        let meta = reader.meta()?.graph;
        let basis = meta.as_ref().and_then(|meta| LinkBasis::decode(&meta.link_basis));
        let mut committed = Committed {
            stamps: FxHashMap::with_capacity_and_hasher(rows.len(), Default::default()),
            meta,
        };
        let mut disks = Vec::with_capacity(rows.len());
        for row in rows {
            disks.push((row.path.clone(), disk_state(&row.disk)));
            committed.stamps.insert(row.path, row.disk);
        }
        Ok(Loaded {
            graph,
            disks,
            basis,
            committed,
        })
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_round_trip_through_the_store_form() {
        let modified = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789);
        let state = DiskState {
            stamp: Some(FileStamp {
                size: 42,
                modified: Some(modified),
            }),
            hash: Some(ContentHash::of(b"x")),
        };
        let stamp = disk_stamp(Some(state));
        assert_eq!(stamp.size, Some(42));
        assert_eq!(disk_state(&stamp), state);
        let early = UNIX_EPOCH - Duration::from_nanos(5);
        assert_eq!(time_from_nanos(nanos_since_epoch(early)), early);
        assert_eq!(disk_stamp(None), DiskStamp::default());
    }
}
