//! The sample board (`testdata/boards/order-placement.json`, one page: an entrance, five
//! definitions, names, and combined steps) against the `ts-orders` golden graph, committed to a
//! temporary store and read back the way the workspace reads it, with sources from
//! `testdata/playgrounds/ts-orders`.

mod common;

use common::{load, playground_root};
use unfold_core::Key;
use unfold_core::testkit::testdata_dir;
use unfold_reader::board::{
    self, Author, Board, BoardError, BoardGraph, CardRef, Combined, HealthKind, Place, Proposal, Target,
};

const PAGE: &str = "page-1";
const REQUIRE_USER: &str = "src/routes/middleware.ts#requireUser";
const CREATE_ORDER: &str = "src/routes/orders.ts#createOrder";
const PLACE_AN_ORDER: &str = "entrance:server/place-an-order";

fn sample() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-placement.json"))
        .expect("read the sample board")
}

/// The graph `board` reads: what it references, from the golden graph's store.
fn graph_for(board: &Board) -> BoardGraph {
    let loaded = load("ts-orders");
    let reader = loaded.store.readers.get().expect("get a reader");
    BoardGraph::load(&reader, &playground_root("ts-orders"), &board.references())
        .expect("load the board's graph")
}

/// The sample board and its graph.
fn loaded() -> (Board, BoardGraph) {
    let parsed = board::parse(&sample()).expect("the sample board parses");
    let graph = graph_for(&parsed);
    (parsed, graph)
}

fn key(text: &str) -> Key {
    Key::from_text(text)
}

#[test]
fn the_sample_board_round_trips_byte_for_byte() {
    let text = sample();
    let parsed = board::parse(&text).expect("the sample board parses");
    assert_eq!(board::serialize(&parsed), text);
    let [page] = parsed.pages.as_slice() else {
        panic!("one page");
    };
    assert_eq!(page.id, PAGE);
    assert_eq!(
        (
            page.cards.len(),
            page.names.len(),
            page.combined.len(),
            page.guards.len()
        ),
        (6, 18, 3, 27)
    );
    assert!(page.links.is_empty());
    assert_eq!(
        page.card("create-order").expect("the card").open,
        [vec![key(
            "src/routes/orders.ts#createOrder/call:orderService.place"
        )]]
    );
}

#[test]
fn the_sample_board_loads_healthy_with_everything_signed_over_its_code() {
    let (parsed, graph) = loaded();
    let health = board::board_health(&parsed, &graph);
    assert!(health.is_healthy(), "{:#?}", health.items);
    assert_eq!(health.label(), None);

    // Every signature matches the verified source: card descriptions (the entrance's over the
    // source of its code), names, and combined steps with their descriptions.
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
    let entrance = graph.entrance(PLACE_AN_ORDER).expect("the entrance");
    let code: Vec<&str> = entrance
        .targets()
        .iter()
        .map(|target| target.code.as_str())
        .collect();
    assert_eq!(
        code,
        [
            REQUIRE_USER,
            "src/routes/middleware.ts#validateBody",
            CREATE_ORDER
        ]
    );
    for entry in &page.names {
        assert_eq!(
            entry.name.signature,
            graph.signature(entry.key.as_str()),
            "{}",
            entry.key
        );
    }
    for combined in &page.combined {
        let (first, last) = (
            &combined.members[0],
            &combined.members[combined.members.len() - 1],
        );
        let span = graph.span_signature(first.as_str(), last.as_str());
        assert_eq!(combined.signature, span, "{}", combined.id);
        if let Some(description) = &combined.description {
            assert_eq!(description.signature, span, "{}", combined.id);
        }
    }
    for guard in &page.guards {
        assert_eq!(
            Some(guard),
            board::current_guard(&graph, guard.key.as_str()).as_ref(),
            "{}",
            guard.key
        );
    }

    // Saving it again changes nothing, whether the page is reused or checked again.
    let (saved, saved_health) =
        board::prepare_save(parsed.clone(), Some((&parsed, &health)), &graph).expect("the board saves");
    assert!(saved_health.is_healthy());
    assert_eq!(board::serialize(&saved), sample());
    let (fresh, fresh_health) = board::prepare_save(parsed, None, &graph).expect("the board saves");
    assert!(fresh_health.is_healthy());
    assert_eq!(board::serialize(&fresh), sample());
}

/// The problem a save of `board` is refused with.
fn refused(board: Board, graph: &BoardGraph) -> (Target, String) {
    match board::prepare_save(board, None, graph) {
        Err(BoardError::Problem { target, message, .. }) => (target, message),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_overlay_is_checked_on_save_against_real_code() {
    let (parsed, graph) = loaded();
    let with = |edit: &dyn Fn(&mut board::Page)| {
        let mut board = parsed.clone();
        edit(&mut board.pages[0]);
        board
    };
    let proposal = |place: Place| Proposal {
        id: "p".to_owned(),
        place,
        title: "Record the refusal".to_owned(),
        reason: None,
        author: Author::agent("test").expect("an author"),
    };
    let after = |step: &str| Place::After {
        after: key(&format!("{REQUIRE_USER}/{step}")),
    };
    let start = |block: &str| Place::Start {
        start: key(&format!("{REQUIRE_USER}{block}")),
    };

    // A proposed step goes after a block, or at the start of a definition's or a part's body.
    for place in [
        after("if[0]"),
        after("call:next"),
        after("if[0]/then/call:res.status().json"),
        start(""),
        start("/if[0]/then"),
    ] {
        let (_, health) = board::prepare_save(
            with(&|page| page.proposals.push(proposal(place.clone()))),
            None,
            &graph,
        )
        .unwrap_or_else(|error| panic!("{place:?}: {error}"));
        assert!(health.is_healthy());
    }
    for (place, expected) in [
        (after("if[0]/then"), "can't go after"),
        (
            after("if[0]/then/call:res.status().json/call:res.status"),
            "can't go after",
        ),
        (start("/call:next"), "can't go at the start of"),
    ] {
        let (target, message) = refused(with(&|page| page.proposals.push(proposal(place.clone()))), &graph);
        assert_eq!(target, Target::Proposal("p".to_owned()));
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    }

    // Combined steps are consecutive blocks of one body and never nest.
    let combined = |id: &str, members: &[&str]| Combined {
        id: id.to_owned(),
        title: "Steps".to_owned(),
        members: members
            .iter()
            .map(|member| key(&format!("{CREATE_ORDER}/{member}")))
            .collect(),
        collapsed: true,
        description: None,
        author: Author::agent("test").expect("an author"),
        signature: None,
    };
    for (members, alone, expected) in [
        (
            &["if[0]", "call:slack.post"][..],
            true,
            "doesn't come right after",
        ),
        (&["if[0]/then/call:ordersQueue.add"][..], false, "never nest"),
        (
            &["call:res.status().json/call:res.status"][..],
            false,
            "can't be combined",
        ),
    ] {
        let edit = |page: &mut board::Page| {
            if alone {
                page.combined.clear();
            }
            page.combined.push(combined("new", members));
        };
        let (target, message) = refused(with(&edit), &graph);
        assert!(matches!(target, Target::Combined(_)), "{target:?}");
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    }
    // Never overlapping: one block in two combined steps fails like an invalid file.
    let overlapping = with(&|page| page.combined.push(combined("new", &["call:res.status().json"])));
    assert!(matches!(
        board::prepare_save(overlapping, None, &graph),
        Err(BoardError::Invalid { .. })
    ));

    // A card opens project calls and callbacks, each inside what the one before it opens.
    let open = |paths: &[&[&str]]| {
        with(&|page| {
            let card = page
                .cards
                .iter_mut()
                .find(|card| card.id == "create-order")
                .expect("the card");
            card.open = paths
                .iter()
                .map(|path| path.iter().map(|step| key(step)).collect())
                .collect();
        })
    };
    let slack = "src/routes/orders.ts#createOrder/call:slack.post";
    let place = "src/routes/orders.ts#createOrder/call:orderService.place";
    let validate = "src/services/order-service.ts#OrderService.place/call:validateOrder";
    let opened = open(&[
        &[slack, "src/lib/slack.ts#slack.post/call:logger.info"],
        &[place, validate],
    ]);
    let (_, health) =
        board::prepare_save(opened.clone(), None, &graph_for(&opened)).expect("project calls open in place");
    assert!(health.is_healthy());
    for (path, expected) in [
        (
            &["src/routes/orders.ts#createOrder/call:res.status().json"][..],
            "can't open in place",
        ),
        (&[validate][..], "is not drawn in"),
    ] {
        let opened = open(&[path]);
        let (target, message) = refused(opened.clone(), &graph_for(&opened));
        assert_eq!(target, Target::Card("create-order".to_owned()));
        assert!(message.contains(expected), "{expected:?} in {message:?}");
    }
}

#[test]
fn a_renamed_entrance_is_offered_by_its_code() {
    let (parsed, graph) = loaded();
    let rename = |board: &mut Board, from: &str, to: &str| {
        let page = &mut board.pages[0];
        if let Some(card) = page
            .cards
            .iter_mut()
            .find(|card| card.reference.key().as_str() == from)
        {
            card.reference = CardRef::entrance(key(to));
        }
        page.guards
            .iter_mut()
            .find(|guard| guard.key.as_str() == from)
            .expect("the entrance is guarded")
            .key = key(to);
    };
    // "Place an order" was called "Place order" when the board was saved.
    let mut board = parsed.clone();
    rename(&mut board, PLACE_AN_ORDER, "entrance:server/place-order");
    let health = board::board_health(&board, &graph);
    let [item] = health.items.as_slice() else {
        panic!("one item: {:#?}", health.items);
    };
    assert_eq!(
        (item.kind, &item.target, item.reconnect.as_deref()),
        (
            HealthKind::Missing,
            &Target::Card("post-orders".to_owned()),
            Some(PLACE_AN_ORDER)
        )
    );

    // Two entrances share `retryFailed` as their code: neither is offered.
    let mut board = parsed;
    board.pages[0].cards.push(board::Card {
        id: "retry".to_owned(),
        reference: CardRef::entrance(key("entrance:orders-cli/retry-failed-orders")),
        at: [0.0, 0.0],
        view: None,
        description: None,
        open: Vec::new(),
    });
    let (mut saved, _) = board::prepare_save(board, None, &graph).expect("the board saves");
    rename(
        &mut saved,
        "entrance:orders-cli/retry-failed-orders",
        "entrance:orders-cli/retry",
    );
    let health = board::board_health(&saved, &graph);
    assert_eq!(health.items.len(), 1);
    assert_eq!(health.items[0].reconnect, None);
}
