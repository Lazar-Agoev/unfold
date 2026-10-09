//! The converted sample board (`testdata/boards/order-placement.json`) against the `ts-orders`
//! reference graph, committed to a temporary store and read back the way the workspace reads it,
//! with sources from `testdata/playgrounds/ts-orders`.

mod common;

use common::{load, playground_root};
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_reader::board::{self, BoardGraph, OutlineRow};
use unfold_reader::mermaid;

fn sample() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-placement.json"))
        .expect("read the sample board")
}

#[test]
fn the_sample_board_round_trips_byte_for_byte() {
    let text = sample();
    let parsed = board::parse(&text).expect("the sample board parses");
    assert_eq!(board::serialize(&parsed), text);
    assert_eq!(parsed.cards.len(), 6);
    assert_eq!(parsed.groups.len(), 2);
    assert_eq!(parsed.labels.len(), 29);
    assert_eq!(parsed.guards.len(), 35);
}

#[test]
fn the_sample_board_loads_healthy_with_every_description_and_label_attached() {
    let loaded = load("playground-ts-native");
    let reader = loaded.store.readers.get().expect("get a reader");
    let text = sample();
    let parsed = board::parse(&text).expect("the sample board parses");
    let graph =
        BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed).expect("load the board's graph");

    board::check(&parsed, &graph).expect("every reference exists and every group is valid");
    let health = board::health(&parsed, &graph);
    assert!(health.is_healthy(), "{:#?}", health.items);

    // Every signature (3 card descriptions, 2 group descriptions, 29 labels, 35 guards) matches
    // the verified source, so every text is attached and current.
    for card in &parsed.cards {
        if let Some(description) = &card.description {
            assert_eq!(
                description.signature,
                graph.signature(card.reference.key().as_str()),
                "{}",
                card.id
            );
        }
    }
    for group in &parsed.groups {
        let (first, last) = (&group.members[0], &group.members[group.members.len() - 1]);
        let signature = graph.span_signature(first.as_str(), last.as_str());
        assert_eq!(
            group.description.as_ref().and_then(|item| item.signature),
            signature,
            "{}",
            group.id
        );
    }
    for (key, label) in &parsed.labels {
        assert_eq!(label.signature, graph.signature(key), "{key}");
    }
    for (key, guard) in &parsed.guards {
        assert_eq!(Some(guard), board::current_guard(&graph, key).as_ref(), "{key}");
    }

    // Saving it again changes nothing.
    let (saved, saved_health) =
        board::prepare_save(parsed.clone(), Some((&parsed, &health)), &graph).expect("the board saves");
    assert!(saved_health.is_healthy());
    assert_eq!(board::serialize(&saved), text);

    // The outline follows the calls from the entrance.
    let outline = board::outline(&parsed, &graph);
    let rows: Vec<(&str, u32)> = outline
        .rows
        .iter()
        .map(|OutlineRow { card, depth }| (card.as_str(), *depth))
        .collect();
    assert_eq!(
        rows,
        [
            ("post-orders", 0),
            ("middleware-0", 1),
            ("middleware-1", 1),
            ("validate-order", 2),
            ("create-order", 1),
            ("place-order", 2),
        ]
    );
    let labeled = outline
        .unlabeled
        .iter()
        .filter(|node| parsed.labels.contains_key(node.key.as_str()))
        .count();
    assert_eq!(labeled, 0, "labeled nodes never count as unlabeled");
}

#[test]
fn the_sample_board_exports_to_mermaid() {
    let loaded = load("playground-ts-native");
    let reader = loaded.store.readers.get().expect("get a reader");
    let parsed = board::parse(&sample()).expect("the sample board parses");
    let graph =
        BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed).expect("load the board's graph");
    let output = mermaid::board(&parsed, &graph);
    assert_eq!(mermaid::board(&parsed, &graph), output, "deterministic");
    assert_golden("boards/order-placement.mmd", &output);

    // Every card is a Code card, so the PoC's diagram has no source labels: it must match
    // exactly, apart from handler numbers (no step numbers in any view).
    let poc = std::fs::read_to_string(
        testdata_dir().join("reference/poc-reader/ts-orders/board-order-placement.mmd"),
    )
    .expect("read the PoC's diagram");
    let mut poc = poc;
    for number in 1..=3 {
        poc = poc.replace(&format!("\"handler {number}\""), "\"handler\"");
    }
    assert_eq!(output, poc);

    // With Flow cards, the board's labels replace the fallback names.
    let mut flowing = parsed;
    for card in &mut flowing.cards {
        card.view = Some(board::View::Flow);
    }
    let output = mermaid::board(&flowing, &graph);
    assert!(output.contains("Is the input not an object?"));
    assert!(output.contains("subgraph c0_g0[\"Queue async placement"));
    assert_golden("boards/order-placement-flow.mmd", &output);
}
