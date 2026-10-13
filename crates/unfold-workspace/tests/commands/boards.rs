//! Boards (docs/ipc.md › Boards): boards are files in `.unfold/boards/`, saved as they are
//! edited; open boards and their sessions, reading a board without opening it, pages, drawn
//! links, connections, frames, resource cards, and notes, one-level folders, the boards list by
//! last edit, outside edits that reload an open board, references, and export.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE, eventually, restless};

const CREATE_ORDER: &str = "src/routes/orders.ts#function:createOrder";
const PLACE_ORDER: &str = "src/services/order-service.ts#class:OrderService/method:place";
const POST_ORDERS: &str = r#"entrance:["express.route","http","POST","/orders","requireUser"]"#;

/// A card for block or entrance `key`.
pub(crate) fn card(id: &str, key: &str) -> Value {
    let kind = if key.starts_with("entrance:") {
        "entrance"
    } else {
        "block"
    };
    json!({ "id": id, "ref": { kind: key }, "at": [0, 0] })
}

/// An unsigned description by a tester.
fn text(words: &str) -> Value {
    json!({ "text": words, "author": "person:tester", "signature": "" })
}

/// What a window saves of a board it shows: its title and pages.
pub(crate) fn edit_of(state: &Value) -> Value {
    json!({ "title": state["document"]["title"], "pages": state["document"]["pages"] })
}

pub(crate) fn id_of(state: &Value) -> String {
    state["id"].as_str().expect("a board id").to_owned()
}

/// The revision string of `text`, as results and events carry it.
fn revision_of(text: &str) -> String {
    unfold_core::ContentHash::of(text.as_bytes()).to_string()
}

impl Harness {
    /// A new board titled `title` in the first project, opened in `window`.
    pub(crate) fn new_board(&self, window: u32, title: &str) -> Value {
        self.ok(
            window,
            "new_board",
            json!({ "project": self.project, "title": title }),
        )
    }

    /// Saves `edit` as board `id` of the first project from `window`.
    pub(crate) fn save(&self, window: u32, id: &str, edit: Value) -> Value {
        self.ok(
            window,
            "save_board",
            json!({ "project": self.project, "id": id, "board": edit }),
        )
    }

    /// Runs a command about board `id` of the first project.
    fn on(&self, window: u32, command: &str, id: &str) -> Value {
        self.ok(window, command, json!({ "project": self.project, "id": id }))
    }

    /// The error code of a command about board `id` of the first project.
    fn on_code(&self, window: u32, command: &str, id: &str) -> String {
        self.code(window, command, json!({ "project": self.project, "id": id }))
    }

    /// The first project's board list.
    fn boards(&self) -> Value {
        self.ok(1, "list_boards", json!({ "project": self.project }))
    }

    /// The ids of the first project's boards, in list order.
    fn board_ids(&self) -> Vec<String> {
        self.boards()["boards"]
            .as_array()
            .expect("boards")
            .iter()
            .map(|board| board["id"].as_str().expect("an id").to_owned())
            .collect()
    }

    /// The first project's board `id` in its board list.
    fn listed(&self, id: &str) -> Value {
        let list = self.boards();
        list["boards"]
            .as_array()
            .expect("boards")
            .iter()
            .find(|board| board["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no {id} in {list}"))
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub(crate) fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.path(relative)).expect("read a board file")
    }

    /// The `board_reloaded` messages about board `id` so far.
    fn reloads(&self, id: &str) -> Vec<Value> {
        self.sink
            .all()
            .into_iter()
            .filter(|message| message["event"]["type"] == "board_reloaded" && message["event"]["board"] == id)
            .collect()
    }

    /// Waits for the `board_reloaded` message about board `id` that `found` picks.
    fn reload(&self, id: &str, what: &str, found: impl Fn(&Value) -> bool) -> Value {
        eventually(what, || self.reloads(id).iter().any(&found));
        self.reloads(id)
            .into_iter()
            .find(|message| found(message))
            .expect("the reload")
    }
}

/// Replaces `path` the way well-behaved writers do: a temporary file renamed over it.
fn write_atomically(path: &std::path::Path, text: &str) {
    let temporary = path.with_extension("agent-tmp");
    std::fs::write(&temporary, text).expect("write");
    std::fs::rename(&temporary, path).expect("rename");
}

/// The sample board (`testdata/boards/order-placement.json`).
fn sample_board() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/boards/order-placement.json"),
    )
    .expect("the sample board")
}

/// Sets the modification time of `path` to `seconds` after the Unix epoch.
fn touch(path: &std::path::Path, seconds: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .expect("set the modification time");
}

#[test]
fn an_open_board_keeps_its_session_until_it_closes() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let board = harness.ok(
        1,
        "new_board",
        json!({ "project": project, "title": "Order placement", "folder": "Checkout", "pageTitle": "Placing" }),
    );
    let id = id_of(&board);
    assert_eq!(id, ".unfold/boards/order-placement.json");
    assert_eq!(board["project"], project.as_str());
    assert_eq!(board["name"], "order-placement");
    let document = &board["document"];
    assert_eq!(document["format"], "unfold-board");
    assert_eq!(document["version"], 1);
    assert_eq!(document["folder"], "Checkout");
    assert_eq!(document["pages"][0]["id"], "page-1");
    assert_eq!(document["pages"][0]["title"], "Placing");
    assert_eq!(board["revision"], revision_of(&harness.read(&id)));

    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let save = json!({ "project": project, "id": id, "board": edit });
    let saved = harness.ok(1, "save_board", save);
    let page = &saved["document"]["pages"][0];
    assert_eq!(page["cards"][0]["id"], "create");
    assert_eq!(page["guards"][0]["key"], CREATE_ORDER, "guards are written");
    assert_eq!(saved["document"]["folder"], "Checkout", "the folder is Rust's");
    assert_ne!(saved["revision"], board["revision"]);
    let row = harness.boards()["boards"][0].clone();
    assert_eq!(row["id"], id.as_str());
    assert_eq!((row["pages"].clone(), row["cards"].clone()), (json!(1), json!(1)));
    assert_eq!(row["title"], "Order placement");
    assert_eq!(row["folder"], "Checkout");
    assert_eq!(row["label"], Value::Null);
    assert!(row["updatedAtMs"].as_i64().is_some_and(|at| at > 0));
    let health = harness.on(1, "board_health", &id);
    assert_eq!(health["label"], Value::Null);

    // Opening it again keeps its session: without a watcher, an outside change goes unseen.
    write_atomically(
        &harness.path(&id),
        &harness.read(&id).replace("Order placement", "Theirs"),
    );
    let reopened = harness.on(1, "open_board", &id);
    assert_eq!(reopened["revision"], saved["revision"]);
    assert_eq!(reopened["document"]["title"], "Order placement");
    assert_eq!(reopened["document"]["pages"][0]["cards"][0]["id"], "create");

    // Closing the window closes it; opening it in the reopened window (a new id) reads the file.
    harness.host.detach_window(1).expect("detach");
    assert_eq!(harness.on_code(2, "board_health", &id), "board_not_open");
    let fresh = harness.on(2, "open_board", &id);
    assert_eq!(fresh["document"]["title"], "Theirs");

    // Closing it closes it.
    harness.on(2, "close_board", &id);
    assert_eq!(harness.on_code(2, "board_health", &id), "board_not_open");
    assert_eq!(harness.on_code(2, "close_board", &id), "board_not_open");
    assert_eq!(harness.on_code(2, "open_board", "../x.json"), "invalid_args");
    assert_eq!(
        harness.on_code(2, "open_board", ".unfold/local/x.json"),
        "invalid_args"
    );
    assert_eq!(
        harness.on_code(2, "open_board", ".unfold/boards/nope.json"),
        "not_found"
    );
    assert_eq!(
        harness.on_code(2, "close_board", ".unfold/boards/nope.json"),
        "board_not_open",
        "a board that can't be opened isn't open"
    );
}

#[test]
fn boards_are_files_saved_over_whatever_they_hold() {
    let harness = Harness::compiled("ts-orders");
    let first = harness.new_board(1, "Order placement");
    let second = harness.new_board(1, "Order placement");
    assert_eq!(id_of(&first), ".unfold/boards/order-placement.json");
    assert_eq!(
        id_of(&second),
        ".unfold/boards/order-placement-2.json",
        "a new board never overwrites another"
    );
    let untitled = harness.ok(1, "new_board", json!({ "project": harness.project }));
    assert_eq!(id_of(&untitled), ".unfold/boards/untitled-board.json");

    // Every save writes the board file itself, canonically.
    let id = id_of(&first);
    let mut edit = edit_of(&first);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let saved = harness.save(1, &id, edit.clone());
    let written = harness.read(&id);
    assert_eq!(saved["revision"], revision_of(&written));
    let file: Value = serde_json::from_str(&written).expect("board JSON");
    assert_eq!(file["title"], saved["document"]["title"]);
    assert_eq!(
        file["pages"][0]["guards"],
        saved["document"]["pages"][0]["guards"]
    );
    for gone in ["projects", "publishes"] {
        assert!(file.get(gone).is_none(), "{gone} in {written}");
    }

    // A save writes the window's version over the file, whatever the file holds now: no
    // revision check, nothing refused (a failed save's Retry relies on it).
    let theirs = written.replace("\"Order placement\"", "\"Theirs\"");
    std::fs::write(harness.path(&id), &theirs).expect("an outside edit");
    edit["title"] = json!("Mine");
    let mine = harness.save(1, &id, edit.clone());
    assert_eq!(mine["document"]["title"], "Mine");
    assert_eq!(mine["revision"], revision_of(&harness.read(&id)));
    // Even a deleted file: the save writes it again.
    std::fs::remove_file(harness.path(&id)).expect("an outside delete");
    edit["title"] = json!("Mine again");
    harness.save(1, &id, edit);
    assert!(harness.read(&id).contains("\"Mine again\""));
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": edit_of(&mine), "force": true })
        ),
        "invalid_args",
        "a save takes no force"
    );

    // Deleting closes it and removes its file.
    let mark = harness.sink.mark();
    harness.on(1, "delete_board", &id);
    assert!(!harness.path(&id).exists());
    assert_eq!(harness.on_code(1, "board_health", &id), "board_not_open");
    let changed = harness.sink.events(mark, "boards_changed");
    assert_eq!(changed[0]["boards"], json!([id]));
    let mut ids = harness.board_ids();
    ids.sort();
    assert_eq!(
        ids,
        [
            ".unfold/boards/order-placement-2.json",
            ".unfold/boards/untitled-board.json"
        ]
    );
}

#[test]
fn a_project_without_a_graph_has_no_board_health() {
    // Adding compiles at once; a project whose `.unfold/local` went away while it rested has
    // no graph until its next Compile.
    let harness = Harness::with("ts-orders", restless());
    eventually("the project rests", || {
        harness.row(&harness.project)["live"] == false
    });
    std::fs::remove_dir_all(harness.root.join(".unfold/local")).expect("remove local");
    harness.write(".unfold/boards/order-placement.json", &sample_board());
    // A teammate's board opens, but health needs a graph: no labels, no Problems.
    let id = ".unfold/boards/order-placement.json";
    assert_eq!(harness.on(1, "open_board", id)["id"], id);
    assert_eq!(harness.listed(id)["label"], Value::Null);
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["boards"], json!([]));
    assert!(!harness.path(".unfold/local").exists(), "nothing was written");
}

#[test]
fn a_board_reads_without_being_opened_or_waking_its_project() {
    let harness = Harness::with("ts-orders", restless());
    harness.ok(9, "compile", json!({ "project": harness.project }));
    let live = || harness.row(&harness.project)["live"] == true;
    eventually("the project rests", || !live());
    let sample = sample_board();
    harness.write(".unfold/boards/order-placement.json", &sample);
    let id = ".unfold/boards/order-placement.json";

    let read = harness.on(1, "read_board", id);
    assert_eq!(read["project"], harness.project.as_str());
    assert_eq!(read["id"], id);
    assert_eq!(read["name"], "order-placement");
    assert_eq!(read["document"]["format"], "unfold-board");
    assert_eq!(read["document"]["title"], "Order placement");
    assert_eq!(read["revision"], revision_of(&sample));
    assert!(!live(), "reading a board file doesn't wake its project");

    // It isn't opened: board commands refuse it until it is.
    assert_eq!(harness.on_code(1, "board_health", id), "board_not_open");
    let opened = harness.on(2, "open_board", id);

    // An edit is written to the file itself.
    let mut edit = edit_of(&opened);
    edit["title"] = json!("Order placement, edited");
    let saved = harness.save(2, id, edit);
    let read = harness.on(1, "read_board", id);
    assert_eq!(read["document"]["title"], "Order placement, edited");
    assert_eq!(read["revision"], saved["revision"]);

    assert_eq!(harness.on_code(1, "read_board", "../x.json"), "invalid_args");
    assert_eq!(
        harness.on_code(1, "read_board", ".unfold/boards/nope.json"),
        "not_found"
    );
    harness.write(".unfold/boards/broken.json", "{");
    assert_eq!(
        harness.on_code(1, "read_board", ".unfold/boards/broken.json"),
        "board_invalid"
    );
    // An old board file (with `projects`) fails like any invalid file.
    harness.write(
        ".unfold/boards/old.json",
        &sample.replace(
            "\"title\": \"Order placement\"",
            "\"projects\": [],\n  \"title\": \"Order placement\"",
        ),
    );
    assert_eq!(
        harness.on_code(1, "read_board", ".unfold/boards/old.json"),
        "board_invalid"
    );
}

#[test]
fn the_boards_list_puts_the_last_edited_first() {
    let harness = Harness::compiled("ts-orders");
    let ids: Vec<String> = ["Alpha", "Beta", "Gamma"]
        .into_iter()
        .map(|title| id_of(&harness.new_board(1, title)))
        .collect();
    // A board's last edit is its file's modification time.
    touch(&harness.path(&ids[0]), 3_000);
    touch(&harness.path(&ids[1]), 1_000);
    touch(&harness.path(&ids[2]), 2_000);
    assert_eq!(
        harness.board_ids(),
        [ids[0].clone(), ids[2].clone(), ids[1].clone()]
    );
    assert_eq!(harness.listed(&ids[0])["updatedAtMs"], 3_000_000);
    // Equal times go by id.
    touch(&harness.path(&ids[2]), 3_000);
    assert_eq!(
        harness.board_ids(),
        [ids[0].clone(), ids[2].clone(), ids[1].clone()]
    );
    // A save is an edit: that board comes first.
    let beta = harness.on(1, "open_board", &ids[1]);
    harness.save(1, &ids[1], edit_of(&beta));
    assert_eq!(harness.board_ids()[0], ids[1]);
    // A file that isn't a board is listed with why.
    harness.write(".unfold/boards/old.json", "{\"projects\": []}");
    touch(&harness.path(".unfold/boards/old.json"), 10);
    let old = harness.listed(".unfold/boards/old.json");
    assert_eq!(old["title"], Value::Null);
    assert!(old["error"].is_string(), "{old}");
    assert_eq!(
        harness.board_ids().last().map(String::as_str),
        Some(".unfold/boards/old.json")
    );
}

#[test]
fn board_refs_find_a_key_on_the_projects_boards() {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    let board = harness.new_board(1, "Refs");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    harness.save(1, &id, edit);
    let refs = harness.ok(
        1,
        "board_refs",
        json!({ "project": harness.project, "key": CREATE_ORDER }),
    );
    assert_eq!(
        refs,
        json!({ "refs": [{ "project": harness.project, "board": id, "title": "Refs", "page": "page-1", "pageTitle": "Page 1" }], "truncated": false })
    );
    let elsewhere = harness.ok(1, "board_refs", json!({ "project": python, "key": CREATE_ORDER }));
    assert_eq!(elsewhere["refs"], json!([]), "another project's boards");
    let absent = harness.ok(
        1,
        "board_refs",
        json!({ "project": harness.project, "key": PLACE_ORDER }),
    );
    assert_eq!(absent["refs"], json!([]));
}

#[test]
fn folders_are_one_level_and_hold_boards_open_or_not() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let folder =
        |command: &str, path: &str| harness.call(1, command, json!({ "project": project, "path": path }));
    let empty = || harness.boards()["emptyFolders"].clone();
    folder("create_folder", "Refunds").expect("create");
    folder("create_folder", "Refunds").expect("creating it again does nothing");
    assert_eq!(empty(), json!(["Refunds"]));
    for bad in ["", "Checkout/Refunds", "a/../b", " padded", "a/", "a\\b", ".."] {
        assert_eq!(
            folder("create_folder", bad).expect_err(bad).code,
            "invalid_folder",
            "{bad}"
        );
    }
    assert_eq!(
        harness.code(
            1,
            "new_board",
            json!({ "project": project, "title": "Nested", "folder": "Checkout/Refunds" })
        ),
        "invalid_folder"
    );

    // A board in an empty folder fills it.
    let inside = harness.ok(
        2,
        "new_board",
        json!({ "project": project, "title": "Inside", "folder": "Refunds" }),
    );
    let inside_id = id_of(&inside);
    assert_eq!(empty(), json!([]));
    let other = harness.ok(
        1,
        "new_board",
        json!({ "project": project, "title": "Other", "folder": "Checkout" }),
    );
    let other_id = id_of(&other);

    // Any board moves, open or not; an open board's next save keeps the folder.
    let mark = harness.sink.mark();
    let moved = harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": inside_id, "folder": "Archive" }),
    );
    assert_eq!(moved["folder"], "Archive");
    assert!(harness.read(&inside_id).contains("\"folder\": \"Archive\""));
    let changed = harness.sink.events(mark, "boards_changed");
    assert_eq!(changed[0]["boards"], json!([inside_id]));
    assert_eq!(changed[0]["folders"], true);
    assert_eq!(empty(), json!(["Refunds"]), "the folder it left stays");
    let mut edit = edit_of(&inside);
    edit["title"] = json!("Inside, renamed");
    let saved = harness.save(2, &inside_id, edit);
    assert_eq!(saved["document"]["folder"], "Archive");
    assert_eq!(saved["document"]["title"], "Inside, renamed");
    assert_eq!(
        harness
            .call(
                1,
                "set_board_folder",
                json!({ "project": project, "id": inside_id, "folder": "Archive/Old" })
            )
            .expect_err("nested")
            .code,
        "invalid_folder"
    );

    // Renaming a folder moves its boards; an empty folder is renamed in place.
    let rename = |from: &str, to: &str| {
        harness.call(
            1,
            "rename_folder",
            json!({ "project": project, "from": from, "to": to }),
        )
    };
    assert_eq!(
        rename("Archive", "Checkout").expect_err("taken").code,
        "folder_exists"
    );
    assert_eq!(rename("Nope", "Other").expect_err("missing").code, "not_found");
    assert_eq!(
        rename("Archive", "Old/Archive").expect_err("nested").code,
        "invalid_folder"
    );
    assert_eq!(rename("Archive", "Old").expect("rename"), json!({ "boards": 1 }));
    assert_eq!(harness.listed(&inside_id)["folder"], "Old");
    assert_eq!(
        rename("Refunds", "Returns").expect("rename"),
        json!({ "boards": 0 })
    );
    assert_eq!(empty(), json!(["Returns"]));

    // Only an empty folder is deleted.
    assert_eq!(
        folder("delete_folder", "Old").expect_err("not empty").code,
        "folder_not_empty"
    );
    folder("delete_folder", "Returns").expect("delete");
    assert_eq!(empty(), json!([]));
    assert_eq!(
        folder("delete_folder", "Returns").expect_err("gone").code,
        "not_found"
    );
    harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": other_id, "folder": null }),
    );
    assert_eq!(harness.listed(&other_id)["folder"], Value::Null);
    assert_eq!(empty(), json!(["Checkout"]), "the folder it left stays");

    // Deleting the last board of a folder keeps the folder.
    harness.on(2, "delete_board", &inside_id);
    assert_eq!(empty(), json!(["Checkout", "Old"]));
}

/// An open board whose file was deleted outside keeps the person's version, which moves to
/// another folder like any board; a closed board with no file is `not_found`.
#[test]
fn an_open_board_deleted_outside_still_moves() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let board = harness.ok(
        1,
        "new_board",
        json!({ "project": project, "title": "Gone", "folder": "Checkout" }),
    );
    let id = id_of(&board);
    std::fs::remove_file(harness.path(&id)).expect("delete");
    let moved = harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": id, "folder": "Archive" }),
    );
    assert_eq!(moved["folder"], "Archive");
    let written: Value = serde_json::from_str(&harness.read(&id)).expect("board JSON");
    assert_eq!(written["title"], "Gone");
    assert_eq!(written["folder"], "Archive");
    let mut edit = edit_of(&board);
    edit["title"] = json!("Gone, renamed");
    assert_eq!(harness.save(1, &id, edit)["document"]["folder"], "Archive");

    harness.on(1, "close_board", &id);
    std::fs::remove_file(harness.path(&id)).expect("delete");
    assert_eq!(
        harness.code(
            1,
            "set_board_folder",
            json!({ "project": project, "id": id, "folder": null })
        ),
        "not_found"
    );
}

#[test]
fn an_outside_edit_reloads_the_open_board() {
    let harness = Harness::new("ts-orders", true);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.new_board(1, "Outside");
    let id = id_of(&board);

    // An agent rewrites the file: the session reloads, and the window that opened the board is
    // told with the board as it is now.
    let mut document = board["document"].clone();
    document["title"] = json!("Rewritten by an agent");
    document["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let agent = serde_json::to_string_pretty(&document).expect("json");
    write_atomically(&harness.path(&id), &agent);
    let message = harness.reload(&id, "the reload of the agent's version", |message| {
        message["event"]["state"]["revision"] == revision_of(&agent)
    });
    assert_eq!(message["kind"], "event");
    assert_eq!(message["windows"], json!([1]));
    let event = &message["event"];
    assert_eq!(event["project"], harness.project.as_str());
    let state = &event["state"];
    assert_eq!(state["id"], id.as_str());
    assert_eq!(state["document"]["title"], "Rewritten by an agent");
    assert_eq!(state["document"]["pages"][0]["cards"][0]["id"], "create");
    assert_eq!(state["health"]["items"], json!([]));

    // The next open shows the new content.
    let reopened = harness.on(1, "open_board", &id);
    assert_eq!(reopened["document"]["title"], "Rewritten by an agent");
    assert_eq!(reopened["revision"], revision_of(&agent));

    // A window whose save was failing ignores the reload; its Retry writes the person's version
    // over the agent's.
    let retried = harness.save(1, &id, edit_of(&board));
    assert_eq!(retried["document"]["title"], "Outside");
    assert!(harness.read(&id).contains("\"Outside\""));
    assert!(
        harness.reloads(&id).len() == 1,
        "the app's own write is not an outside edit"
    );

    // A deleted file is reported once, with no board; the next save writes it back.
    std::fs::remove_file(harness.path(&id)).expect("delete");
    let gone = harness.reload(&id, "the delete", |message| message["event"]["state"].is_null());
    assert_eq!(gone["windows"], json!([1]));
    let back = harness.save(1, &id, edit_of(&retried));
    assert_eq!(back["revision"], revision_of(&harness.read(&id)));
}

/// A window whose save is failing ignores a reload; its Retry is checked against the version it
/// was editing on, so a problem that version already had doesn't refuse it, even when the
/// outside change fixed it.
#[test]
fn a_retry_after_an_ignored_reload_keeps_the_persons_problems() {
    let harness = Harness::new("ts-orders", true);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.new_board(1, "Retry");
    let id = id_of(&board);
    const GHOST: &str = "src/routes/orders.ts#function:noSuchThing";

    // An agent writes a card whose code is missing; the window takes the reload.
    let mut document = board["document"].clone();
    document["pages"][0]["cards"] = json!([card("ghost", GHOST)]);
    let broken = serde_json::to_string_pretty(&document).expect("json");
    write_atomically(&harness.path(&id), &broken);
    let taken = harness.reload(&id, "the agent's version", |message| {
        message["event"]["state"]["revision"] == revision_of(&broken)
    });
    let taken = &taken["event"]["state"];
    assert_eq!(taken["health"]["items"][0]["kind"], "missing");

    // The person edits it, and the save fails (unseen here). Meanwhile the agent fixes the card,
    // and the window ignores that reload.
    let mut person = edit_of(taken);
    person["title"] = json!("Retry, renamed");
    document["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let fixed = serde_json::to_string_pretty(&document).expect("json");
    write_atomically(&harness.path(&id), &fixed);
    harness.reload(&id, "the agent's fix", |message| {
        message["event"]["state"]["revision"] == revision_of(&fixed)
    });

    // A new problem is still refused; the person's version, with the problem it had, is written.
    let mut worse = person.clone();
    worse["pages"][0]["cards"] = json!([
        card("ghost", GHOST),
        card("other", "src/routes/orders.ts#function:nope")
    ]);
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": worse })
        ),
        "board_problem"
    );
    let retried = harness.save(1, &id, person.clone());
    assert_eq!(retried["document"]["title"], "Retry, renamed");
    assert_eq!(retried["document"]["pages"][0]["cards"][0]["id"], "ghost");
    assert!(harness.read(&id).contains("Retry, renamed"));

    // Once saved, the replaced versions are forgotten: the session is the person's version.
    let mut next = edit_of(&retried);
    next["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    harness.save(1, &id, next);
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": person })
        ),
        "board_problem",
        "the ghost card is new again"
    );
}

/// A writer that truncates the file and writes it in pieces is caught mid-write: the board
/// reloads only once it is finished, never from the empty or half-written file.
#[test]
fn a_non_atomic_writer_reloads_the_board_once_it_finishes() {
    let harness = Harness::new("ts-orders", true);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.new_board(1, "Outside");
    let id = id_of(&board);
    let mut document = board["document"].clone();
    document["title"] = json!("Written in pieces");
    let text = serde_json::to_string_pretty(&document).expect("json");
    let path = harness.path(&id);
    // Each piece lands well after the watcher's quiet period, so a check sees each one.
    let pause = Duration::from_millis(150);
    std::fs::write(&path, "").expect("truncate");
    std::thread::sleep(pause);
    std::fs::write(&path, &text[..text.len() / 2]).expect("first half");
    std::thread::sleep(pause);
    std::fs::write(&path, &text).expect("whole board");

    harness.reload(&id, "the reload of the whole board", |message| {
        message["event"]["state"]["revision"] == revision_of(&text)
    });
    // Checks run in order, so every reload of this board has arrived by now.
    let reloaded: Vec<Value> = harness
        .reloads(&id)
        .into_iter()
        .map(|message| message["event"]["state"]["revision"].clone())
        .collect();
    assert_eq!(
        reloaded,
        [json!(revision_of(&text))],
        "one reload, of the whole board"
    );
    assert_eq!(
        harness.on(1, "open_board", &id)["document"]["title"],
        "Written in pieces"
    );
}

#[test]
fn frames_resources_connections_and_notes_are_saved_as_sent() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.new_board(1, "System");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"][0]["defaultView"] = json!("diagram");
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER), card("place", PLACE_ORDER)]);
    edit["pages"][0]["groups"] = json!([{
        "author": "agent:claude", "card": "create", "collapsed": false, "id": "queue-it",
        "members": ["src/routes/orders.ts#function:createOrder/if[0]"], "title": "Queue it"
    }]);
    edit["pages"][0]["resources"] = json!([{
        "anchors": [{ "key": "src/queues/queues.ts#variable:ordersQueue" }], "at": [400.0, 0.0],
        "author": "agent:claude", "id": "orders", "kind": "queue", "name": "orders"
    }]);
    edit["pages"][0]["frames"] = json!([{
        "author": "agent:claude", "id": "server", "look": "box", "members": ["create", "place"], "title": "server"
    }]);
    edit["pages"][0]["links"] = json!([{
        "anchors": [{ "key": "src/queues/queues.ts#variable:ordersQueue" }], "author": "agent:claude",
        "channel": "orders", "from": "create", "id": "enqueue", "kind": "queue", "to": "orders",
        "step": "src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]"
    }]);
    edit["pages"][0]["notes"] = json!([{
        "at": [0.0, -40.0], "attach": "enqueue", "author": "agent:claude", "id": "note", "text": "One job per order."
    }]);
    let saved = harness.save(1, &id, edit.clone());
    let page = &saved["document"]["pages"][0];
    for field in ["frames", "notes"] {
        assert_eq!(page[field], edit["pages"][0][field], "{field} carried through");
    }
    // The claims come back signed over the code they cite, and otherwise as sent.
    for field in ["resources", "links"] {
        let mut items = page[field].clone();
        for item in items.as_array_mut().expect("items") {
            let signature = item
                .as_object_mut()
                .and_then(|item| item.remove("signature"))
                .expect("signed");
            assert!(signature.as_str().is_some_and(|text| text.starts_with("sha256:")));
        }
        assert_eq!(items, edit["pages"][0][field], "{field} carried through");
    }
    assert_eq!(page["groups"][0]["author"], "agent:claude");
    assert_eq!(
        saved["health"]["items"],
        json!([]),
        "a link to a resource isn't broken"
    );
    let written = harness.read(&id);
    assert!(written.contains("\"kind\": \"queue\""), "{written}");
    assert!(written.contains("\"defaultView\": \"diagram\""), "{written}");

    // A window that removes an item removes what refers to it too.
    let mut dangling = edit;
    dangling["pages"][0]["links"] = json!([]);
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": dangling })
        ),
        "board_invalid",
        "a note attached to a removed link"
    );
}

#[test]
fn pages_cards_and_drawn_links_are_checked_and_signed() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.new_board(1, "Pages");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"] = json!([
        {
            "id": "page-1", "title": "Placing", "defaultView": "flow", "groups": [],
            "cards": [card("create", CREATE_ORDER), card("place", PLACE_ORDER)],
            "links": [{ "id": "create-place", "from": "create", "to": "place", "label": text("hands the draft over") }],
        },
        {
            "id": "page-2", "title": "The route", "defaultView": "code", "groups": [],
            "cards": [card("route", POST_ORDERS)],
        },
    ]);
    let saved = harness.save(1, &id, edit.clone());
    let pages = saved["document"]["pages"].as_array().expect("pages");
    assert_eq!(pages.len(), 2);
    let link = &pages[0]["links"][0];
    assert!(
        link["label"]["signature"]
            .as_str()
            .is_some_and(|signature| signature.starts_with("sha256:")),
        "a link label is signed over both ends: {link}"
    );
    assert_eq!(saved["health"]["items"], json!([]));

    // What a save refuses.
    let mut elsewhere = edit.clone();
    elsewhere["pages"][1]["cards"][0]["ref"]["project"] = json!("py-orders");
    let mut dangling = edit.clone();
    dangling["pages"][0]["links"][0]["to"] = json!("nowhere");
    let mut missing = edit.clone();
    missing["pages"][1]["cards"] = json!([card("ghost", "src/routes/orders.ts#function:noSuchThing")]);
    let mut twice = edit.clone();
    twice["pages"][1]["id"] = json!("page-1");
    for (bad, code) in [
        (elsewhere, "board_invalid"),
        (dangling, "board_invalid"),
        (missing, "board_problem"),
        (twice, "board_invalid"),
        (json!({ "title": "x" }), "board_invalid"),
        (
            json!({ "title": "x", "pages": [], "folder": "Sneaky" }),
            "board_invalid",
        ),
    ] {
        assert_eq!(
            harness.code(
                1,
                "save_board",
                json!({ "project": harness.project, "id": id, "board": bad })
            ),
            code,
            "{bad}"
        );
    }

    // Repairs name their page.
    let repair = |repair: Value| {
        harness.call(
            1,
            "repair_board",
            json!({ "project": harness.project, "id": id, "repair": repair }),
        )
    };
    let repaired =
        repair(json!({ "page": "page-1", "kind": "remove_link", "link": "create-place" })).expect("repair");
    assert_eq!(repaired["document"]["pages"][0]["links"], json!([]));
    assert_eq!(
        repair(json!({ "page": "page-9", "kind": "remove_card", "card": "create" }))
            .expect_err("no such page")
            .code,
        "invalid_args"
    );
    let repaired =
        repair(json!({ "page": "page-2", "kind": "remove_card", "card": "route" })).expect("repair");
    assert_eq!(repaired["document"]["pages"][1]["cards"], json!([]));
}

#[test]
fn closing_a_window_waits_for_its_last_board_save() {
    // The window's final save and its detach race: the save is dispatched, then the detach
    // follows at once on another channel. The detach must let the save land.
    for round in 0..5 {
        let harness = Harness::compiled("ts-orders");
        let board = harness.new_board(1, &format!("Closing {round}"));
        let id = id_of(&board);
        let mut edit = edit_of(&board);
        edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
        let (sender, receiver) = std::sync::mpsc::channel();
        harness.host.dispatch_async(
            1,
            harness.request(),
            json!({ "command": "save_board", "args": { "project": harness.project, "id": id, "board": edit } })
                .to_string(),
            move |result| {
                let _ = sender.send(result);
            },
        );
        harness.host.detach_window(1).expect("detach");
        let saved = receiver
            .recv_timeout(PATIENCE)
            .expect("the save finishes")
            .unwrap_or_else(|error| panic!("the save failed: {error}"));
        assert!(saved.contains("createOrder"), "{saved}");
        let written: Value = serde_json::from_str(&harness.read(&id)).expect("board JSON");
        assert_eq!(written["pages"][0]["cards"][0]["id"], "create", "round {round}");
        // After the detach, the window's boards are closed.
        assert_eq!(harness.on_code(1, "board_health", &id), "board_not_open");
    }
}

#[test]
fn health_reaches_problems_and_repairs_fix_it() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.new_board(1, "Health");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    edit["pages"][0]["cards"][0]["description"] = text("Places the order.");
    let saved = harness.save(1, &id, edit);
    let signature = saved["document"]["pages"][0]["cards"][0]["description"]["signature"]
        .as_str()
        .expect("signature");
    assert!(signature.starts_with("sha256:"), "{signature}");
    assert!(saved["label"].is_null(), "a healthy board has no label");

    // Changing the described code makes the description outdated after the next compile.
    let path = harness.path("src/routes/orders.ts");
    let source = std::fs::read_to_string(&path).expect("read");
    let changed = source.replacen(
        "export async function createOrder(",
        "export async function createOrder(\n  ",
        1,
    );
    assert_ne!(source, changed, "the playground still has createOrder");
    std::fs::write(&path, changed).expect("write");
    harness.ok(1, "compile", json!({ "project": harness.project }));
    eventually("board health reaches Problems", || {
        let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
        problems
            .as_array()
            .expect("problems")
            .iter()
            .any(|problem| problem["category"] == "board" && problem["path"] == id.as_str())
    });
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(
        summary["health"]["boards"],
        json!([{ "id": id, "title": "Health", "items": 1 }])
    );
    assert_eq!(summary["health"]["boardsNeedingReview"], 1);
    assert_eq!(harness.listed(&id)["label"], "1 outdated");
    let health = harness.on(1, "board_health", &id);
    assert_eq!(health["health"]["outdated"], 1);
    assert_eq!(health["health"]["items"][0]["page"], "page-1");
    assert_eq!(health["label"], "1 outdated");

    // The Resign repair signs it again, and Problems follow.
    let repaired = harness.ok(
        1,
        "repair_board",
        json!({ "project": harness.project, "id": id, "repair": { "page": "page-1", "kind": "resign", "target": { "card": "create" } } }),
    );
    assert_eq!(repaired["health"]["outdated"], 0);
    let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
    assert!(
        problems
            .as_array()
            .expect("problems")
            .iter()
            .all(|problem| problem["category"] != "board")
    );
}
