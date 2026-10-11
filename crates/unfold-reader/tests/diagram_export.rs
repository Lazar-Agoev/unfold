//! Mermaid export of Diagram-view cards: a Diagram node draws one arrow to each card it calls,
//! as the app does. (Type declarations in Flow view are checked with a compiled project in
//! `unfold-workspace`'s board command tests: no reference corpus has them.)

mod common;

use common::{load, playground_root};
use unfold_reader::board::{self, BoardGraph, PageGraphs};
use unfold_reader::mermaid;

const SCAN: &str = "src/orders_playground/features/control.py#function:scan";
const NORMALIZE: &str = "src/orders_playground/features/control.py#function:normalize";

/// A one-page board of `cards` (`[id, key, view]`) at made-up places.
fn board_of(cards: &[[&str; 3]]) -> board::Board {
    let cards: Vec<_> = cards
        .iter()
        .enumerate()
        .map(|(index, [id, key, view])| {
            serde_json::json!({ "id": id, "ref": { "block": key }, "at": [index * 600, 0], "view": view })
        })
        .collect();
    let text = serde_json::json!({
        "format": "unfold-board",
        "version": 1,
        "title": "Views",
        "projects": [],
        "pages": [{ "id": "page-1", "title": "Views", "defaultView": "code", "cards": cards, "groups": [] }],
    });
    board::parse(&text.to_string()).expect("the board parses")
}

/// The page's Mermaid against `corpus`, with sources from `playground`.
fn export(corpus: &str, playground: &str, cards: &[[&str; 3]]) -> String {
    let loaded = load(corpus);
    let graph = BoardGraph::from_graph(&loaded.graph, &playground_root(playground));
    let board = board_of(cards);
    mermaid::page(&board, "page-1", &PageGraphs::own_only(&graph)).expect("the page exists")
}

/// The export's arrows between cards.
fn arrows(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("c0") && line.contains("-->"))
        .collect()
}

#[test]
fn a_diagram_node_draws_one_arrow_to_each_card_it_calls() {
    let code = export(
        "python-playground",
        "py-orders",
        &[["scan", SCAN, "code"], ["normalize", NORMALIZE, "diagram"]],
    );
    assert_eq!(arrows(&code).len(), 4, "{code}");
    let diagram = export(
        "python-playground",
        "py-orders",
        &[["scan", SCAN, "diagram"], ["normalize", NORMALIZE, "diagram"]],
    );
    assert_eq!(arrows(&diagram), ["c0 -->|\"4 calls\"| c1"], "{diagram}");
}
