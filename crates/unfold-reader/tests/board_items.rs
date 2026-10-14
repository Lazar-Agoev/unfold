//! The example board with every item the format has besides cards, names, and combined steps
//! (`testdata/boards/order-system.json`: nested frames, every kind of resource card, a
//! connection of each of the seven kinds, notes, and a proposed step) against the `ts-orders`
//! golden graph: it round-trips, loads healthy, saves to itself, and repairs without leaving
//! anything dangling.

mod common;

use std::collections::BTreeSet;

use common::{load, playground_root};
use unfold_core::testkit::testdata_dir;
use unfold_reader::board::{self, Board, BoardGraph, ConnectionKind, Repair, ResourceKind, Target, View};

fn example() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-system.json")).expect("read the example board")
}

/// The example board and its graph, loaded for the keys it references.
fn loaded() -> (Board, BoardGraph) {
    let loaded = load("ts-orders");
    let reader = loaded.store.readers.get().expect("get a reader");
    let parsed = board::parse(&example()).expect("the example board parses");
    let graph = BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed.references())
        .expect("load the board's graph");
    (parsed, graph)
}

#[test]
fn the_example_board_round_trips_byte_for_byte() {
    let text = example();
    let parsed = board::parse(&text).expect("the example board parses");
    assert_eq!(board::serialize(&parsed), text);
    let page = &parsed.pages[0];
    assert_eq!(page.default_view, View::Compact);
    assert_eq!(
        (
            page.cards.len(),
            page.frames.len(),
            page.resources.len(),
            page.links.len(),
            page.notes.len(),
            page.proposals.len()
        ),
        (6, 3, 7, 9, 3, 1)
    );
    let kinds: BTreeSet<&str> = page
        .resources
        .iter()
        .map(|resource| resource.kind.as_str())
        .collect();
    let all: BTreeSet<&str> = [
        ResourceKind::Table,
        ResourceKind::Queue,
        ResourceKind::Cache,
        ResourceKind::Storage,
        ResourceKind::External,
        ResourceKind::Actor,
    ]
    .iter()
    .map(|kind| kind.as_str())
    .collect();
    assert_eq!(kinds, all);
    let kinds: BTreeSet<&str> = page.links.iter().map(|link| link.kind.as_str()).collect();
    let seven: BTreeSet<&str> = [
        ConnectionKind::Http,
        ConnectionKind::Queue,
        ConnectionKind::Reads,
        ConnectionKind::Writes,
        ConnectionKind::Calls,
        ConnectionKind::Relates,
        ConnectionKind::Other,
    ]
    .iter()
    .map(|kind| kind.as_str())
    .collect();
    assert_eq!(kinds, seven, "a connection of each kind");
    // The page's items share one id space with its cards: editing it through the window's edit
    // shape reads back the same.
    let edit = serde_json::json!({ "title": parsed.title, "pages": parsed.pages }).to_string();
    assert_eq!(board::parse_edit(&edit, None).expect("the edit parses"), parsed);
}

#[test]
fn the_example_board_loads_healthy_and_saves_to_itself() {
    let (parsed, graph) = loaded();
    let health = board::board_health(&parsed, &graph);
    assert!(health.is_healthy(), "{:#?}", health.items);
    let page = &parsed.pages[0];
    // An entrance at a connection's `to` end gives its channel.
    assert_eq!(
        page.connection("customer-posts")
            .and_then(|link| link.channel.as_deref()),
        Some("POST /orders")
    );
    // Signed claims carry their signatures; a person's connection that cites nothing has none.
    assert!(
        page.connection("enqueue-order")
            .is_some_and(|link| link.signature.is_some())
    );
    assert!(
        page.connection("read-orders")
            .is_some_and(|link| link.signature.is_none())
    );
    let (fresh, fresh_health) = board::prepare_save(parsed, None, &graph).expect("the board saves");
    assert!(fresh_health.is_healthy());
    assert_eq!(board::serialize(&fresh), example());
}

#[test]
fn removing_a_card_leaves_nothing_dangling() {
    let (parsed, graph) = loaded();
    let mut page = parsed.pages[0].clone();
    let remove = |page: &mut board::Page, id: &str| {
        board::apply_repair(page, &graph, &Repair::Remove(Target::Card(id.to_owned())))
            .expect("the repair applies");
    };
    remove(&mut page, "create-order");
    let ids = |items: Vec<&str>| items.into_iter().map(str::to_owned).collect::<Vec<_>>();
    assert!(
        !page
            .links
            .iter()
            .any(|link| link.from == "create-order" || link.to == "create-order")
    );
    assert_eq!(
        page.combined.len(),
        1,
        "a combined step applies page-wide, by key"
    );
    let server = page.frame("server").expect("the server frame stays");
    assert_eq!(
        server.members,
        ids(vec!["post-orders", "place-order", "async-note"])
    );
    // Its note stays where it was, unattached: the card's position plus the note's offset.
    let note = page.note("async-note").expect("the note stays");
    assert_eq!((note.attach.as_deref(), note.at), (None, [0.0, 240.0]));
    // So does the note of a connection that went with it: its ends' midpoint plus its offset.
    let note = page.note("job-note").expect("the note stays");
    assert_eq!((note.attach.as_deref(), note.at), (None, [160.0, 160.0]));

    // A frame whose members are all removed goes too, with the connections to it.
    for card in ["orders-worker", "process-order"] {
        remove(&mut page, card);
    }
    assert!(page.frame("worker").is_none());
    assert!(page.connection("worker-takes").is_none());
    assert_eq!(
        page.frame("ts-orders").expect("the zone stays").members,
        ids(vec!["server", "orders-table", "orders-queue"])
    );
    let mut board = parsed;
    board.pages[0] = page;
    let text = board::serialize(&board);
    assert_eq!(board::serialize(&board::parse(&text).expect("it parses")), text);
}
