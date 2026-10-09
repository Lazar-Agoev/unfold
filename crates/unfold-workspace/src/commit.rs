//! Turning an engine update into one store commit, and loading the committed graph back on
//! reopen (architecture §4.3 step 4, §7.8).
//!
//! After each successful update the engine thread holds the previous and the new graph. The
//! commit describes only what differs from what the store holds ([`Committed`]):
//!
//! - one `FileChange` per file whose record, disk stamp, structure, edges, or definitions
//!   changed (`FileChange::between` compares structure and edges; definitions and stamps are
//!   compared here against the committed ones);
//! - removed files, entrance changes by key (`EntranceChanges::between`), and the inputs,
//!   applications, and memberships when they changed;
//! - the graph metadata, every time.
//!
//! When the workspace doesn't know what the store holds (a first compile, a recreated or cleared
//! cache, or a commit that failed), every file is written in full with `Writer::replace`, which
//! also drops whatever else the store held. Disk stamps come from `Engine::disk_state`; reader
//! projections from `unfold_reader::project`. Everything goes in one transaction, so a failed
//! commit leaves the previous graph readable.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustc_hash::FxHashMap;
use unfold_core::{ContentHash, Graph, RepoPath};
use unfold_engine::discovery::FileStamp;
use unfold_engine::update::DiskState;
use unfold_engine::{CancelToken, Engine, PreparedUpdate, UpdateOutcome};
use unfold_reader::{Projections, project};
use unfold_store::{
    Application, CommitStats, Definition, DiskStamp, EntranceChanges, FileChange, GraphMeta, Membership,
    Reader, StoreError, Update, Writer,
};

use crate::error::{Result, WorkspaceError};

/// What the store holds besides the graph the engine remembers: each file's disk stamp, the
/// reader projections, and nothing else. It lets a commit skip rows that didn't change.
#[derive(Debug, Clone, Default)]
pub(crate) struct Committed {
    stamps: FxHashMap<RepoPath, DiskStamp>,
    definitions: FxHashMap<RepoPath, Vec<Definition>>,
    applications: Vec<Application>,
    memberships: Vec<Membership>,
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
    }
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
) -> Result<(CommitStats, Committed)> {
    let cancelled = || cancel.is_some_and(CancelToken::is_cancelled);
    let outcome = prepared.outcome();
    let graph = &outcome.graph;
    let projections = project(graph);
    if cancelled() {
        return Err(WorkspaceError::Cancelled);
    }
    let stamps: Vec<DiskStamp> = graph
        .files
        .iter()
        .map(|file| disk_stamp(prepared.disk_state(&file.file.path)))
        .collect();
    let stats = {
        let update = build(
            outcome,
            &projections,
            &stamps,
            committed,
            graph_meta(engine, prepared),
        );
        let written = match committed {
            Some(_) => writer.commit_unless(&update, &cancelled),
            None => writer.replace_unless(&update, &cancelled),
        };
        match written {
            Ok(stats) => stats,
            Err(StoreError::Cancelled) => return Err(WorkspaceError::Cancelled),
            Err(error) => return Err(error.into()),
        }
    };
    let Projections {
        definitions,
        applications,
        memberships,
    } = projections;
    let mut next = Committed {
        stamps: FxHashMap::with_capacity_and_hasher(graph.files.len(), Default::default()),
        definitions: FxHashMap::with_capacity_and_hasher(graph.files.len(), Default::default()),
        applications,
        memberships,
    };
    for ((file, stamp), definitions) in graph.files.iter().zip(stamps).zip(definitions) {
        next.stamps.insert(file.file.path.clone(), stamp);
        next.definitions.insert(file.file.path.clone(), definitions);
    }
    Ok((stats, next))
}

fn build<'a>(
    outcome: &'a UpdateOutcome,
    projections: &'a Projections,
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
                    definitions: Some(projections.file_definitions(index)),
                    ..FileChange::full(file, stamps[index])
                })
                .collect(),
            removed_files: Vec::new(),
            inputs: Some(&graph.inputs),
            entrances: EntranceChanges::all(&graph.entrances),
            applications: Some(&projections.applications),
            memberships: Some(&projections.memberships),
            meta: Some(meta),
        };
    };
    let previous: &Graph = &outcome.previous;
    let mut files = Vec::new();
    for (index, file) in graph.files.iter().enumerate() {
        let path = &file.file.path;
        let definitions = projections.file_definitions(index);
        let stamp = stamps[index];
        let mut change = match previous.file(path.as_str()) {
            Some(before) if committed.stamps.contains_key(path) => {
                let change = FileChange::between(before, file, stamp);
                let row_changed = before.file != file.file || committed.stamps.get(path) != Some(&stamp);
                if !row_changed
                    && !change.has_parts()
                    && committed.definitions.get(path).map(Vec::as_slice) == Some(definitions)
                {
                    continue;
                }
                change
            }
            _ => FileChange::full(file, stamp),
        };
        if committed.definitions.get(path).map(Vec::as_slice) != Some(definitions) {
            change.definitions = Some(definitions);
        }
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
        entrances: EntranceChanges::between(&previous.entrances, &graph.entrances),
        applications: (committed.applications != projections.applications)
            .then_some(&projections.applications[..]),
        memberships: (committed.memberships != projections.memberships)
            .then_some(&projections.memberships[..]),
        meta: Some(meta),
    }
}

/// The committed graph, its files' disk states, and what the store holds, read in one
/// snapshot: what reopening a folder restores the engine from.
pub(crate) struct Loaded {
    pub(crate) graph: Graph,
    pub(crate) disks: Vec<(RepoPath, DiskState)>,
    pub(crate) committed: Committed,
}

/// Reads everything [`Loaded`] holds.
pub(crate) fn load(reader: &Reader) -> Result<Loaded> {
    Ok(reader.snapshot(|reader| {
        let graph = reader.graph()?;
        let rows = reader.files()?;
        let mut committed = Committed {
            applications: reader.applications()?,
            memberships: reader.memberships()?,
            ..Committed::default()
        };
        let mut cursor = None;
        loop {
            let page = reader.definitions(cursor, 4_096)?;
            for row in page.items {
                committed
                    .definitions
                    .entry(row.path)
                    .or_default()
                    .push(Definition {
                        key: row.key,
                        local_id: row.local_id,
                        kind: row.kind,
                        qualified_name: row.qualified_name,
                        label: row.label,
                    });
            }
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        for definitions in committed.definitions.values_mut() {
            definitions.sort_by_key(|definition| definition.local_id);
        }
        let mut disks = Vec::with_capacity(rows.len());
        for row in rows {
            committed.definitions.entry(row.path.clone()).or_default();
            disks.push((row.path.clone(), disk_state(&row.disk)));
            committed.stamps.insert(row.path, row.disk);
        }
        Ok(Loaded {
            graph,
            disks,
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
