//! Commits: a first compile round-trips every record, and later updates write exactly their
//! delta and leave the store equal to a fresh compile of the new graph.

use unfold_core::testkit::sample::shop;
use unfold_core::testkit::target;
use unfold_core::vocab::{Form, PairingReason, Reason, TargetState};
use unfold_core::{Graph, Key, LinkTarget, Pairing};
use unfold_store::{CommitStats, StoreError, Update};

use crate::common::{ENGINE, Prepared, definitions_of, dump, meta, temp_store};

/// The sample's file at `path`, to edit.
fn file_mut<'g>(graph: &'g mut Graph, path: &str) -> &'g mut unfold_core::FileGraph {
    graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == path)
        .unwrap_or_else(|| panic!("the sample has {path}"))
}

#[test]
fn a_first_compile_round_trips_every_record() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(shop().build());
    let stats = store.writer.commit(&prepared.first_compile()).unwrap();
    let graph = &prepared.graph;
    let count = |f: fn(&unfold_core::FileGraph) -> usize| graph.files.iter().map(f).sum::<usize>() as u64;
    assert_eq!(stats.files_written, graph.files.len() as u64);
    assert_eq!(stats.nodes_written, count(|file| file.nodes.len()));
    assert_eq!(stats.links_written, count(|file| file.links.len()));
    assert_eq!(
        stats.typed_records_written,
        count(|file| {
            let types = &file.types;
            types.signatures.len() + types.parameters.len() + types.members.len() + types.arguments.len()
        })
    );
    assert_eq!(stats.apps_written, 1);
    assert_eq!(stats.entrances_written, 4);

    let reader = store.readers.get().unwrap();
    for file in &graph.files {
        assert_eq!(
            reader.file_graph(file.file.path.as_str()).unwrap().as_ref(),
            Some(file)
        );
    }
    assert_eq!(reader.inputs().unwrap(), graph.inputs);
    assert_eq!(reader.apps().unwrap(), graph.apps);
    assert_eq!(reader.graph().unwrap(), *graph);
    let stored = reader.meta().unwrap();
    assert_eq!(stored.engine_version, ENGINE);
    assert_eq!(stored.schema_version, unfold_store::SCHEMA_VERSION.to_string());
    assert_eq!(stored.graph, Some(meta(1)));
    assert!(reader.file_graph("src/absent.ts").unwrap().is_none());

    // The sample reaches every outcome, so every target column round-tripped.
    let outcomes: std::collections::BTreeSet<&str> = graph
        .files
        .iter()
        .flat_map(|file| &file.links)
        .map(|link| link.to.outcome().as_str())
        .collect();
    assert_eq!(outcomes.len(), 4);
}

#[test]
fn the_whole_graph_loads_in_one_read() {
    let (_dir, mut store) = temp_store();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.graph().unwrap(), Graph::default());
    let prepared = Prepared::new(shop().build());
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
    let reader = incremental.readers.get().expect("reader");
    assert_eq!(reader.graph().expect("graph"), after.graph);

    let (_dir2, mut fresh) = temp_store();
    fresh
        .writer
        .commit(&after.first_compile())
        .expect("fresh compile");
    assert_eq!(dump(&reader), dump(&fresh.readers.get().expect("reader")));
    stats
}

#[test]
fn a_link_only_change_rewrites_one_files_links() {
    let before = shop().build();
    let mut after = before.clone();
    let store = file_mut(&mut after, "shop/store.py");
    let link = store
        .links
        .iter_mut()
        .find(|link| &*link.text == "save_order")
        .unwrap();
    link.to = LinkTarget::Unknown(Reason::Ambiguous);
    let link_count = store.links.len() as u64;

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 1);
    assert_eq!(stats.nodes_written, 0);
    assert_eq!(stats.links_written, link_count);
    assert_eq!(stats.typed_records_written, 0);
    assert_eq!(stats.definitions_written, 0);
    assert_eq!(stats.apps_written, 0);
}

#[test]
fn a_typed_record_change_rewrites_only_that_files_typed_records() {
    let before = shop().build();
    let mut after = before.clone();
    let store = file_mut(&mut after, "shop/store.py");
    let paired = store
        .types
        .arguments
        .iter_mut()
        .find(|argument| matches!(argument.pairing, Some(Pairing::Parameter(_))))
        .unwrap();
    paired.pairing = Some(Pairing::Unpaired(PairingReason::Receiver));
    let typed = {
        let types = &store.types;
        (types.signatures.len() + types.parameters.len() + types.members.len() + types.arguments.len()) as u64
    };

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 1);
    assert_eq!(stats.nodes_written + stats.links_written, 0);
    assert_eq!(stats.typed_records_written, typed);
}

#[test]
fn removing_a_file_deletes_only_it() {
    let before = shop().build();
    let mut after = before.clone();
    after
        .files
        .retain(|file| file.file.path.as_str() != "src/orders.ts");

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_removed, 1);
    assert_eq!(stats.files_written, 0);
}

#[test]
fn a_node_change_replaces_nodes_and_definitions() {
    let before = shop().build();
    let mut after = before.clone();
    // Add a function to the end of store.py: a new node, key, and definition. (The store does
    // not validate; it only needs well-formed rows.)
    let store = file_mut(&mut after, "shop/store.py");
    let mut node = store
        .nodes
        .iter()
        .find(|node| node.form == Form::Function)
        .unwrap()
        .clone();
    node.parent = Some(0);
    node.name = Some("forget_order".into());
    node.key = Key::from_text("shop/store.py#forget_order");
    store.nodes.push(node);
    let nodes = store.nodes.len() as u64;
    let definitions = definitions_of(store).len() as u64;

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 1);
    assert_eq!(stats.nodes_written, nodes);
    assert_eq!(stats.links_written, 0);
    assert_eq!(stats.definitions_written, definitions);
}

#[test]
fn a_changed_app_replaces_the_apps() {
    let before = shop().build();
    let mut after = before.clone();
    let shop_app = &mut after.apps[0];
    shop_app.about = Some("Takes orders.".into());
    shop_app.entrances.remove(1);
    shop_app.entrances[1].code = vec![target("shop/service.py#refund_order", TargetState::NotAnalyzed)];

    let stats = apply_delta(before, after);
    assert_eq!(stats.files_written, 0);
    assert_eq!(stats.apps_written, 1);
    assert_eq!(stats.entrances_written, 3);
}

#[test]
fn an_empty_update_writes_nothing() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(shop().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let before = dump(&store.readers.get().unwrap());
    let update = Update::default();
    assert!(update.is_empty());
    let stats = store.writer.commit(&update).unwrap();
    assert_eq!(stats.files_written + stats.files_removed + stats.apps_written, 0);
    assert_eq!(dump(&store.readers.get().unwrap()), before);
    assert_eq!(store.readers.get().unwrap().meta().unwrap().graph, Some(meta(1)));
}

#[test]
fn clear_empties_the_graph_and_facts_but_keeps_the_versions() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(shop().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    store.writer.clear().unwrap();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.graph().unwrap(), Graph::default());
    let stored = reader.meta().unwrap();
    assert_eq!(stored.engine_version, ENGINE);
    assert_eq!(stored.graph, None);
    let stats = reader.stats().unwrap();
    assert_eq!(
        stats,
        unfold_store::IndexStats {
            database_bytes: stats.database_bytes,
            ..unfold_store::IndexStats::default()
        },
        "every table is empty"
    );
    assert!(
        reader
            .search("order", unfold_store::CandidateKind::Definition, &[], 10)
            .unwrap()
            .is_empty()
    );
    drop(reader);
    // The cleared store accepts a new first compile.
    store.writer.commit(&prepared.first_compile()).unwrap();
    assert_eq!(store.readers.get().unwrap().graph().unwrap(), prepared.graph);
}

#[test]
fn replace_makes_the_store_exactly_the_update() {
    let before = Prepared::new(shop().build());
    let mut after = before.graph.clone();
    after
        .files
        .retain(|file| file.file.path.as_str() != "src/orders.ts");
    after.apps[0].entrances.truncate(1);
    let after = Prepared::new(after);

    let (_dir, mut replaced) = temp_store();
    replaced.writer.commit(&before.first_compile()).unwrap();
    // A full update of `after`, as a caller that no longer knows what the store holds sends.
    let stats = replaced.writer.replace(&after.first_compile()).unwrap();
    assert_eq!(stats.files_removed, 1);

    let (_dir2, mut fresh) = temp_store();
    fresh.writer.commit(&after.first_compile()).unwrap();
    assert_eq!(
        dump(&replaced.readers.get().unwrap()),
        dump(&fresh.readers.get().unwrap())
    );

    // Sets the update leaves out are emptied, not kept.
    let bare = Update {
        inputs: None,
        apps: None,
        ..after.first_compile()
    };
    replaced.writer.replace(&bare).unwrap();
    let reader = replaced.readers.get().unwrap();
    assert!(reader.inputs().unwrap().is_empty());
    assert!(reader.apps().unwrap().is_empty());
    assert!(
        reader
            .search("order", unfold_store::CandidateKind::Entrance, &[], 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(reader.files().unwrap().len(), after.graph.files.len());
}

/// A cancelled commit or replace writes nothing, whenever it is cancelled: between files or
/// just before the transaction commits. The writer stays usable, and the next commit lands as
/// if the cancelled one never started.
#[test]
fn a_cancelled_commit_writes_nothing() {
    let before = Prepared::new(shop().build());
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
