//! Typed records: they round-trip through a commit, are replaced only when they change, and the
//! reverse index of type links answers "used in N places".

use unfold_core::testkit::sample::typed;
use unfold_core::vocab::{PairingReason, Resolution};
use unfold_core::{Graph, Pairing};

use crate::common::{Prepared, dump, temp_store};

#[test]
fn typed_records_round_trip() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(typed().build());
    let stats = store.writer.commit(&prepared.first_compile()).unwrap();
    let records: usize = prepared
        .graph
        .files
        .iter()
        .map(|file| {
            let types = &file.types;
            types.signatures.len()
                + types.parameters.len()
                + types.members.len()
                + types.arguments.len()
                + types.results.len()
                + types.type_links.len()
        })
        .sum();
    assert_eq!(stats.typed_records_written, records as u64);
    let reader = store.readers.get().unwrap();
    for file in &prepared.graph.files {
        assert_eq!(
            reader.file_graph(file.file.path.as_str()).unwrap().as_ref(),
            Some(file)
        );
    }
    assert_eq!(reader.graph().unwrap(), prepared.graph);
}

#[test]
fn the_reverse_index_counts_and_lists_uses() {
    let (_dir, mut store) = temp_store();
    let prepared = Prepared::new(typed().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.type_use_count("src/cart.ts#interface:Cart").unwrap(), 2);
    assert_eq!(reader.type_use_count("src/cart.ts#type_alias:CartId").unwrap(), 1);
    assert_eq!(reader.type_use_count("src/cart.ts#enum:Status").unwrap(), 0);
    let uses = reader.type_uses("src/cart.ts#interface:Cart", 0, 10).unwrap();
    let places: Vec<(&str, &str, u32)> = uses
        .iter()
        .map(|place| (place.path.as_str(), place.block.key.as_str(), place.range.start))
        .collect();
    assert_eq!(
        places,
        [
            ("src/charge.ts", "src/charge.ts#function:charge", 61),
            ("src/charge.ts", "src/charge.ts#function:pay", 147),
        ]
    );
    let page = reader.type_uses("src/cart.ts#interface:Cart", 1, 10).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].block.key.as_str(), "src/charge.ts#function:pay");
    assert!(
        reader
            .type_uses("src/absent.ts#interface:X", 0, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn changed_typed_records_alone_are_rewritten_and_equal_a_fresh_store() {
    let (_dir, mut store) = temp_store();
    let first = Prepared::new(typed().build());
    store.writer.commit(&first.first_compile()).unwrap();

    let mut graph: Graph = first.graph.clone();
    let charge = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "src/charge.ts")
        .unwrap();
    charge.types.arguments[0].pairing = Some(Pairing::Unpaired(PairingReason::Receiver));
    let pay = charge
        .types
        .type_links
        .iter_mut()
        .find(|link| link.range.start == 147)
        .unwrap();
    pay.resolution = Resolution::Unresolved;
    pay.target = None;
    pay.reason = Some(unfold_core::vocab::Reason::Ambiguous);
    let next = Prepared::new(graph);
    let update = next.since(&first);
    let changed: Vec<_> = update
        .files
        .iter()
        .filter(|change| change.has_parts())
        .map(|change| {
            (
                change.record.path.as_str(),
                change.structure.is_some(),
                change.edges.is_some(),
                change.types.is_some(),
            )
        })
        .collect();
    assert_eq!(changed, [("src/charge.ts", false, false, true)]);
    store.writer.commit(&update).unwrap();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.graph().unwrap(), next.graph);
    assert_eq!(reader.type_use_count("src/cart.ts#interface:Cart").unwrap(), 1);

    let (_fresh_dir, mut fresh) = temp_store();
    fresh.writer.commit(&next.first_compile()).unwrap();
    assert_eq!(dump(&reader), dump(&fresh.readers.get().unwrap()));
}

#[test]
fn removing_a_file_removes_its_typed_records() {
    let (_dir, mut store) = temp_store();
    let first = Prepared::new(typed().build());
    store.writer.commit(&first.first_compile()).unwrap();
    let mut graph = first.graph.clone();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "src/charge.ts");
    let next = Prepared::new(graph);
    store.writer.commit(&next.since(&first)).unwrap();
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.type_use_count("src/cart.ts#interface:Cart").unwrap(), 0);
    assert_eq!(reader.graph().unwrap(), next.graph);
}
