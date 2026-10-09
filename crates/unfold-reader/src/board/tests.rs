//! Board tests: strict parsing, deterministic serialization, signatures, guards, groups,
//! health and repairs, saving, and board files, on a small hand-built graph whose source is
//! written to a temporary folder.

use std::path::Path;

use unfold_core::testkit::{GraphBuilder, Outcome};
use unfold_core::vocab::{BlockKind, EdgeKind, Language, Role, SlotName};
use unfold_core::{ByteRange, Graph, Key};

use super::files::{self, BoardFile, Change, Place, Revision};
use super::*;

const SOURCE: &str = "export function run() {\n  const first = 1;\n  if (first) { console.log(first); }\n  console.log('done');\n  return first;\n}\n";

const RUN: &str = "main.ts#function:run";
const FIRST: &str = "main.ts#function:run/variable:first";
const IF0: &str = "main.ts#function:run/if[0]";
const LOG0: &str = "main.ts#function:run/expression_statement[0]";
const RETURN0: &str = "main.ts#function:run/return[0]";

fn range(start: usize, end: usize) -> ByteRange {
    ByteRange {
        start: u32::try_from(start).unwrap(),
        end: u32::try_from(end).unwrap(),
    }
}

fn console_log(
    builder: &mut GraphBuilder,
    parent: unfold_core::testkit::BlockRef,
    role: Role,
    at: usize,
    text: &str,
) {
    let statement_text = &text[at..at + text[at..].find(';').unwrap() + 1];
    let statement = builder.block(
        parent,
        BlockKind::ExpressionStatement,
        Some(role),
        None,
        range(at, at + statement_text.len()),
    );
    let call = range(at, at + statement_text.len() - 1);
    builder.slot(statement, SlotName::Expression, None, call);
    let block = builder.block(statement, BlockKind::Call, Some(Role::Expression), None, call);
    builder.slot(block, SlotName::Callee, None, range(at, at + "console.log".len()));
    builder.edge(
        block,
        EdgeKind::Calls,
        Outcome::external("global"),
        "console.log",
        None,
    );
}

/// The graph of one function written as `text`: one statement per line, each a `const`, an
/// `if (…) { console.log(…); }`, a `console.log(…);`, or a `return`.
fn graph_of(text: &str) -> Graph {
    let mut builder = GraphBuilder::new();
    let root = builder.file("main.ts", Language::TypeScript, text);
    let start = text.find("function ").unwrap();
    let name = &text[start + "function ".len()..text.find('(').unwrap()];
    let end = text.rfind('}').unwrap() + 1;
    let function = builder.block(
        root,
        BlockKind::Function,
        Some(Role::Body),
        Some(name),
        range(start, end),
    );
    let mut offset = text.find('\n').unwrap() + 1;
    for line in text[offset..].lines() {
        let indent = line.len() - line.trim_start().len();
        let at = offset + indent;
        let statement = line.trim();
        if let Some(rest) = statement.strip_prefix("const ") {
            let name = &rest[..rest.find(' ').unwrap()];
            let block = builder.block(
                function,
                BlockKind::Variable,
                Some(Role::Body),
                Some(name),
                range(at, at + statement.len() - 1),
            );
            let initializer = at + statement.find("= ").unwrap() + 2;
            builder.slot(
                block,
                SlotName::Initializer,
                None,
                range(initializer, at + statement.len() - 1),
            );
        } else if statement.starts_with("if (") {
            let block = builder.block(
                function,
                BlockKind::If,
                Some(Role::Body),
                None,
                range(at, at + statement.len()),
            );
            let close = statement.find(')').unwrap();
            builder.slot(block, SlotName::Condition, None, range(at + 4, at + close));
            let then = at + statement.find('{').unwrap();
            builder.slot(block, SlotName::Then, None, range(then, at + statement.len()));
            console_log(&mut builder, block, Role::Then, then + 2, text);
        } else if statement.starts_with("console.log") {
            console_log(&mut builder, function, Role::Body, at, text);
        } else if statement.starts_with("return ") {
            let block = builder.block(
                function,
                BlockKind::Return,
                Some(Role::Body),
                None,
                range(at, at + statement.len()),
            );
            builder.slot(
                block,
                SlotName::Expression,
                None,
                range(at + 7, at + statement.len() - 1),
            );
        }
        offset += line.len() + 1;
    }
    builder.build()
}

/// A folder holding `text` as `main.ts`, and the board graph of it.
fn folder(text: &str) -> (tempfile::TempDir, BoardGraph) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.ts"), text).unwrap();
    let graph = BoardGraph::from_graph(&graph_of(text), dir.path());
    (dir, graph)
}

/// The same folder after an edit and a compile.
fn recompiled(dir: &Path, text: &str) -> BoardGraph {
    std::fs::write(dir.join("main.ts"), text).unwrap();
    BoardGraph::from_graph(&graph_of(text), dir)
}

fn author() -> Author {
    Author::agent("test").unwrap()
}

fn sample_board() -> Board {
    let mut board = Board::new("Run explanation");
    board.default_view = View::Flow;
    board.cards.push(Card {
        id: "run".to_owned(),
        reference: CardRef::Block(Key::from_text(RUN)),
        at: [10.0, 20.0],
        view: None,
        folded: false,
        description: None,
    });
    board
}

fn group(members: &[&str]) -> Group {
    Group {
        id: "steps".to_owned(),
        card: "run".to_owned(),
        title: "Steps".to_owned(),
        members: members.iter().map(|key| Key::from_text(key)).collect(),
        collapsed: false,
        description: None,
    }
}

fn invalid(text: &str) -> (String, String) {
    match parse(text) {
        Err(BoardError::Invalid { path, message }) => (path, message),
        other => panic!("expected an invalid board, got {other:?}"),
    }
}

const MINIMAL: &str = r#"{"cards":[{"at":[0,0],"id":"run","ref":{"block":"main.ts#function:run"}}],"defaultView":"code","format":"unfold-board","groups":[],"title":"T","version":1}"#;

#[test]
fn parsing_rejects_everything_outside_the_format_with_a_path() {
    assert!(parse(MINIMAL).is_ok());
    let cases = [
        (MINIMAL.replace("\"title\"", "\"extra\":1,\"title\""), "/extra", "unknown field"),
        (MINIMAL.replace("\"id\":\"run\"", "\"id\":\"run\",\"state\":\"code\""), "/cards/0/state", "unknown field"),
        (
            MINIMAL.replace(r#"{"block":"main.ts#function:run"}"#, r#"{"block":"main.ts#function:run","entrance":"entrance:x"}"#),
            "/cards/0/ref",
            "exactly one",
        ),
        (MINIMAL.replace(r#"{"block":"main.ts#function:run"}"#, r#"{"block":"entrance:x"}"#), "/cards/0/ref/block", "block key"),
        (MINIMAL.replace("[0,0]", "[0]"), "/cards/0/at", "two numbers"),
        (MINIMAL.replace("[0,0]", "[0,\"1\"]"), "/cards/0/at/1", "finite number"),
        (MINIMAL.replace("\"version\":1", "\"version\":3"), "/version", "unsupported version"),
        (MINIMAL.replace("unfold-board", "codegraph-board"), "/format", "unfold-board"),
        (MINIMAL.replace("\"code\"", "\"canvas\""), "/defaultView", "\"code\" or \"flow\""),
        (MINIMAL.replace("\"T\"", "\"  \""), "/title", "must not be empty"),
        (MINIMAL.replace(",\"groups\":[]", ""), "/", "missing field `groups`"),
        (
            MINIMAL.replace("\"id\":\"run\"", r#""id":"run","description":{"author":"you","text":"x"}"#),
            "/cards/0/description/author",
            "person:<name>",
        ),
        (
            MINIMAL.replace("\"id\":\"run\"", r#""id":"run","description":{"author":"person:a","text":"x","signature":"sha256:XYZ"}"#),
            "/cards/0/description/signature",
            "sha256",
        ),
        (
            MINIMAL.replace("\"groups\":[]", r#""groups":[{"card":"nope","collapsed":false,"id":"g","members":["main.ts#function:run/if[0]"],"title":"G"}]"#),
            "/groups/0/card",
            "unknown card",
        ),
        (
            MINIMAL.replace("\"groups\":[]", r#""groups":[{"card":"run","collapsed":false,"id":"run","members":["main.ts#a"],"title":"G"}]"#),
            "/groups/0/id",
            "duplicate id",
        ),
        (
            MINIMAL.replace("\"groups\":[]", r#""groups":[{"card":"run","collapsed":false,"id":"g","members":["main.ts#a","main.ts#a"],"title":"G"}]"#),
            "/groups/0/members/1",
            "repeats",
        ),
        (
            MINIMAL.replace("\"groups\":[]", r#""groups":[],"guards":{"main.ts#a":{"kind":"lambda","owner":"","signature":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}}"#),
            "/guards/main.ts#a/kind",
            "block kind",
        ),
        (
            MINIMAL.replace("\"groups\":[]", r#""groups":[],"labels":{"a/b#c":{"author":"agent:x","text":"t","extra":1}}"#),
            "/labels/a~1b#c/extra",
            "unknown field",
        ),
    ];
    for (text, path, message) in cases {
        let (actual_path, actual_message) = invalid(&text);
        assert_eq!(actual_path, path, "{text}");
        assert!(
            actual_message.contains(message),
            "{actual_message} lacks {message}"
        );
    }
    assert!(
        matches!(parse("{\"a\":1,\"a\":2}"), Err(BoardError::Syntax { .. })),
        "duplicate keys"
    );
    assert!(matches!(parse("[1,"), Err(BoardError::Syntax { line: 1, .. })));
    assert_eq!(invalid("[]").0, "/");
}

#[test]
fn serialization_is_canonical_and_round_trips() {
    let mut board = parse(MINIMAL).unwrap();
    board.cards.insert(
        0,
        Card {
            id: "z-last".to_owned(),
            reference: CardRef::Entrance(Key::from_text("entrance:[\"manual\",\"worker\",\"\",\"\",\"x\"]")),
            at: [-0.5, 1e21],
            view: Some(View::Code),
            folded: true,
            description: Some(Description {
                text: "Quote \" and\nnewline".to_owned(),
                author: author(),
                signature: None,
            }),
        },
    );
    let text = serialize(&board);
    let expected = "{\n  \"cards\": [\n    {\n      \"at\": [\n        0,\n        0\n      ],\n      \"id\": \"run\",\n      \"ref\": {\n        \"block\": \"main.ts#function:run\"\n      }\n    },\n    {\n      \"at\": [\n        -0.5,\n        1e+21\n      ],\n      \"description\": {\n        \"author\": \"agent:test\",\n        \"signature\": \"\",\n        \"text\": \"Quote \\\" and\\nnewline\"\n      },\n      \"folded\": true,\n      \"id\": \"z-last\",\n      \"ref\": {\n        \"entrance\": \"entrance:[\\\"manual\\\",\\\"worker\\\",\\\"\\\",\\\"\\\",\\\"x\\\"]\"\n      },\n      \"view\": \"code\"\n    }\n  ],\n  \"defaultView\": \"code\",\n  \"format\": \"unfold-board\",\n  \"groups\": [],\n  \"guards\": {},\n  \"labels\": {},\n  \"title\": \"T\",\n  \"version\": 1\n}\n";
    assert_eq!(text, expected);
    let reparsed = parse(&text).unwrap();
    assert_eq!(serialize(&reparsed), text);
    // A false fold is the default and is never written.
    let mut unfolded = reparsed;
    unfolded.cards.iter_mut().for_each(|card| card.folded = false);
    assert!(!serialize(&unfolded).contains("folded"));
    // The schema is the documented one and is valid JSON.
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    assert_eq!(schema["properties"]["format"]["const"], FORMAT);
    let mirror = std::fs::read_to_string(
        unfold_core::testkit::testdata_dir().join("../docs/schemas/unfold-board.schema.json"),
    )
    .unwrap();
    assert_eq!(mirror, SCHEMA, "docs/schemas mirrors the crate's schema");
}

#[test]
fn descriptions_are_signed_from_verified_source_and_go_outdated() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    describe_card(&mut board, &graph, "run", "Explains run", &author()).unwrap();
    set_label(&mut board, &graph, FIRST, "Start with one", &author()).unwrap();
    let card_signature = board.cards[0].description.as_ref().unwrap().signature.unwrap();
    assert_eq!(Some(card_signature), graph.signature(RUN));
    assert!(health(&board, &graph).is_healthy());
    assert!(
        set_label(&mut board, &graph, RUN, "The card itself", &author()).is_err(),
        "a card is not its own node"
    );

    // The file changes on disk but is not compiled yet: nothing can be signed against it.
    std::fs::write(
        dir.path().join("main.ts"),
        SOURCE.replace("first = 1", "first = 2"),
    )
    .unwrap();
    let stale = BoardGraph::from_graph(&graph_of(SOURCE), dir.path());
    assert_eq!(health(&board, &stale).outdated, 2);
    assert!(matches!(
        describe_card(&mut board.clone(), &stale, "run", "x", &author()),
        Err(BoardError::Unverified { .. })
    ));

    // After the compile, the named references reconnect and their text is outdated.
    let updated = recompiled(dir.path(), &SOURCE.replace("first = 1", "first = 2"));
    let report = health(&board, &updated);
    assert_eq!((report.outdated, report.review, report.missing), (2, 0, 0));
    assert!(
        report
            .items
            .iter()
            .all(|item| item.repairs.contains(&RepairKind::Resign))
    );
    apply_repair(
        &mut board,
        &updated,
        &Repair::Resign(Target::Card("run".to_owned())),
    )
    .unwrap();
    apply_repair(
        &mut board,
        &updated,
        &Repair::Resign(Target::Label(FIRST.to_owned())),
    )
    .unwrap();
    assert!(health(&board, &updated).is_healthy());
    // Blank text removes a label.
    set_label(&mut board, &updated, FIRST, "  ", &author()).unwrap();
    assert!(board.labels.is_empty());
}

#[test]
fn an_ordinal_reference_whose_code_changed_is_held_for_review() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.groups.push(group(&[IF0]));
    set_label(&mut board, &graph, IF0, "Log when set", &author()).unwrap();
    set_label(&mut board, &graph, FIRST, "Start with one", &author()).unwrap();
    let (saved, report) = prepare_save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    assert_eq!(saved.guards[IF0].kind, "if");
    assert_eq!(saved.guards[IF0].owner, RUN);
    assert_eq!(
        saved.guards.len(),
        3,
        "the card, the group member, and the labels"
    );

    let inserted = SOURCE.replace(
        "  if (first)",
        "  if (!first) { console.log('inserted'); }\n  if (first)",
    );
    let updated = recompiled(dir.path(), &inserted);
    let report = health(&saved, &updated);
    let held: Vec<&HealthItem> = report
        .items
        .iter()
        .filter(|item| item.kind == HealthKind::Review)
        .collect();
    assert_eq!(held.len(), 2, "{:#?}", report.items);
    assert!(
        held.iter()
            .all(|item| item.message.contains("Position-based reference changed"))
    );
    assert!(
        report
            .items
            .iter()
            .all(|item| item.target != Target::Label(FIRST.to_owned())),
        "a named, unchanged reference stays healthy"
    );
    assert!(describe_group(&mut saved.clone(), &updated, "steps", "New text", &author()).is_err());
    // Mermaid never applies the held group or label to the new code.
    let output = crate::mermaid::board(&saved, &updated);
    assert!(output.contains("Group needs review: Steps"));
    assert!(!output.contains("Log when set"));
    // Saving with the problem carried over unchanged is allowed; accepting clears it.
    let mut moved = saved.clone();
    moved.cards[0].at = [99.0, 99.0];
    let (moved, moved_report) = prepare_save(moved, Some((&saved, &report)), &updated).unwrap();
    assert_eq!(moved_report.review, 2);
    assert_eq!(
        moved.guards[IF0], saved.guards[IF0],
        "guards survive until accepted"
    );
    let mut accepted = moved;
    apply_repair(&mut accepted, &updated, &Repair::Accept(IF0.to_owned())).unwrap();
    let after = health(&accepted, &updated);
    assert_eq!(after.review, 0);
    assert_eq!(after.outdated, 1, "the accepted label must still be re-signed");
}

#[test]
fn groups_are_consecutive_siblings_in_one_body() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let then_statement = "main.ts#function:run/if[0]/expression_statement[0]";
    let cases: [(&[&str], Option<&str>); 6] = [
        (&[FIRST, IF0], None),
        (&[FIRST, LOG0], Some("consecutive siblings")),
        (&[LOG0, IF0], Some("consecutive siblings")),
        (&[IF0, then_statement], Some("siblings in the same body")),
        (&[RUN], Some("inside their card's body")),
        (&["main.ts#function:run/nope[0]"], Some("missing block")),
    ];
    for (members, problem) in cases {
        board.groups = vec![group(members)];
        let found = group_problem(&board, &graph, &board.groups[0]);
        match problem {
            Some(expected) => assert!(
                found.is_some_and(|text| text.contains(expected)),
                "{members:?}: {found:?}"
            ),
            None => assert_eq!(found, None, "{members:?}"),
        }
    }
    // Overlapping and nested groups are broken.
    board.groups = vec![
        group(&[FIRST, IF0]),
        Group {
            id: "inner".to_owned(),
            ..group(&[then_statement])
        },
    ];
    assert!(check(&board, &graph).is_err());
    let report = health(&board, &graph);
    assert_eq!(report.broken_groups, 2);
    apply_repair(&mut board, &graph, &Repair::Ungroup("inner".to_owned())).unwrap();
    assert!(check(&board, &graph).is_ok());
    // A group on an entrance card is broken.
    board.cards.push(Card {
        id: "entrance".to_owned(),
        reference: CardRef::Entrance(Key::from_text("entrance:none")),
        at: [0.0, 0.0],
        view: None,
        folded: false,
        description: None,
    });
    let entrance_group = Group {
        card: "entrance".to_owned(),
        ..group(&[FIRST])
    };
    assert_eq!(
        group_problem(&board, &graph, &entrance_group),
        Some("Group must belong to a block card")
    );
}

#[test]
fn missing_references_are_placeholders_with_repairs() {
    let (dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    set_label(&mut board, &graph, RETURN0, "Hand back first", &author()).unwrap();
    let (saved, _) = prepare_save(board, None, &graph).unwrap();
    let renamed = SOURCE.replace("function run", "function start");
    let updated = recompiled(dir.path(), &renamed);
    let report = health(&saved, &updated);
    assert_eq!(report.missing, 2);
    let card = report
        .items
        .iter()
        .find(|item| item.target == Target::Card("run".to_owned()))
        .unwrap();
    assert_eq!(card.repairs, [RepairKind::Reconnect, RepairKind::RemoveCard]);
    // A position update keeps the placeholder; a new missing reference is refused.
    let mut moved = saved.clone();
    moved.cards[0].at = [1.0, 2.0];
    assert!(prepare_save(moved.clone(), Some((&saved, &report)), &updated).is_ok());
    moved.cards.push(Card {
        id: "new".to_owned(),
        reference: CardRef::Block(Key::from_text("main.ts#function:nope")),
        ..moved.cards[0].clone()
    });
    assert!(matches!(
        prepare_save(moved, Some((&saved, &report)), &updated),
        Err(BoardError::Problem { target: Target::Card(id), .. }) if id == "new"
    ));
    // The manual repair after a rename.
    let mut repaired = saved;
    apply_repair(
        &mut repaired,
        &updated,
        &Repair::Reconnect {
            from: RUN.to_owned(),
            to: "main.ts#function:start".to_owned(),
        },
    )
    .unwrap();
    apply_repair(
        &mut repaired,
        &updated,
        &Repair::Reconnect {
            from: RETURN0.to_owned(),
            to: "main.ts#function:start/return[0]".to_owned(),
        },
    )
    .unwrap();
    let report = health(&repaired, &updated);
    assert_eq!(report.missing, 0, "{:#?}", report.items);
    assert!(repaired.guards.contains_key("main.ts#function:start"));
    assert!(!repaired.guards.contains_key(RUN));
    assert!(
        apply_repair(
            &mut repaired,
            &updated,
            &Repair::Reconnect {
                from: "a".to_owned(),
                to: "main.ts#nope".to_owned()
            }
        )
        .is_err()
    );
    apply_repair(&mut repaired, &updated, &Repair::RemoveCard("run".to_owned())).unwrap();
    assert!(repaired.cards.is_empty());
}

#[test]
fn reconnecting_onto_a_member_never_repeats_it() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.groups.push(group(&[FIRST, IF0]));
    apply_repair(
        &mut board,
        &graph,
        &Repair::Reconnect {
            from: FIRST.to_owned(),
            to: IF0.to_owned(),
        },
    )
    .unwrap();
    assert_eq!(board.groups[0].members, [Key::from_text(IF0)]);
    assert_eq!(
        parse(&serialize(&board)).unwrap(),
        board,
        "the board still reads back"
    );
}

#[test]
fn saving_signs_restores_dropped_guards_and_drops_unused_ones() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    board.cards[0].description = Some(Description {
        text: "Unsigned until saved".to_owned(),
        author: author(),
        signature: None,
    });
    board.groups.push(group(&[FIRST, IF0]));
    board.groups[0].description = Some(Description {
        text: "Set up".to_owned(),
        author: author(),
        signature: None,
    });
    let (saved, report) = prepare_save(board, None, &graph).unwrap();
    assert!(report.is_healthy());
    assert_eq!(
        saved.cards[0].description.as_ref().unwrap().signature,
        graph.signature(RUN)
    );
    assert_eq!(
        saved.groups[0].description.as_ref().unwrap().signature,
        graph.span_signature(FIRST, IF0)
    );
    let mut edited = saved.clone();
    edited.guards.clear();
    edited.guards.insert(
        "main.ts#unused".to_owned(),
        Guard {
            kind: "if".to_owned(),
            owner: String::new(),
            signature: Signature::of(b"x"),
        },
    );
    let (again, _) = prepare_save(edited, Some((&saved, &report)), &graph).unwrap();
    assert_eq!(again.guards, saved.guards);
    assert_eq!(serialize(&again), serialize(&saved));
}

#[test]
fn the_outline_follows_calls_and_counts_unlabeled_nodes() {
    let (_dir, graph) = folder(SOURCE);
    let mut board = sample_board();
    let empty = outline(&board, &graph);
    assert_eq!(
        empty.rows,
        [OutlineRow {
            card: "run".to_owned(),
            depth: 0
        }]
    );
    let unlabeled = empty.unlabeled.len();
    assert_eq!(unlabeled, 5, "first, if, its then statement, the log, the return");
    set_label(&mut board, &graph, FIRST, "Start with one", &author()).unwrap();
    assert_eq!(outline(&board, &graph).unlabeled.len(), unlabeled - 1);
}

#[test]
fn board_files_publish_once_and_write_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let board = parse(MINIMAL).unwrap();
    let text = serialize(&board);

    let draft = BoardFile::draft("7f3c2a").unwrap();
    assert_eq!(draft.relative(), ".unfold/local/drafts/7f3c2a.json");
    let revision = files::save(root, &draft, &text).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join(".unfold/local/.gitignore")).unwrap(),
        "*\n"
    );
    assert!(
        !root.join(".unfold/boards").exists(),
        "a draft writes nothing to share"
    );
    let loaded = files::load(root, &draft).unwrap();
    assert_eq!((loaded.board.clone(), loaded.revision), (board, revision));
    assert_eq!(files::change(root, &draft, revision).unwrap(), Change::Unchanged);

    // An external edit is detected.
    std::fs::write(draft.path(root), text.replace("\"T\"", "\"Edited\"")).unwrap();
    assert!(matches!(
        files::change(root, &draft, revision).unwrap(),
        Change::Changed(_)
    ));

    let name = files::slug("  Order placement: v2!  ");
    assert_eq!(name, "order-placement-v2");
    let (published, _) = files::publish(root, &draft, &name, &text).unwrap();
    assert_eq!(published.relative(), ".unfold/boards/order-placement-v2.json");
    assert_eq!(std::fs::read_to_string(published.path(root)).unwrap(), text);
    assert_eq!(
        files::change(root, &draft, revision).unwrap(),
        Change::Missing,
        "publishing removes the draft"
    );

    // Publish never overwrites; the collision names the file and offers a free name.
    let second = BoardFile::draft("second").unwrap();
    files::save(root, &second, &text).unwrap();
    let other = text.replace("\"T\"", "\"Other\"");
    match files::publish(root, &second, &name, &other) {
        Err(BoardError::NameTaken { path, suggestion }) => {
            assert_eq!(path, ".unfold/boards/order-placement-v2.json");
            assert_eq!(suggestion, "order-placement-v2-2");
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(published.path(root)).unwrap(),
        text,
        "the existing board is untouched"
    );
    assert!(second.path(root).exists(), "the draft stays");

    // Rename never overwrites either; move back to drafts; delete.
    let renamed = files::rename(root, &published, "order-placement").unwrap();
    assert!(!published.path(root).exists());
    assert!(matches!(
        files::rename(root, &renamed, "order-placement"),
        Err(BoardError::NameTaken { .. })
    ));
    let back = BoardFile::draft("back").unwrap();
    files::unpublish(root, &renamed, &back, &text).unwrap();
    assert!(!renamed.path(root).exists());
    assert_eq!(
        files::list(root, Place::Draft)
            .unwrap()
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>(),
        ["back", "second"]
    );
    files::remove(root, &back).unwrap();
    files::remove(root, &back).unwrap();
    assert!(files::list(root, Place::Published).unwrap().is_empty());

    // Atomic replacement leaves no temporary files behind.
    let target = root.join("atomic.json");
    files::write_atomic(&target, b"one").unwrap();
    files::write_atomic(&target, b"two").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"two");
    let leftovers = std::fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);

    for bad in [
        "../escape",
        "a/b",
        "",
        ".hidden",
        "a\\b",
        "x".repeat(101).as_str(),
    ] {
        assert!(
            matches!(BoardFile::published(bad), Err(BoardError::InvalidName { .. })),
            "{bad}"
        );
    }
    assert_eq!(files::slug("¿¿"), "board");
    assert_eq!(files::slug("Café Crème"), "cafe-creme");
}

#[cfg(unix)]
#[test]
fn board_directories_must_not_be_links() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".unfold")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".unfold/boards")).unwrap();
    let text = serialize(&parse(MINIMAL).unwrap());
    assert!(matches!(
        files::create_published(dir.path(), "x", &text),
        Err(BoardError::Io { .. })
    ));
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());

    // A board-looking file outside the folder is never read, listed, renamed, or deleted
    // through the link (a cloned repository can commit `.unfold/boards` as a link).
    let victim = outside.path().join("x.json");
    std::fs::write(&victim, &text).unwrap();
    let file = BoardFile::published("x").unwrap();
    let refused = |result: Result<(), BoardError>| matches!(result, Err(BoardError::Io { .. }));
    assert!(refused(files::remove(dir.path(), &file)));
    assert!(refused(files::load(dir.path(), &file).map(drop)));
    assert!(refused(
        files::change(dir.path(), &file, Revision::of(b"")).map(drop)
    ));
    assert!(refused(files::list(dir.path(), Place::Published).map(drop)));
    assert!(refused(files::rename(dir.path(), &file, "y").map(drop)));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);

    // A board file that is itself a link is refused for reading; removing it removes the link.
    std::fs::remove_file(dir.path().join(".unfold/boards")).unwrap();
    std::fs::create_dir(dir.path().join(".unfold/boards")).unwrap();
    std::os::unix::fs::symlink(&victim, file.path(dir.path())).unwrap();
    assert!(refused(files::load(dir.path(), &file).map(drop)));
    assert!(refused(
        files::change(dir.path(), &file, Revision::of(b"")).map(drop)
    ));
    files::remove(dir.path(), &file).unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), text);
}

#[test]
fn free_names_stay_valid_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let text = serialize(&parse(MINIMAL).unwrap());
    // A taken name at the length limit still gets a valid alternative (this used to loop
    // forever, because every suffixed candidate was too long).
    let long = "a".repeat(100);
    files::create_published(dir.path(), &long, &text).unwrap();
    match files::create_published(dir.path(), &long, &text) {
        Err(BoardError::NameTaken { suggestion, .. }) => {
            assert_eq!(suggestion, format!("{}-2", "a".repeat(98)));
            assert!(BoardFile::published(&suggestion).is_ok());
        }
        result => panic!("expected a collision, got {result:?}"),
    }
    assert_eq!(files::free_name(dir.path(), "fresh"), "fresh");
}

#[test]
fn board_names_avoid_reserved_device_names() {
    for reserved in ["con", "CON", "aux.v2", "nul", "com1", "LPT9"] {
        assert!(
            matches!(
                BoardFile::published(reserved),
                Err(BoardError::InvalidName { .. })
            ),
            "{reserved}"
        );
    }
    for fine in ["console", "com0", "com10", "auxiliary", "board"] {
        assert!(BoardFile::published(fine).is_ok(), "{fine}");
    }
    assert_eq!(files::slug("Con"), "con-board");
    assert!(BoardFile::published(&files::slug("AUX")).is_ok());
}

#[test]
fn oversized_board_files_are_refused_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let file = BoardFile::draft("big").unwrap();
    files::save(dir.path(), &file, &serialize(&parse(MINIMAL).unwrap())).unwrap();
    let handle = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path(dir.path()))
        .unwrap();
    handle.set_len(files::MAX_BOARD_BYTES + 1).unwrap();
    assert!(matches!(
        files::load(dir.path(), &file),
        Err(BoardError::Io { .. })
    ));
}
