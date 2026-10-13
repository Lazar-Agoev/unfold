//! The example board with every item the format has besides cards, groups, and labels
//! (`testdata/boards/order-system.json`: Diagram views, an attributed step group, nested frames,
//! every kind of resource card, connections of every line style, and notes): it round-trips and
//! repairs without leaving anything dangling. Saving it to itself and its health need a live
//! compile: `unfold-workspace`'s `tests/commands/claims.rs` checks them on `ts-orders`.

mod common;

use common::{load, playground_root};
use unfold_core::testkit::testdata_dir;
use unfold_reader::board::{self, Board, BoardGraph, Repair, ResourceKind, View};

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
    assert_eq!(page.default_view, View::Diagram);
    assert_eq!(
        (
            page.cards.len(),
            page.frames.len(),
            page.resources.len(),
            page.links.len(),
            page.notes.len()
        ),
        (6, 3, 7, 10, 3)
    );
    assert!(
        page.groups[0].author.is_some(),
        "the step group's title is attributed"
    );
    let kinds: Vec<ResourceKind> = page.resources.iter().map(|resource| resource.kind).collect();
    for kind in [
        ResourceKind::Table,
        ResourceKind::Queue,
        ResourceKind::Cache,
        ResourceKind::Storage,
        ResourceKind::External,
        ResourceKind::Actor,
    ] {
        assert!(kinds.contains(&kind), "{kind:?}");
    }
    // The page's items share one id space with its cards: editing it through the window's edit
    // shape reads back the same.
    let edit = serde_json::json!({ "title": parsed.title, "pages": parsed.pages }).to_string();
    assert_eq!(board::parse_edit(&edit, None).expect("the edit parses"), parsed);
}

#[test]
fn removing_a_card_leaves_nothing_dangling() {
    let (parsed, graph) = loaded();
    let mut page = parsed.pages[0].clone();
    board::apply_repair(&mut page, &graph, &Repair::RemoveCard("create-order".to_owned()))
        .expect("the repair applies");
    let ids = |items: Vec<&str>| items.into_iter().map(str::to_owned).collect::<Vec<_>>();
    assert!(page.groups.is_empty(), "its step group goes with it");
    assert!(
        !page
            .links
            .iter()
            .any(|link| link.from == "create-order" || link.to == "create-order")
    );
    let server = page.frame("server").expect("the server frame stays");
    assert_eq!(
        server.members,
        ids(vec!["post-orders", "place-order", "async-note"])
    );
    // Its note stays where it was, unattached: the card's position plus the note's offset.
    let note = page.note("async-note").expect("the note stays");
    assert_eq!((note.attach.as_deref(), note.at), (None, [320.0, 240.0]));
    // So does the note of a link that went with it: the link's ends' midpoint plus its offset.
    let note = page.note("job-note").expect("the note stays");
    assert_eq!((note.attach.as_deref(), note.at), (None, [320.0, 160.0]));

    // A frame whose members are all removed goes too, with the links to it.
    for card in ["orders-worker", "process-order"] {
        board::apply_repair(&mut page, &graph, &Repair::RemoveCard(card.to_owned()))
            .expect("the repair applies");
    }
    assert!(page.frame("worker").is_none());
    assert!(page.link("worker-takes").is_none());
    assert_eq!(
        page.frame("ts-orders").expect("the zone stays").members,
        ids(vec!["server", "orders-table", "orders-queue"])
    );
    let mut board = parsed;
    board.pages[0] = page;
    let text = board::serialize(&board);
    assert_eq!(board::serialize(&board::parse(&text).expect("it parses")), text);
}
