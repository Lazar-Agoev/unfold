//! Commits: a first compile round-trips, and later updates write exactly their delta and leave
//! the store equal to a fresh compile of the new graph.

use unfold_core::testkit::sample::sample;
use unfold_core::vocab::{BlockKind, Reason, Resolution, Role, SlotName};
use unfold_core::{Graph, Key};
use unfold_store::{CommitStats, StoreError, Update};

use crate::common::{Prepared, dump, meta, temp_store};

#[test]
fn a_first_compile_round_trips() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    let stats = store.writer.commit(&prepared.first_compile()).unwrap();
    let graph = &prepared.graph;
    assert_eq!(stats.files_written, graph.files.len() as u64);
    assert_eq!(
        stats.blocks_written,
        graph
            .files
            .iter()
            .map(|file| file.blocks.len() as u64)
            .sum::<u64>()
    );
    assert_eq!(stats.entrances_written, 3);

    let reader = store.readers.get().unwrap();
    for file in &graph.files {
        assert_eq!(
            reader.file_graph(file.file.path.as_str()).unwrap().as_ref(),
            Some(file)
        );
    }
    assert_eq!(reader.entrances().unwrap(), graph.entrances);
    assert_eq!(reader.inputs().unwrap(), graph.inputs);
    assert_eq!(reader.applications().unwrap(), prepared.applications);
    assert_eq!(reader.memberships().unwrap(), prepared.memberships);
    let stored = reader.meta().unwrap();
    assert_eq!(stored.engine_version, crate::common::ENGINE);
    assert_eq!(stored.schema_version, unfold_store::SCHEMA_VERSION.to_string());
    assert_eq!(stored.graph, Some(meta(1)));
    assert!(reader.file_graph("src/absent.ts").unwrap().is_none());
}

#[test]
fn the_whole_graph_loads_in_one_read() {
    let (_dir, mut store) = temp_store();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.graph().unwrap(), Graph::default());
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    assert_eq!(reader.graph().unwrap(), prepared.graph);

    let mut after = prepared.graph.clone();
    let removed = after.files.remove(0).file.path;
    let next = Prepared::new(after);
    store.writer.commit(&next.since(&prepared)).unwrap();
    let loaded = reader.graph().unwrap();
    assert_eq!(loaded, next.graph);
    assert!(loaded.file(removed.as_str()).is_none());
}

/// Commits `before`, then the delta to `after`, and checks the result equals a fresh store
/// that committed `after` directly. Returns the delta's statistics.
fn apply_delta(before: Graph, after: Graph) -> CommitStats {
    let before = Prepared::new(before);
    let after = Prepared::new(after);
    let (_dir, mut incremental) = temp_store();
    incremental
        .writer
        .commit(&before.first_compile())
        .expect("first compile");
    let stats = incremental.writer.commit(&after.since(&before)).expect("delta");

    let (_dir2, mut fresh) = temp_store();
    fresh
        .writer
        .commit(&after.first_compile())
        .expect("fresh compile");
    let incremental_dump = dump(&incremental.readers.get().expect("reader"));
    assert_eq!(incremental_dump, dump(&fresh.readers.get().expect("reader")));
    stats
}

#[test]
fn an_edge_only_change_rewrites_one_files_edges() {
    let before = sample().build();
    let mut after = before.clone();
    let orders = after
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "src/orders.ts")
        .unwrap();
    let edge = orders
        .edges
        .iter_mut()
        .find(|edge| &*edge.text == "save")
        .unwrap();
    edge.target = None;
    edge.resolution = Resolution::Unresolved;
    edge.reason = Some(Reason::Ambiguous);
    let edge_count = orders.edges.len() as u64;

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 1);
    assert_eq!(stats.blocks_written, 0);
    assert_eq!(stats.slots_written, 0);
    assert_eq!(stats.edges_written, edge_count);
    assert_eq!(stats.definitions_written, 0);
    assert_eq!(stats.entrances_written + stats.entrances_removed, 0);
}

#[test]
fn removing_a_file_and_its_entrance_deletes_only_them() {
    let before = sample().build();
    let mut after = before.clone();
    after
        .files
        .retain(|file| file.file.path.as_str() != "jobs/tasks.py");
    after
        .entrances
        .retain(|entrance| entrance.evidence.path.as_str() != ".unfold/config.toml");
    after.normalize();

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_removed, 1);
    assert_eq!(stats.files_written, 0);
    assert_eq!(stats.entrances_removed, 1);
    assert_eq!(stats.entrances_written, 0);
}

#[test]
fn a_structural_change_replaces_blocks_slots_and_definitions() {
    let before = sample().build();
    let mut after = before.clone();
    // Add a function to helper.ts: a new block, slot, key, and definition. (The store does
    // not validate; it only needs well-formed rows.)
    let helper = after
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "src/helper.ts")
        .unwrap();
    let mut block = helper.blocks[1].clone();
    assert_eq!(block.kind, BlockKind::Function);
    block.local_id = helper.blocks.len() as u32;
    block.parent = Some(0);
    block.role = Some(Role::Body);
    block.name = Some("helperTwo".into());
    block.key = Key::from_text("src/helper.ts#function:helperTwo");
    helper.slots.push(unfold_core::Slot {
        block: block.local_id,
        slot: SlotName::Name,
        index: None,
        range: block.range,
    });
    helper.blocks.push(block);
    let (blocks, slots) = (helper.blocks.len() as u64, helper.slots.len() as u64);

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 1);
    assert_eq!(stats.blocks_written, blocks);
    assert_eq!(stats.slots_written, slots);
    assert_eq!(stats.edges_written, 0);
    assert_eq!(stats.definitions_written, 2);
}

#[test]
fn an_empty_update_writes_nothing() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let before = dump(&store.readers.get().unwrap());
    let update = Update::default();
    assert!(update.is_empty());
    let stats = store.writer.commit(&update).unwrap();
    assert_eq!(
        stats.files_written + stats.files_removed + stats.entrances_written,
        0
    );
    assert_eq!(dump(&store.readers.get().unwrap()), before);
    assert_eq!(store.readers.get().unwrap().meta().unwrap().graph, Some(meta(1)));
}

#[test]
fn clear_empties_the_graph_and_facts_but_keeps_the_versions() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    store.writer.clear().unwrap();
    let reader = store.readers.get().unwrap();
    assert!(reader.files().unwrap().is_empty());
    assert!(reader.entrances().unwrap().is_empty());
    let stored = reader.meta().unwrap();
    assert_eq!(stored.engine_version, crate::common::ENGINE);
    assert_eq!(stored.graph, None);
    assert_eq!(reader.stats().unwrap().keys, 0);
    drop(reader);
    // The cleared store accepts a new first compile.
    store.writer.commit(&prepared.first_compile()).unwrap();
    assert_eq!(store.readers.get().unwrap().files().unwrap().len(), 4);
}

#[test]
fn replace_makes_the_store_exactly_the_update() {
    let before = Prepared::new(sample().build());
    let mut after = before.graph.clone();
    after
        .files
        .retain(|file| file.file.path.as_str() != "jobs/tasks.py");
    after
        .entrances
        .retain(|entrance| entrance.evidence.path.as_str() != ".unfold/config.toml");
    after.normalize();
    let after = Prepared::new(after);

    let (_dir, mut replaced) = temp_store();
    replaced.writer.commit(&before.first_compile()).unwrap();
    // A full update of `after`, as a caller that no longer knows what the store holds sends.
    let stats = replaced.writer.replace(&after.first_compile()).unwrap();
    assert_eq!(stats.files_removed, 1);
    assert_eq!(stats.entrances_removed, 1);

    let (_dir2, mut fresh) = temp_store();
    fresh.writer.commit(&after.first_compile()).unwrap();
    assert_eq!(
        dump(&replaced.readers.get().unwrap()),
        dump(&fresh.readers.get().unwrap())
    );

    // Sets the update leaves out are emptied, not kept.
    let bare = Update {
        inputs: None,
        applications: None,
        memberships: None,
        ..after.first_compile()
    };
    replaced.writer.replace(&bare).unwrap();
    let reader = replaced.readers.get().unwrap();
    assert!(reader.inputs().unwrap().is_empty());
    assert!(reader.applications().unwrap().is_empty());
    assert!(reader.memberships().unwrap().is_empty());
    assert_eq!(reader.files().unwrap().len(), after.graph.files.len());
}

/// A cancelled commit or replace writes nothing, whenever it is cancelled: between files or
/// just before the transaction commits. The writer stays usable, and the next commit lands as
/// if the cancelled one never started.
#[test]
fn a_cancelled_commit_writes_nothing() {
    let before = Prepared::new(sample().build());
    let mut after = before.graph.clone();
    let removed = after.files.remove(0).file.path;
    let after = Prepared::new(after);
    let (_dir, mut store) = temp_store();
    store.writer.commit(&before.first_compile()).unwrap();
    let stored = dump(&store.readers.get().unwrap());

    // Each update is asked once before each file and once before committing: cancel it after
    // every number of answers short of all of them.
    let delta = after.since(&before);
    let full = after.first_compile();
    for (replace, update) in [(false, &delta), (true, &full)] {
        for allowed in 0..=update.files.len() {
            let asked = std::cell::Cell::new(0);
            let cancelled = || {
                asked.set(asked.get() + 1);
                asked.get() > allowed
            };
            let result = if replace {
                store.writer.replace_unless(update, &cancelled)
            } else {
                store.writer.commit_unless(update, &cancelled)
            };
            assert!(matches!(result, Err(StoreError::Cancelled)), "{result:?}");
            assert_eq!(
                dump(&store.readers.get().unwrap()),
                stored,
                "after {allowed} files"
            );
        }
    }

    store
        .writer
        .commit_unless(&after.since(&before), &|| false)
        .unwrap();
    let (_fresh_dir, mut fresh) = temp_store();
    fresh.writer.commit(&after.first_compile()).unwrap();
    assert_eq!(
        dump(&store.readers.get().unwrap()),
        dump(&fresh.readers.get().unwrap())
    );
    assert!(
        store
            .readers
            .get()
            .unwrap()
            .file_graph(removed.as_str())
            .unwrap()
            .is_none()
    );
}
