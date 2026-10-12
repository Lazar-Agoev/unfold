//! The converted sample board (`testdata/boards/order-placement.json`, one page) against the
//! `ts-orders` reference graph, committed to a temporary store and read back the way the
//! workspace reads it, with sources from `testdata/playgrounds/ts-orders`.

mod common;

use common::{load, playground_root};
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_reader::board::{self, Board, BoardGraph, OutlineRow, PageGraphs};
use unfold_reader::mermaid;

const PAGE: &str = "page-1";

fn sample() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-placement.json"))
        .expect("read the sample board")
}

/// The sample board and its graph, loaded for the keys it references.
fn loaded() -> (Board, BoardGraph) {
    let loaded = load("playground-ts-native");
    let reader = loaded.store.readers.get().expect("get a reader");
    let parsed = board::parse(&sample()).expect("the sample board parses");
    let graph = BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed.references(None))
        .expect("load the board's graph");
    (parsed, graph)
}

#[test]
fn the_sample_board_round_trips_byte_for_byte() {
    let text = sample();
    let parsed = board::parse(&text).expect("the sample board parses");
    assert_eq!(board::serialize(&parsed), text);
    assert!(parsed.projects.is_empty());
    let [page] = parsed.pages.as_slice() else {
        panic!("one page");
    };
    assert_eq!(page.id, PAGE);
    assert_eq!(page.cards.len(), 6);
    assert_eq!(page.groups.len(), 2);
    assert_eq!(page.labels.len(), 29);
    assert_eq!(page.guards.len(), 35);
    assert!(page.links.is_empty());
}

#[test]
fn the_sample_board_loads_healthy_with_every_description_and_label_attached() {
    let (parsed, graph) = loaded();
    let graphs = PageGraphs::own_only(&graph);
    let health = board::board_health(&parsed, &graphs);
    assert!(health.is_healthy(), "{:#?}", health.items);
    assert_eq!(health.label(), None);

    // Every signature (3 card descriptions, 2 group descriptions, 29 labels, 35 guards) matches
    // the verified source, so every text is attached and current.
    let page = &parsed.pages[0];
    for card in &page.cards {
        if let Some(description) = &card.description {
            assert_eq!(
                description.signature,
                graph.signature(card.reference.key().as_str()),
                "{}",
                card.id
            );
        }
    }
    for group in &page.groups {
        let (first, last) = (&group.members[0], &group.members[group.members.len() - 1]);
        let signature = graph.span_signature(first.as_str(), last.as_str());
        assert_eq!(
            group.description.as_ref().and_then(|item| item.signature),
            signature,
            "{}",
            group.id
        );
    }
    for entry in &page.labels {
        assert_eq!(
            entry.label.signature,
            graph.signature(entry.key.as_str()),
            "{}",
            entry.key
        );
    }
    for guard in &page.guards {
        assert_eq!(
            Some(guard),
            board::current_guard(&graph, None, guard.key.as_str()).as_ref(),
            "{}",
            guard.key
        );
    }

    // Saving it again changes nothing, whether the page is reused or checked again.
    let (saved, saved_health) =
        board::prepare_save(parsed.clone(), Some((&parsed, &health)), &graphs).expect("the board saves");
    assert!(saved_health.is_healthy());
    assert_eq!(board::serialize(&saved), sample());
    let (fresh, fresh_health) = board::prepare_save(parsed.clone(), None, &graphs).expect("the board saves");
    assert!(fresh_health.is_healthy());
    assert_eq!(board::serialize(&fresh), sample());
    assert_eq!(board::changes(&fresh, &parsed), 0);

    // The outline follows the calls from the entrance.
    let outline = board::outline(page, &graphs);
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
        .filter(|node| page.label_entry(None, node.key.as_str()).is_some())
        .count();
    assert_eq!(labeled, 0, "labeled nodes never count as unlabeled");
}

#[test]
fn the_sample_board_exports_to_mermaid() {
    let (parsed, graph) = loaded();
    let graphs = PageGraphs::own_only(&graph);
    let output = mermaid::page(&parsed, PAGE, &graphs).expect("the page exists");
    assert_eq!(
        mermaid::page(&parsed, PAGE, &graphs),
        Some(output.clone()),
        "deterministic"
    );
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

    // With Flow cards, the page's labels replace the fallback names.
    let mut flowing = parsed;
    for card in &mut flowing.pages[0].cards {
        card.view = Some(board::View::Flow);
    }
    let output = mermaid::page(&flowing, PAGE, &graphs).expect("the page exists");
    assert!(output.contains("Is the input not an object?"));
    assert!(output.contains("subgraph c0_g0[\"Queue async placement"));
    assert_golden("boards/order-placement-flow.mmd", &output);
}

#[test]
fn a_page_spanning_projects_exports_placeholders_and_drawn_links() {
    let (mut parsed, graph) = loaded();
    let web = board::ProjectName::parse("storefront-web").expect("a shared name");
    let page = &mut parsed.pages[0];
    page.cards.push(board::Card {
        id: "checkout".to_owned(),
        reference: board::CardRef::entrance(
            unfold_core::Key::from_text("entrance:[\"next.route\",\"http\",\"POST\",\"/api/cart/checkout\"]"),
            Some(web.clone()),
        ),
        at: [-704.0, -2.0],
        view: None,
        folded: false,
        description: None,
        expanded: Vec::new(),
    });
    page.links.push(board::Link {
        label: Some(board::Description {
            text: "calls POST /orders".to_owned(),
            author: board::Author::person("you").expect("an author"),
            signature: None,
        }),
        ..board::Link::new("checkout-calls-orders", "checkout", "post-orders")
    });
    parsed.refresh_projects();
    assert_eq!(parsed.projects, [web]);
    let graphs = PageGraphs::own_only(&graph);
    let output = mermaid::page(&parsed, PAGE, &graphs).expect("the page exists");
    assert!(output.contains("c6[\"From storefront-web, not connected\"]"));
    assert!(output.contains("c6 -.->|\"calls POST /orders\"| c4"));
    assert_golden("boards/order-placement-links.mmd", &output);

    // The page still round-trips with its link and its other project.
    let text = board::serialize(&parsed);
    assert_eq!(board::serialize(&board::parse(&text).expect("it parses")), text);
}
