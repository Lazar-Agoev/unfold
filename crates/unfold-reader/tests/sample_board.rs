//! The converted sample board (`testdata/boards/order-placement.json`, one page) against the
//! `ts-orders` golden graph, committed to a temporary store and read back the way the
//! workspace reads it, with sources from `testdata/playgrounds/ts-orders`.

mod common;

use common::{load, playground_root};
use unfold_core::testkit::testdata_dir;
use unfold_reader::board::{self, Board, BoardGraph};

const PAGE: &str = "page-1";

fn sample() -> String {
    std::fs::read_to_string(testdata_dir().join("boards/order-placement.json"))
        .expect("read the sample board")
}

/// The sample board and its graph, loaded for the keys it references.
fn loaded() -> (Board, BoardGraph) {
    let loaded = load("ts-orders");
    let reader = loaded.store.readers.get().expect("get a reader");
    let parsed = board::parse(&sample()).expect("the sample board parses");
    let graph = BoardGraph::load(&reader, &playground_root("ts-orders"), &parsed.references())
        .expect("load the board's graph");
    (parsed, graph)
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
    assert_eq!(page.cards.len(), 6);
    assert_eq!(page.groups.len(), 2);
    assert_eq!(page.labels.len(), 29);
    assert_eq!(page.guards.len(), 35);
    assert!(page.links.is_empty());
}

#[test]
fn the_sample_board_loads_healthy_with_every_description_and_label_attached() {
    let (parsed, graph) = loaded();
    let health = board::board_health(&parsed, &graph);
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
    let (fresh, fresh_health) = board::prepare_save(parsed.clone(), None, &graph).expect("the board saves");
    assert!(fresh_health.is_healthy());
    assert_eq!(board::serialize(&fresh), sample());
}

#[test]
fn a_page_round_trips_its_drawn_links() {
    let text = sample();
    let mut parsed = board::parse(&text).expect("the sample board parses");
    parsed.pages[0].links.push(board::Link {
        label: Some(board::Description {
            text: "replies with the order".to_owned(),
            author: board::Author::person("you").expect("an author"),
            signature: None,
        }),
        ..board::Link::new("place-replies", "place-order", "post-orders")
    });
    let text = board::serialize(&parsed);
    assert_eq!(board::serialize(&board::parse(&text).expect("it parses")), text);
}

#[test]
fn a_proposed_step_goes_only_where_the_card_flow_lets_one_go() {
    const REQUIRE_USER: &str = "src/routes/middleware.ts#function:requireUser";
    let (parsed, graph) = loaded();
    let health = board::board_health(&parsed, &graph);
    let proposal = |id: &str, after: &str, path: Option<board::ProposalPath>| board::Proposal {
        id: id.to_owned(),
        card: "middleware-0".to_owned(),
        after: unfold_core::Key::from_text(&format!("{REQUIRE_USER}/{after}")),
        path,
        title: "Record the refusal".to_owned(),
        reason: None,
        author: board::Author::agent("test").expect("an author"),
    };
    let with = |proposal: board::Proposal| {
        let mut board = parsed.clone();
        board.pages[0].proposals.push(proposal);
        board
    };
    let refused =
        |board: Board, previous: Option<(&Board, &board::Health)>, expected: &str| match board::prepare_save(
            board, previous, &graph,
        ) {
            Err(board::BoardError::Problem { target, message, .. }) => {
                assert_eq!(target, board::Target::Proposal("p".to_owned()), "{message}");
                assert!(message.contains(expected), "{expected:?} in {message:?}");
            }
            Ok(_) => panic!("expected `{expected}`, but it saved"),
            Err(other) => panic!("expected `{expected}`, got {other:?}"),
        };

    // After a step with one way on, or on one of a decision's actual paths.
    let (saved, saved_health) = board::prepare_save(
        with(proposal("p", "if[0]", Some(board::ProposalPath::Yes))),
        Some((&parsed, &health)),
        &graph,
    )
    .expect("a decision's yes path takes a proposal");
    assert!(saved_health.is_healthy(), "{:#?}", saved_health.items);
    let (_, next_health) = board::prepare_save(
        with(proposal("p", "expression_statement[0]", None)),
        Some((&parsed, &health)),
        &graph,
    )
    .expect("a call statement takes a proposal");
    assert!(next_health.is_healthy());

    for (after, path, expected) in [
        ("if[0]/return[0]", None, "nothing runs after it"),
        ("if[0]", None, "has 2 paths: choose yes or no"),
        (
            "if[0]",
            Some(board::ProposalPath::Each),
            "has no each path: choose yes or no",
        ),
        (
            "expression_statement[0]/call[0]",
            None,
            "is not a step of its card's Flow",
        ),
        (
            "expression_statement[0]",
            Some(board::ProposalPath::Yes),
            "has one way on",
        ),
    ] {
        refused(
            with(proposal("p", after, path)),
            Some((&parsed, &health)),
            expected,
        );
    }

    // A path-only edit is a new placement, checked again.
    let mut rerouted = saved.clone();
    rerouted.pages[0].proposals[0].path = Some(board::ProposalPath::Each);
    refused(rerouted, Some((&saved, &saved_health)), "has no each path");
    let mut rerouted = saved.clone();
    rerouted.pages[0].proposals[0].path = Some(board::ProposalPath::No);
    assert!(board::prepare_save(rerouted, Some((&saved, &saved_health)), &graph).is_ok());
}
