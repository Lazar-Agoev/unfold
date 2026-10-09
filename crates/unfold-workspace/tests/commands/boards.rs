//! Boards across windows: claims, `held_by`, Move to New Window, saving, publishing,
//! unpublishing, renaming, deleting, health in Problems, and outside edits.

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE};

/// A block card for `key` at a position.
fn block_card(id: &str, key: &str) -> Value {
    json!({ "id": id, "ref": { "block": key }, "at": [0, 0] })
}

#[test]
fn a_board_is_open_in_one_window_at_a_time() {
    let harness = Harness::compiled("ts-orders");
    harness.open(2);
    let board = harness.ok(1, "new_board", json!({ "title": "Order placement" }));
    let id = board["id"].as_str().expect("id").to_owned();
    assert!(id.starts_with(".unfold/local/drafts/"), "{id}");
    assert_eq!(board["document"]["format"], "unfold-board");

    // Window 2 is told who has it, and can't touch it.
    let held = harness.ok(2, "open_board", json!({ "id": id }));
    assert_eq!(held, json!({ "status": "held", "heldBy": 1 }));
    let mut document = board["document"].clone();
    document["cards"] = json!([block_card("create", "src/routes/orders.ts#function:createOrder")]);
    let save = json!({ "id": id, "board": document });
    assert_eq!(harness.code(2, "save_board", save.clone()), "board_held");
    assert_eq!(
        harness.code(
            2,
            "card",
            json!({ "key": "src/routes/orders.ts#function:createOrder", "board": id })
        ),
        "board_held"
    );

    // Window 1 saves; the listing shows the card and who holds the board.
    let saved = harness.ok(1, "save_board", save);
    assert_eq!(saved["document"]["cards"].as_array().map(Vec::len), Some(1));
    assert_ne!(saved["revision"], board["revision"]);
    let list = harness.ok(2, "list_boards", json!({}));
    let draft = &list["drafts"][0];
    assert_eq!(draft["id"], id.as_str());
    assert_eq!(draft["cards"], 1);
    assert_eq!(draft["heldBy"], 1);

    // Move to New Window hands the claim over.
    harness.ok(1, "move_board", json!({ "id": id, "toWindow": 2 }));
    assert_eq!(harness.code(1, "board_health", json!({ "id": id })), "board_held");
    let health = harness.ok(2, "board_health", json!({ "id": id }));
    assert_eq!(health["outline"]["rows"][0]["card"], "create");
    assert_eq!(
        harness.code(1, "move_board", json!({ "id": id, "toWindow": 1 })),
        "board_held"
    );

    // Closing window 2 releases it; window 1 can open it again.
    harness.host.close_workspace(2).expect("close");
    let reopened = harness.ok(1, "open_board", json!({ "id": id }));
    assert_eq!(reopened["status"], "opened");
    assert_eq!(reopened["board"]["document"]["cards"][0]["id"], "create");
}

#[test]
fn publish_never_overwrites_and_boards_move_between_places() {
    let harness = Harness::compiled("ts-orders");
    let first = harness.ok(1, "new_board", json!({ "title": "Order placement" }));
    let published = harness.ok(1, "publish_board", json!({ "id": first["id"] }));
    assert_eq!(published["status"], "done");
    let published_id = published["board"]["id"].as_str().expect("id").to_owned();
    assert_eq!(published_id, ".unfold/boards/order-placement.json");
    assert!(harness.root.join(&published_id).is_file());
    assert!(
        !harness.root.join(first["id"].as_str().expect("id")).exists(),
        "the draft is gone"
    );

    // A second board with the same title gets a suggestion instead of overwriting.
    let second = harness.ok(1, "new_board", json!({ "title": "Order placement" }));
    let taken = harness.ok(1, "publish_board", json!({ "id": second["id"] }));
    assert_eq!(
        taken,
        json!({
            "status": "name_taken",
            "path": ".unfold/boards/order-placement.json",
            "suggestion": "order-placement-2",
        })
    );
    let renamed = harness.ok(
        1,
        "publish_board",
        json!({ "id": second["id"], "name": taken["suggestion"] }),
    );
    assert_eq!(renamed["board"]["id"], ".unfold/boards/order-placement-2.json");

    // Rename the file and the title, move it back to drafts, and delete it.
    let renamed = harness.ok(
        1,
        "rename_board",
        json!({ "id": published_id, "title": "Placing orders", "name": "placing-orders" }),
    );
    let id = renamed["board"]["id"].as_str().expect("id").to_owned();
    assert_eq!(id, ".unfold/boards/placing-orders.json");
    assert_eq!(renamed["board"]["document"]["title"], "Placing orders");
    let taken = harness.ok(
        1,
        "rename_board",
        json!({ "id": id, "name": "order-placement-2" }),
    );
    assert_eq!(taken["status"], "name_taken");
    let draft = harness.ok(1, "unpublish_board", json!({ "id": id }));
    let draft_id = draft["id"].as_str().expect("id").to_owned();
    assert!(
        draft_id.starts_with(".unfold/local/drafts/placing-orders"),
        "{draft_id}"
    );
    assert!(!harness.root.join(&id).exists());
    harness.ok(1, "delete_board", json!({ "id": draft_id }));
    assert_eq!(
        harness.code(1, "board_health", json!({ "id": draft_id })),
        "board_not_held"
    );
    let list = harness.ok(1, "list_boards", json!({}));
    assert_eq!(list["drafts"].as_array().map(Vec::len), Some(0));
    assert_eq!(list["published"].as_array().map(Vec::len), Some(1));
}

#[test]
fn saves_are_validated_and_health_reaches_problems() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.ok(1, "new_board", json!({}));
    let id = board["id"].as_str().expect("id").to_owned();
    let mut document = board["document"].clone();

    // A reference that isn't in the graph is refused.
    document["cards"] = json!([block_card("ghost", "src/routes/orders.ts#function:noSuchThing")]);
    assert_eq!(
        harness.code(1, "save_board", json!({ "id": id, "board": document })),
        "board_problem"
    );
    // A document that isn't an unfold-board is refused.
    assert_eq!(
        harness.code(1, "save_board", json!({ "id": id, "board": { "title": "x" } })),
        "board_invalid"
    );

    // A description is signed from verified source when saved.
    document["cards"] = json!([{
        "id": "create",
        "ref": { "block": "src/routes/orders.ts#function:createOrder" },
        "at": [0, 0],
        "description": { "text": "Places the order.", "author": "person:tester", "signature": "" },
    }]);
    let saved = harness.ok(1, "save_board", json!({ "id": id, "board": document }));
    let signature = saved["document"]["cards"][0]["description"]["signature"]
        .as_str()
        .expect("signature");
    assert!(signature.starts_with("sha256:"), "{signature}");
    assert!(saved["label"].is_null(), "a healthy board has no label");

    // Changing the described code makes the description outdated after the next compile.
    let path = harness.root.join("src/routes/orders.ts");
    let text = std::fs::read_to_string(&path).expect("read");
    let changed = text.replacen(
        "export async function createOrder(",
        "export async function createOrder(\n  ",
        1,
    );
    assert_ne!(text, changed, "the playground still has createOrder");
    std::fs::write(&path, changed).expect("write");
    harness.ok(1, "compile", json!({}));
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        let problems = harness.ok(1, "problems", json!({}));
        let board_problem = problems
            .as_array()
            .expect("problems")
            .iter()
            .any(|problem| problem["category"] == "board" && problem["path"] == id.as_str());
        if board_problem {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "board health never reached Problems"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let health = harness.ok(1, "board_health", json!({ "id": id }));
    assert_eq!(health["health"]["outdated"], 1);
    assert_eq!(health["label"], "1 outdated");

    // The Resign repair signs it again.
    let repaired = harness.ok(
        1,
        "repair_board",
        json!({ "id": id, "repair": { "kind": "resign", "target": { "card": "create" } } }),
    );
    assert_eq!(repaired["health"]["outdated"], 0);
}

/// The revision string of `text`, as events and results carry it.
fn revision_of(text: &str) -> String {
    unfold_core::ContentHash::of(text.as_bytes()).to_string()
}

/// Waits for the `board_conflict` about `id` that reports `revision` on disk.
fn conflict_at(harness: &Harness, id: &str, revision: &str) -> Value {
    harness
        .sink
        .wait_for(|message| {
            message["event"]["type"] == "board_conflict"
                && message["event"]["board"] == id
                && message["event"]["diskRevision"] == revision
        })
        .expect("a board_conflict with the file's final revision")
}

#[test]
fn outside_edits_are_conflicts() {
    let harness = Harness::new("ts-orders", true);
    harness.open(1);
    harness.ok(1, "compile", json!({}));
    let board = harness.ok(1, "new_board", json!({ "title": "Outside" }));
    let id = board["id"].as_str().expect("id").to_owned();
    // An agent rewrites the draft the way well-behaved writers do: a temporary file renamed
    // over it.
    let mut document = board["document"].clone();
    document["title"] = json!("Rewritten by an agent");
    let text = serde_json::to_string_pretty(&document).expect("json");
    let path = harness.root.join(&id);
    let temporary = path.with_extension("agent-tmp");
    std::fs::write(&temporary, &text).expect("write");
    std::fs::rename(&temporary, &path).expect("rename");
    let event = conflict_at(&harness, &id, &revision_of(&text));
    assert_eq!(event["windows"], json!([1]));
    assert_eq!(event["event"]["savedRevision"], board["revision"]);

    // Saving over it is refused unless forced; reloading takes the agent's version.
    let save = json!({ "id": id, "board": board["document"] });
    assert_eq!(harness.code(1, "save_board", save), "board_conflict");
    let reloaded = harness.ok(1, "reload_board", json!({ "id": id }));
    assert_eq!(reloaded["document"]["title"], "Rewritten by an agent");
    assert_eq!(reloaded["revision"], event["event"]["diskRevision"]);
    let forced = harness.ok(
        1,
        "save_board",
        json!({ "id": id, "board": board["document"], "force": true }),
    );
    assert_eq!(forced["document"]["title"], "Outside");
}

/// A writer that truncates the file and writes it in pieces is caught mid-write: the
/// conflicts report only the finished board, never the empty or half-written file.
#[test]
fn a_non_atomic_writer_is_reported_once_it_finishes() {
    let harness = Harness::new("ts-orders", true);
    harness.open(1);
    harness.ok(1, "compile", json!({}));
    let board = harness.ok(1, "new_board", json!({ "title": "Outside" }));
    let id = board["id"].as_str().expect("id").to_owned();
    let mut document = board["document"].clone();
    document["title"] = json!("Written in pieces");
    let text = serde_json::to_string_pretty(&document).expect("json");
    let path = harness.root.join(&id);
    // Each piece lands well after the watcher's quiet period, so a check sees each one.
    let pause = std::time::Duration::from_millis(150);
    std::fs::write(&path, "").expect("truncate");
    std::thread::sleep(pause);
    std::fs::write(&path, &text[..text.len() / 2]).expect("first half");
    std::thread::sleep(pause);
    std::fs::write(&path, &text).expect("whole board");

    let event = conflict_at(&harness, &id, &revision_of(&text));
    assert_eq!(event["event"]["savedRevision"], board["revision"]);
    // Checks run in order, so every conflict about this board has arrived by now.
    let reported: Vec<Value> = harness
        .sink
        .all()
        .into_iter()
        .filter(|message| {
            message["event"]["type"] == "board_conflict" && message["event"]["board"] == id.as_str()
        })
        .map(|message| message["event"]["diskRevision"].clone())
        .collect();
    assert_eq!(
        reported,
        [json!(revision_of(&text))],
        "one conflict, for the whole board"
    );
    let reloaded = harness.ok(1, "reload_board", json!({ "id": id }));
    assert_eq!(reloaded["document"]["title"], "Written in pieces");
    assert_eq!(reloaded["revision"], json!(revision_of(&text)));
}

#[test]
fn closing_a_window_waits_for_its_last_board_save() {
    // The window's final save and its detach race: the save is dispatched, then the close
    // follows at once on another channel. The close must let the save land.
    for round in 0..5 {
        let harness = Harness::compiled("ts-orders");
        let board = harness.ok(1, "new_board", json!({ "title": format!("Closing {round}") }));
        let id = board["id"].as_str().expect("id").to_owned();
        let mut document = board["document"].clone();
        document["cards"] = json!([block_card("create", "src/routes/orders.ts#function:createOrder")]);
        let (sender, receiver) = std::sync::mpsc::channel();
        harness.host.dispatch_async(
            1,
            harness.request(),
            json!({ "command": "save_board", "args": { "id": id, "board": document } }).to_string(),
            move |result| {
                let _ = sender.send(result);
            },
        );
        harness.host.close_workspace(1).expect("close");
        let saved = receiver
            .recv_timeout(PATIENCE)
            .expect("the save finishes")
            .unwrap_or_else(|error| panic!("the save failed: {error}"));
        assert!(saved.contains("createOrder"), "{saved}");
        let text = std::fs::read_to_string(harness.root.join(&id)).expect("the board file");
        let written: Value = serde_json::from_str(&text).expect("board JSON");
        assert_eq!(written["cards"][0]["id"], "create", "round {round}");
        // After the close, the window holds nothing.
        assert_eq!(harness.code(1, "list_boards", json!({})), "not_attached");
    }
}
