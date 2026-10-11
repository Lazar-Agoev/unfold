//! The example board with every item the format has besides cards, groups, and labels
//! (`testdata/boards/order-system.json`: Diagram views, an attributed step group, nested frames,
//! every kind of resource card, connections of every line style, and notes) against the
//! `ts-orders` reference graph: it round-trips, saves to itself, is healthy, outlines through
//! its cards only, counts its changes, repairs without leaving anything dangling, and exports to
//! Mermaid.

mod common;

use common::{load, playground_root};
use unfold_core::testkit::{assert_golden, testdata_dir};
use unfold_reader::board::{self, Board, BoardGraph, OutlineRow, PageGraphs, Repair, ResourceKind, View};
use unfold_reader::mermaid;

const PAGE: &str = "page-1";

fn example() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-system.json")).expect("read the example board")
}

/// The example board and its graph, loaded for the keys it references.
fn loaded() -> (Board, BoardGraph) {
    let loaded = load("playground-ts-native");
    let reader = loaded.store.readers.get().expect("get a reader");
    let parsed = board::parse(&example()).expect("the example board parses");
    let graph = BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed.references(None))
        .expect("load the board's graph");
    (parsed, graph)
}

#[test]
fn the_example_board_round_trips_byte_for_byte() {
    let text = example();
    let parsed = board::parse(&text).expect("the example board parses");
    assert_eq!(board::serialize(&parsed), text);
    assert!(parsed.projects.is_empty());
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
fn the_example_board_saves_to_itself_and_is_healthy() {
    let (parsed, graph) = loaded();
    let graphs = PageGraphs::own_only(&graph);
    let health = board::board_health(&parsed, &graphs);
    assert!(health.is_healthy(), "{:#?}", health.items);

    // A fresh save signs and guards the cards and carries every other item through unchanged.
    let (saved, saved_health) = board::prepare_save(parsed.clone(), None, &graphs).expect("the board saves");
    assert!(saved_health.is_healthy());
    assert_golden("boards/order-system.json", &board::serialize(&saved));
    assert_eq!(board::changes(&saved, &parsed), 0);

    // The outline follows calls, then links between two cards (the runtime call to
    // `StripeProvider.charge` included); links to resources and frames aren't followed.
    let outline = board::outline(&saved.pages[0], &graphs);
    let rows: Vec<(&str, u32)> = outline
        .rows
        .iter()
        .map(|OutlineRow { card, depth }| (card.as_str(), *depth))
        .collect();
    assert_eq!(
        rows,
        [
            ("post-orders", 0),
            ("create-order", 1),
            ("place-order", 2),
            ("stripe-charge", 3),
            ("orders-worker", 0),
            ("process-order", 1),
        ]
    );
}

#[test]
fn every_item_counts_as_one_unpublished_change() {
    let published = board::parse(&example()).expect("the example board parses");
    let mut working = published.clone();
    let page = &mut working.pages[0];
    page.notes.retain(|note| note.id != "reading-order");
    page.frames[0].title = "API".to_owned();
    page.resources[0].at = [0.0, 0.0];
    page.links
        .iter_mut()
        .filter(|link| link.id == "enqueue-order")
        .for_each(|link| link.channel = Some("orders-v2".to_owned()));
    page.groups[0].author = None;
    assert_eq!(board::changes(&working, &published), 5);
}

#[test]
fn removing_a_card_leaves_nothing_dangling() {
    let (parsed, graph) = loaded();
    let graphs = PageGraphs::own_only(&graph);
    let mut page = parsed.pages[0].clone();
    board::apply_repair(&mut page, &graphs, &Repair::RemoveCard("create-order".to_owned()))
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
        board::apply_repair(&mut page, &graphs, &Repair::RemoveCard(card.to_owned()))
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

#[test]
fn the_example_board_exports_to_mermaid() {
    let (parsed, graph) = loaded();
    let graphs = PageGraphs::own_only(&graph);
    let output = mermaid::page(&parsed, PAGE, &graphs).expect("the page exists");
    assert_eq!(
        mermaid::page(&parsed, PAGE, &graphs),
        Some(output.clone()),
        "deterministic"
    );
    for expected in [
        // Frames are nested subgraphs with their direction, around their members.
        "  subgraph f1[\"ts-orders\"]\n    direction LR\n    subgraph f0[\"server<br/>",
        "      direction TB\n      c3([\"POST /orders\"])\n",
        "    subgraph f2[\"worker\"]\n      c1([\"worker orders\"])\n",
        // A Diagram-view card is a box, like a Code-view card.
        "      c2[\"OrderService.place · src/services/order-service.ts:13\"]\n",
        // Resource cards take their kind's shape.
        "  r0(((\"actor · Customer\")))\n",
        "  r1[(\"table · customers<br/>customerId: string · PK<br/>email: string · unique<br/>items: OrderItem#91;#93; · optional\")]\n",
        "    r3[(\"table · orders<br/>id · PK<br/>customerId · FK → customers.customerId<br/>Orders by id",
        "    r2[/\"queue · orders\"/]\n",
        "  r4[\\\"cache · Redis\"\\]\n",
        "  r5[/\"storage · Orders report\"\\]\n",
        "  r6{{\"external · slack\"}}\n",
        // Connections take their kind's line and words; `reads` points back at the code, and a
        // connection with a step leaves from the Flow node that shows it.
        "  r0 -->|\"HTTP · POST /orders\"| c3\n",
        "  c0_n3 ==>|\"queue · orders · place-order job\"| r2\n",
        "  r2 ==>|\"queue · orders\"| f2\n",
        "  c2 -->|\"writes\"| r3\n",
        "  r3 -->|\"reads\"| c4\n",
        "  r3 ---|\"relates · many to one · customerId → customerId\"| r1\n",
        "  c2 -.->|\"runtime call\"| c5\n",
        "  c0_n7 -.->|\"posts to #35;orders\"| r6\n",
        "  r2 -.->|\"kept in\"| r4\n",
        // Notes are comments where they sit.
        "      %% Note on create-order: Queued orders are placed by the worker.",
        "  %% Note: Read left to right: the request, then the worker.\n",
    ] {
        assert!(output.contains(expected), "{expected}\n{output}");
    }
    assert_golden("boards/order-system.mmd", &output);
}
