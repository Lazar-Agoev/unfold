//! Boards (contract §3.5, §4.6, §4.7, §6): claims across windows, pages, cards from other
//! projects and placeholders, drawn links, unpublished changes and Publish (conflicts, keep
//! mine, discard), Move back to drafts, folders, view state, outside edits per working file,
//! drafts and references across projects, thumbnails, and export.

use std::path::PathBuf;

use serde_json::{Value, json};
use unfold_workspace::ProjectId;

use crate::common::{Harness, PATIENCE, eventually};

const CREATE_ORDER: &str = "src/routes/orders.ts#function:createOrder";
const PLACE_ORDER: &str = "src/services/order-service.ts#class:OrderService/method:place";
const POST_ORDERS: &str = r#"entrance:["express.route","http","POST","/orders","requireUser"]"#;
const HEALTH_CHECK: &str = "src/orders_playground/server.py#function:health_check";
const CHECKOUT: &str = r#"entrance:["next.route","http","POST","/api/cart/checkout"]"#;

/// A card for block or entrance `key` of the board's own project.
fn card(id: &str, key: &str) -> Value {
    let kind = if key.starts_with("entrance:") {
        "entrance"
    } else {
        "block"
    };
    json!({ "id": id, "ref": { kind: key }, "at": [0, 0] })
}

/// A card for `key` of the project shared as `project`.
fn foreign(id: &str, key: &str, project: &str) -> Value {
    let mut card = card(id, key);
    card["ref"]["project"] = json!(project);
    card
}

/// An unsigned description by a tester.
fn text(words: &str) -> Value {
    json!({ "text": words, "author": "person:tester", "signature": "" })
}

/// What a window saves of a board it shows: its title and pages.
fn edit_of(state: &Value) -> Value {
    json!({ "title": state["document"]["title"], "pages": state["document"]["pages"] })
}

fn id_of(state: &Value) -> String {
    state["id"].as_str().expect("a board id").to_owned()
}

fn project_id(text: &str) -> ProjectId {
    serde_json::from_value(json!(text)).expect("a project id")
}

/// The revision string of `text`, as results and events carry it.
fn revision_of(text: &str) -> String {
    unfold_core::ContentHash::of(text.as_bytes()).to_string()
}

impl Harness {
    /// A new draft titled `title` in the first project, held by `window`.
    fn draft(&self, window: u32, title: &str) -> Value {
        self.ok(
            window,
            "new_board",
            json!({ "project": self.project, "title": title }),
        )
    }

    /// Saves `edit` as board `id` of the first project from `window`.
    fn save(&self, window: u32, id: &str, edit: Value) -> Value {
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

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.path(relative)).expect("read a board file")
    }

    /// Waits for the `board_conflict` about `id` that reports `revision` on disk.
    fn conflict_at(&self, id: &str, revision: &str) -> Value {
        let find = || {
            self.sink.all().into_iter().find(|message| {
                message["event"]["type"] == "board_conflict"
                    && message["event"]["board"] == id
                    && message["event"]["diskRevision"] == revision
            })
        };
        eventually("a board_conflict with the file's final revision", || {
            find().is_some()
        });
        find().expect("the conflict")
    }
}

/// Replaces `path` the way well-behaved writers do: a temporary file renamed over it.
fn write_atomically(path: &std::path::Path, text: &str) {
    let temporary = path.with_extension("agent-tmp");
    std::fs::write(&temporary, text).expect("write");
    std::fs::rename(&temporary, path).expect("rename");
}

#[test]
fn a_board_is_open_in_one_window_at_a_time() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let board = harness.ok(
        1,
        "new_board",
        json!({ "project": project, "title": "Order placement", "folder": "Checkout", "pageTitle": "Placing" }),
    );
    let id = id_of(&board);
    assert!(id.starts_with(".unfold/local/drafts/board-"), "{id}");
    assert_eq!(board["project"], project.as_str());
    assert_eq!(board["place"], "draft");
    assert_eq!(board["published"], Value::Null);
    let document = &board["document"];
    assert_eq!(document["format"], "unfold-board");
    assert_eq!(document["version"], 1);
    assert_eq!(document["folder"], "Checkout");
    assert_eq!(document["projects"], json!([]));
    assert_eq!(document["pages"][0]["id"], "page-1");
    assert_eq!(document["pages"][0]["title"], "Placing");
    assert!(harness.path(&id).is_file());

    // Window 2 is told who holds it, and can't touch it.
    assert_eq!(
        harness.on(2, "open_board", &id),
        json!({ "status": "held", "heldBy": 1 })
    );
    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let save = json!({ "project": project, "id": id, "board": edit });
    assert_eq!(harness.code(2, "save_board", save.clone()), "board_held");
    let saved = harness.ok(1, "save_board", save);
    let page = &saved["document"]["pages"][0];
    assert_eq!(page["cards"][0]["id"], "create");
    assert_eq!(page["guards"][0]["key"], CREATE_ORDER, "guards are written");
    assert_eq!(saved["document"]["folder"], "Checkout", "the folder is Rust's");
    assert_ne!(saved["revision"], board["revision"]);
    let row = {
        let list = harness.ok(2, "list_boards", json!({ "project": project }));
        list["boards"][0].clone()
    };
    assert_eq!(row["id"], id.as_str());
    assert_eq!(row["heldBy"], 1);
    assert_eq!((row["pages"].clone(), row["cards"].clone()), (json!(1), json!(1)));
    assert_eq!(row["title"], "Order placement");
    assert_eq!(row["folder"], "Checkout");
    assert_eq!(row["changes"], 0);
    assert_eq!(row["label"], Value::Null);
    assert!(row["updatedAtMs"].as_i64().is_some_and(|at| at > 0));

    // Open in New Window hands the claim over.
    harness.ok(
        1,
        "move_board",
        json!({ "project": project, "id": id, "toWindow": 2 }),
    );
    assert_eq!(harness.on_code(1, "board_health", &id), "board_held");
    let health = harness.on(2, "board_health", &id);
    assert_eq!(health["outlines"][0]["page"], "page-1");
    assert_eq!(health["outlines"][0]["rows"][0]["card"], "create");
    assert_eq!(health["label"], Value::Null);
    assert_eq!(
        harness.code(
            1,
            "move_board",
            json!({ "project": project, "id": id, "toWindow": 1 })
        ),
        "board_held"
    );

    // Closing window 2 releases it; window 1 opens it again and it becomes a recent board.
    harness.host.detach_window(2).expect("detach");
    let reopened = harness.on(1, "open_board", &id);
    assert_eq!(reopened["status"], "opened");
    assert_eq!(
        reopened["board"]["document"]["pages"][0]["cards"][0]["id"],
        "create"
    );
    let recent = harness.ok(1, "workspace", json!({}))["recentBoards"][0].clone();
    assert_eq!(recent["board"], id.as_str());
    assert_eq!(recent["project"], project.as_str());
    assert_eq!(recent["title"], "Order placement");

    // Closing it releases it.
    harness.on(1, "close_board", &id);
    assert_eq!(harness.on_code(1, "board_health", &id), "board_not_held");
    assert_eq!(harness.on_code(1, "close_board", &id), "board_not_held");
    assert_eq!(harness.on_code(1, "open_board", "../x.json"), "invalid_args");
    assert_eq!(
        harness.on_code(1, "open_board", ".unfold/boards/nope.json"),
        "not_found"
    );
    assert_eq!(
        harness.on_code(2, "open_board", ".unfold/boards/nope.json"),
        "not_found",
        "a board that can't be opened isn't claimed"
    );
}

#[test]
fn a_project_without_a_graph_has_no_drafts_and_no_board_health() {
    let harness = Harness::new("ts-orders", false);
    let sample = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/boards/order-placement.json"),
    )
    .expect("the sample board");
    harness.write(".unfold/boards/order-placement.json", &sample);
    assert_eq!(
        harness.code(1, "new_board", json!({ "project": harness.project })),
        "not_compiled"
    );
    // A teammate's published board opens, but health needs a graph: no labels, no Problems.
    let id = ".unfold/boards/order-placement.json";
    assert_eq!(harness.on(1, "open_board", id)["status"], "opened");
    assert_eq!(harness.listed(id)["label"], Value::Null);
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["boards"], json!([]));
    assert!(!harness.path(".unfold/local").exists(), "nothing was written");
}

#[test]
fn pages_cards_and_drawn_links_are_checked_and_signed() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.draft(1, "Pages");
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
    let outlines = saved["outlines"].as_array().expect("outlines");
    assert_eq!(outlines.len(), 2, "one outline per page");
    assert_eq!(
        outlines[0]["rows"],
        json!([{ "card": "create", "depth": 0 }, { "card": "place", "depth": 1 }])
    );
    assert_eq!(outlines[1]["rows"][0]["card"], "route");

    // What a save refuses.
    let mut own = edit.clone();
    own["pages"][1]["cards"] = json!([foreign("route", POST_ORDERS, "ts-orders")]);
    assert_eq!(
        harness.code(
            1,
            "save_board",
            json!({ "project": harness.project, "id": id, "board": own })
        ),
        "invalid_args",
        "a ref names the board's own project only by omitting it"
    );
    let mut dangling = edit.clone();
    dangling["pages"][0]["links"][0]["to"] = json!("nowhere");
    let mut missing = edit.clone();
    missing["pages"][1]["cards"] = json!([card("ghost", "src/routes/orders.ts#function:noSuchThing")]);
    let mut twice = edit.clone();
    twice["pages"][1]["id"] = json!("page-1");
    for (bad, code) in [
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

    // Export: one page at a time.
    let mermaid = harness.ok(
        1,
        "export_mermaid_text",
        json!({ "project": harness.project, "board": { "id": id, "page": "page-1" } }),
    );
    let text = mermaid["text"].as_str().expect("text");
    assert!(text.starts_with("flowchart LR"), "{text}");
    assert!(text.contains("createOrder"), "{text}");
    assert_eq!(
        harness.code(
            1,
            "export_mermaid_text",
            json!({ "project": harness.project, "board": { "id": id, "page": "page-9" } })
        ),
        "not_found"
    );
    assert_eq!(
        harness.code(
            2,
            "export_mermaid_text",
            json!({ "project": harness.project, "board": { "id": id, "page": "page-1" } })
        ),
        "board_held"
    );
    assert_eq!(
        harness.code(
            1,
            "export_mermaid_text",
            json!({ "project": harness.project, "board": { "id": id, "page": "page-1" }, "entrance": POST_ORDERS })
        ),
        "invalid_args"
    );
    let entrance = harness.ok(
        1,
        "export_mermaid_text",
        json!({ "project": harness.project, "entrance": POST_ORDERS }),
    );
    assert!(
        entrance["text"]
            .as_str()
            .is_some_and(|text| text.contains("POST /orders"))
    );
}

#[test]
fn cards_from_other_projects_and_placeholders() {
    let harness = Harness::compiled("ts-orders");
    let (python, python_root) = harness.add("py-orders", "py-orders");
    harness.ok(9, "compile", json!({ "project": python }));
    let board = harness.draft(1, "Across");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([
        card("create", CREATE_ORDER),
        foreign("health", HEALTH_CHECK, "py-orders"),
        foreign("checkout", CHECKOUT, "storefront-web"),
    ]);
    edit["pages"][0]["links"] = json!([
        { "id": "checkout-create", "from": "checkout", "to": "create" },
        { "id": "health-create", "from": "health", "to": "create", "label": text("checks it") },
    ]);
    let saved = harness.save(1, &id, edit);
    let document = &saved["document"];
    assert_eq!(document["projects"], json!(["py-orders", "storefront-web"]));
    let guards = document["pages"][0]["guards"].as_array().expect("guards");
    assert!(
        guards
            .iter()
            .any(|guard| guard["project"] == "py-orders" && guard["key"] == HEALTH_CHECK),
        "a connected project's references are guarded: {guards:?}"
    );
    assert!(
        guards.iter().all(|guard| guard["project"] != "storefront-web"),
        "nothing is known of a project that is not connected"
    );
    assert!(
        document["pages"][0]["links"][1]["label"]["signature"]
            .as_str()
            .is_some_and(|signature| signature.starts_with("sha256:"))
    );
    // The placeholder is a health item, never a save error.
    let items = saved["health"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["kind"], "notConnected");
    assert_eq!(items[0]["page"], "page-1");
    assert_eq!(items[0]["target"], json!({ "card": "checkout" }));
    assert_eq!(saved["label"], "1 not connected");

    // The open board keeps the project its cards come from live.
    let projects = harness.host.projects().expect("initialized");
    let own = projects.handle(&project_id(&harness.project)).expect("project");
    assert!(own.claims().references(&project_id(&python)));

    // Thumbnails, references, and export know which project each card is from.
    let thumbnails = harness.ok(
        1,
        "board_thumbnails",
        json!({ "project": harness.project, "boards": [id] }),
    );
    let cards = thumbnails["thumbnails"][0]["cards"].as_array().expect("cards");
    let shown = |card: &str| {
        cards
            .iter()
            .find(|shown| shown["id"] == card)
            .cloned()
            .unwrap_or_else(|| panic!("no {card} in {cards:?}"))
    };
    assert_eq!(shown("create")["foreign"], false);
    assert!(shown("create")["lines"].as_u64().is_some_and(|lines| lines > 1));
    assert_eq!(shown("health")["foreign"], true);
    assert!(shown("health")["lines"].as_u64().is_some_and(|lines| lines > 1));
    assert_eq!(shown("checkout")["kind"], "entrance");
    assert_eq!(shown("checkout")["lines"], Value::Null);
    assert_eq!(
        thumbnails["thumbnails"][0]["links"],
        json!([["checkout", "create"], ["health", "create"]])
    );
    let refs = harness.ok(1, "board_refs", json!({ "project": python, "key": HEALTH_CHECK }));
    assert_eq!(
        refs,
        json!({ "refs": [{ "project": harness.project, "board": id, "title": "Across", "page": "page-1", "pageTitle": "Page 1" }], "truncated": false })
    );
    let own_refs = harness.ok(
        1,
        "board_refs",
        json!({ "project": harness.project, "key": HEALTH_CHECK }),
    );
    assert_eq!(own_refs["refs"], json!([]), "the key is another project's");
    let mermaid = harness.ok(
        1,
        "export_mermaid_text",
        json!({ "project": harness.project, "board": { "id": id, "page": "page-1" } }),
    );
    let text = mermaid["text"].as_str().expect("text");
    assert!(text.contains("From storefront-web, not connected"), "{text}");
    assert!(
        text.contains("-.->|\"checks it\"|"),
        "drawn links are dashed: {text}"
    );

    // A change to the other project's graph reaches this board's health.
    let server = python_root.join("src/orders_playground/server.py");
    let source = std::fs::read_to_string(&server).expect("read");
    std::fs::write(&server, source.replace("def health_check(", "def health_probe(")).expect("write");
    harness.ok(9, "compile", json!({ "project": python }));
    eventually("the board's health follows the other project", || {
        let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
        problems.as_array().expect("problems").iter().any(|problem| {
            problem["category"] == "board"
                && problem["path"] == id.as_str()
                && problem["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("missing reference"))
        })
    });
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(
        summary["health"]["boards"],
        json!([{ "id": id, "title": "Across", "items": 3 }]),
        "not connected, missing, and the link to the missing card"
    );
}

#[test]
fn published_boards_keep_unpublished_changes_until_publish() {
    let harness = Harness::compiled("ts-orders");
    let draft = harness.draft(1, "Order placement");
    let draft_id = id_of(&draft);
    let view = json!({ "page": "page-1", "pages": { "page-1": { "x": 10.0, "y": -4.5, "zoom": 0.8, "lens": "flow" } } });
    harness.ok(
        1,
        "save_board_view",
        json!({ "project": harness.project, "id": draft_id, "view": view }),
    );
    assert_eq!(harness.on(1, "open_board", &draft_id)["board"]["view"], view);

    // The first Publish moves the board, its claim, view state, and recent entry.
    let published = harness.on(1, "publish_board", &draft_id);
    assert_eq!(published["status"], "done");
    let state = &published["board"];
    let id = id_of(state);
    assert_eq!(id, ".unfold/boards/order-placement.json");
    assert_eq!(state["place"], "published");
    assert_eq!(state["name"], "order-placement");
    assert_eq!(state["view"], view);
    let original = harness.read(&id);
    assert_eq!(
        state["published"],
        json!({ "revision": revision_of(&original), "changes": 0, "baseChanged": false })
    );
    assert!(!harness.path(&draft_id).exists(), "the draft is gone");
    assert_eq!(harness.on_code(1, "board_health", &draft_id), "board_not_held");
    let recent = harness.ok(1, "workspace", json!({}))["recentBoards"][0].clone();
    assert_eq!(recent["board"], id.as_str());

    // A second board with that title is offered a free name; only a draft takes a name.
    let second = harness.draft(1, "Order placement");
    let taken = harness.on(1, "publish_board", &id_of(&second));
    assert_eq!(
        taken,
        json!({ "status": "name_taken", "path": id, "suggestion": "order-placement-2" })
    );
    let named = harness.ok(
        1,
        "publish_board",
        json!({ "project": harness.project, "id": id_of(&second), "name": "order-placement-2" }),
    );
    assert_eq!(named["board"]["id"], ".unfold/boards/order-placement-2.json");
    assert_eq!(
        harness.code(
            1,
            "publish_board",
            json!({ "project": harness.project, "id": id, "name": "x" })
        ),
        "invalid_args"
    );

    // An edit is an unpublished change, in a changes copy: the published file stays as it was.
    let copy = ".unfold/local/drafts/order-placement.changes.json";
    let mut edit = edit_of(state);
    edit["title"] = json!("Placing orders");
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    let changed = harness.save(1, &id, edit.clone());
    assert_eq!(changed["id"], id.as_str(), "the board keeps its id");
    assert_eq!(
        changed["published"],
        json!({ "revision": revision_of(&original), "changes": 2, "baseChanged": false })
    );
    assert_eq!(harness.read(&id), original);
    let copy_text = harness.read(copy);
    assert_eq!(changed["revision"], revision_of(&copy_text));
    let copy_json: Value = serde_json::from_str(&copy_text).expect("json");
    assert_eq!(copy_json["publishes"], "boards/order-placement.json");
    assert_eq!(copy_json["baseRevision"], revision_of(&original));
    let row = harness.listed(&id);
    assert_eq!(
        (row["changes"].clone(), row["title"].clone()),
        (json!(2), json!("Placing orders"))
    );
    let drafts = harness.ok(1, "drafts", json!({}));
    let listed: Vec<&Value> = drafts["drafts"]
        .as_array()
        .expect("drafts")
        .iter()
        .map(|row| &row["id"])
        .collect();
    assert_eq!(
        listed,
        [&json!(id)],
        "drafts holds the board with changes, not the published ones without"
    );

    // Undoing the edit removes the copy.
    let back = harness.save(1, &id, edit_of(state));
    assert_eq!(back["published"]["changes"], 0);
    assert_eq!(back["revision"], revision_of(&original));
    assert!(!harness.path(copy).exists());

    // Publish writes the changes over the published file.
    harness.save(1, &id, edit.clone());
    let done = harness.on(1, "publish_board", &id);
    assert_eq!(done["status"], "done");
    assert!(harness.read(&id).contains("Placing orders"));
    assert!(!harness.path(copy).exists());
    assert_eq!(done["board"]["published"]["changes"], 0);
    assert_eq!(done["board"]["revision"], revision_of(&harness.read(&id)));
    // Publishing a board without changes does nothing.
    assert_eq!(harness.on(1, "publish_board", &id)["status"], "done");

    // A pull changes the published file under the changes: Publish asks.
    let mut more = edit.clone();
    more["pages"][0]["cards"] = json!([card("create", CREATE_ORDER), card("place", PLACE_ORDER)]);
    harness.save(1, &id, more.clone());
    let theirs = harness.read(&id).replace("Placing orders", "Their title");
    std::fs::write(harness.path(&id), &theirs).expect("pull");
    assert_eq!(
        harness.on(1, "publish_board", &id),
        json!({ "status": "conflict", "theirsRevision": revision_of(&theirs) })
    );
    let reloaded = harness.on(1, "reload_board", &id);
    assert_eq!(reloaded["published"]["baseChanged"], true);
    let kept = harness.ok(
        1,
        "publish_board",
        json!({ "project": harness.project, "id": id, "resolve": "keep_mine" }),
    );
    assert_eq!(kept["status"], "done");
    assert!(harness.read(&id).contains("Placing orders"));

    // A deleted published file is a conflict too; keeping mine recreates it.
    more["title"] = json!("Placing orders again");
    harness.save(1, &id, more.clone());
    std::fs::remove_file(harness.path(&id)).expect("delete");
    assert_eq!(
        harness.on(1, "publish_board", &id),
        json!({ "status": "conflict", "theirsRevision": null })
    );
    harness.ok(
        1,
        "publish_board",
        json!({ "project": harness.project, "id": id, "resolve": "keep_mine" }),
    );
    assert!(harness.read(&id).contains("Placing orders again"));

    // Discard drops the changes and returns to the published file.
    let mut discard = more.clone();
    discard["title"] = json!("Never published");
    harness.save(1, &id, discard);
    assert!(harness.path(copy).exists());
    let discarded = harness.on(1, "discard_changes", &id);
    assert_eq!(discarded["document"]["title"], "Placing orders again");
    assert_eq!(discarded["published"]["changes"], 0);
    assert!(!harness.path(copy).exists());
    let fresh = harness.draft(1, "Fresh");
    assert_eq!(
        harness.on_code(1, "discard_changes", &id_of(&fresh)),
        "invalid_args"
    );
    assert_eq!(
        harness.on_code(1, "unpublish_board", &id_of(&fresh)),
        "invalid_args"
    );
}

#[test]
fn moving_back_to_drafts_and_deleting() {
    let harness = Harness::compiled("ts-orders");
    let draft = harness.draft(1, "Order placement");
    let published = harness.on(1, "publish_board", &id_of(&draft));
    let id = id_of(&published["board"]);
    let mut edit = edit_of(&published["board"]);
    edit["pages"][0]["cards"] = json!([card("create", CREATE_ORDER)]);
    harness.save(1, &id, edit);
    harness.on(1, "open_board", &id);

    // The working version (with its unpublished changes) becomes a draft with a new id.
    let moved = harness.on(1, "unpublish_board", &id);
    let draft_id = id_of(&moved);
    assert!(
        draft_id.starts_with(".unfold/local/drafts/order-placement"),
        "{draft_id}"
    );
    assert_eq!(moved["place"], "draft");
    assert_eq!(moved["published"], Value::Null);
    assert_eq!(moved["document"]["pages"][0]["cards"][0]["id"], "create");
    assert!(!harness.path(&id).exists());
    assert!(
        !harness
            .path(".unfold/local/drafts/order-placement.changes.json")
            .exists()
    );
    assert!(harness.path(&draft_id).is_file());
    assert_eq!(harness.on_code(1, "board_health", &id), "board_not_held");
    harness.on(1, "board_health", &draft_id);
    let recents = harness.ok(1, "workspace", json!({}))["recentBoards"].clone();
    assert_eq!(recents[0]["board"], draft_id.as_str());
    let ids: Vec<Value> = harness.boards()["boards"]
        .as_array()
        .expect("boards")
        .iter()
        .map(|board| board["id"].clone())
        .collect();
    assert_eq!(ids, [json!(draft_id)]);

    // Deleting closes it and forgets it.
    let mark = harness.sink.mark();
    harness.on(1, "delete_board", &draft_id);
    assert!(!harness.path(&draft_id).exists());
    assert_eq!(harness.on_code(1, "board_health", &draft_id), "board_not_held");
    assert_eq!(harness.boards()["boards"], json!([]));
    assert_eq!(harness.ok(1, "workspace", json!({}))["recentBoards"], json!([]));
    let changed = harness.sink.events(mark, "boards_changed");
    assert_eq!(changed[0]["boards"], json!([draft_id]));

    // A published board's delete takes its changes copy too.
    let other = harness.on(1, "publish_board", &id_of(&harness.draft(1, "Other")));
    let other_id = id_of(&other["board"]);
    let mut edit = edit_of(&other["board"]);
    edit["title"] = json!("Other, changed");
    harness.save(1, &other_id, edit);
    harness.on(1, "delete_board", &other_id);
    assert!(!harness.path(&other_id).exists());
    assert!(!harness.path(".unfold/local/drafts/other.changes.json").exists());

    // Duplicate makes an unopened draft from the working version, in the same folder.
    let source = harness.ok(
        1,
        "new_board",
        json!({ "project": harness.project, "title": "Source", "folder": "Kept" }),
    );
    let copy = harness.on(1, "duplicate_board", &id_of(&source));
    assert_eq!(copy["title"], "Source copy");
    assert_eq!(copy["folder"], "Kept");
    assert_eq!(copy["heldBy"], Value::Null);
    assert_ne!(copy["id"], source["id"]);
    assert_eq!(
        harness.ok(
            2,
            "open_board",
            json!({ "project": harness.project, "id": copy["id"] })
        )["status"],
        "opened"
    );
}

#[test]
fn folders_hold_boards_held_or_not() {
    let harness = Harness::compiled("ts-orders");
    let project = harness.project.clone();
    let folder =
        |command: &str, path: &str| harness.call(1, command, json!({ "project": project, "path": path }));
    let empty = || harness.boards()["emptyFolders"].clone();
    folder("create_folder", "Checkout/Refunds").expect("create");
    folder("create_folder", "Checkout/Refunds").expect("creating it again does nothing");
    assert_eq!(empty(), json!(["Checkout/Refunds"]));
    for bad in ["", "a/../b", " padded", "a/", "a\\b"] {
        assert_eq!(
            folder("create_folder", bad).expect_err(bad).code,
            "invalid_folder",
            "{bad}"
        );
    }

    // A board above an empty folder leaves it empty; one in it fills it.
    let above = harness.ok(
        1,
        "new_board",
        json!({ "project": project, "title": "Above", "folder": "Checkout" }),
    );
    assert_eq!(empty(), json!(["Checkout/Refunds"]));
    let inside = harness.ok(
        2,
        "new_board",
        json!({ "project": project, "title": "Inside", "folder": "Checkout/Refunds" }),
    );
    let inside_id = id_of(&inside);
    assert_eq!(empty(), json!([]));

    // Any window moves any board; the holder's next save keeps the folder without a conflict.
    let mark = harness.sink.mark();
    let moved = harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": inside_id, "folder": "Archive" }),
    );
    assert_eq!(moved["folder"], "Archive");
    assert_eq!(moved["heldBy"], 2);
    let changed = harness.sink.events(mark, "boards_changed");
    assert_eq!(changed[0]["boards"], json!([inside_id]));
    assert_eq!(changed[0]["folders"], true);
    assert_eq!(empty(), json!(["Checkout/Refunds"]), "the folder it left stays");
    let mut edit = edit_of(&inside);
    edit["title"] = json!("Inside, renamed");
    let saved = harness.save(2, &inside_id, edit);
    assert_eq!(saved["document"]["folder"], "Archive");
    assert_eq!(saved["document"]["title"], "Inside, renamed");

    // A published board's move is an unpublished change.
    let published = harness.on(1, "publish_board", &id_of(&above));
    let published_id = id_of(&published["board"]);
    let before = harness.read(&published_id);
    let moved = harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": published_id, "folder": "Orders" }),
    );
    assert_eq!(moved["changes"], 1);
    assert_eq!(harness.read(&published_id), before);
    assert!(harness.path(".unfold/local/drafts/above.changes.json").exists());
    let back = harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": published_id, "folder": "Checkout" }),
    );
    assert_eq!(back["changes"], 0, "back where it was published: no changes");
    assert!(!harness.path(".unfold/local/drafts/above.changes.json").exists());
    assert_eq!(
        empty(),
        json!(["Checkout/Refunds", "Orders"]),
        "the folder it left stays"
    );

    // Renaming a folder moves what is in and under it.
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
        rename("Archive", "Old/Archive").expect("rename"),
        json!({ "boards": 1 })
    );
    assert_eq!(harness.listed(&inside_id)["folder"], "Old/Archive");
    assert_eq!(
        rename("Checkout", "Payments").expect("rename"),
        json!({ "boards": 1 })
    );
    assert_eq!(empty(), json!(["Orders", "Payments/Refunds"]));
    assert_eq!(harness.listed(&published_id)["changes"], 1);
    assert_eq!(
        rename("Old", "A/B/C/D/E/F/G/H").expect_err("too deep").code,
        "invalid_folder"
    );

    // Only an empty folder is deleted, with the empty folders under it.
    assert_eq!(
        folder("delete_folder", "Old").expect_err("not empty").code,
        "folder_not_empty"
    );
    assert_eq!(
        folder("delete_folder", "Payments").expect_err("not empty").code,
        "folder_not_empty"
    );
    harness.ok(
        1,
        "set_board_folder",
        json!({ "project": project, "id": published_id, "folder": null }),
    );
    assert_eq!(
        empty(),
        json!(["Orders", "Payments/Refunds"]),
        "the folder it left still holds an empty folder"
    );
    folder("delete_folder", "Payments").expect("delete");
    folder("delete_folder", "Orders").expect("delete");
    assert_eq!(empty(), json!([]));
    assert_eq!(
        folder("delete_folder", "Payments").expect_err("gone").code,
        "not_found"
    );

    // Deleting the last board of a folder keeps the folder.
    harness.on(2, "delete_board", &inside_id);
    assert_eq!(empty(), json!(["Old/Archive"]));
}

#[test]
fn outside_edits_are_conflicts_for_the_working_file() {
    let harness = Harness::new("ts-orders", true);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.draft(1, "Outside");
    let id = id_of(&board);

    // An agent rewrites the draft.
    let mut document = board["document"].clone();
    document["title"] = json!("Rewritten by an agent");
    let agent = serde_json::to_string_pretty(&document).expect("json");
    write_atomically(&harness.path(&id), &agent);
    let message = harness.conflict_at(&id, &revision_of(&agent));
    assert_eq!(message["kind"], "event");
    assert_eq!(message["windows"], json!([1]));
    assert_eq!(message["event"]["file"], "draft");
    assert_eq!(message["event"]["project"], harness.project.as_str());
    assert_eq!(message["event"]["savedRevision"], board["revision"]);

    // Saving over it is refused unless forced; reloading takes the agent's version.
    let save = json!({ "project": harness.project, "id": id, "board": edit_of(&board) });
    assert_eq!(harness.code(1, "save_board", save.clone()), "board_conflict");
    let reloaded = harness.on(1, "reload_board", &id);
    assert_eq!(reloaded["document"]["title"], "Rewritten by an agent");
    assert_eq!(reloaded["revision"], revision_of(&agent));
    let mut forced = save;
    forced["force"] = json!(true);
    let forced = harness.ok(1, "save_board", forced);
    assert_eq!(forced["document"]["title"], "Outside");

    // A published board without changes: its published file is the working file.
    let published = harness.on(1, "publish_board", &id);
    let published_id = id_of(&published["board"]);
    let theirs = harness.read(&published_id).replace("\"Outside\"", "\"Pulled\"");
    write_atomically(&harness.path(&published_id), &theirs);
    let message = harness.conflict_at(&published_id, &revision_of(&theirs));
    assert_eq!(message["event"]["file"], "published");
    let reloaded = harness.on(1, "reload_board", &published_id);
    assert_eq!(reloaded["document"]["title"], "Pulled");

    // With unpublished changes, the copy is: reported under the published board's id.
    let mut edit = edit_of(&reloaded);
    edit["title"] = json!("Mine");
    harness.save(1, &published_id, edit);
    let copy = harness.path(".unfold/local/drafts/outside.changes.json");
    let mark = harness.sink.mark();
    let agent = std::fs::read_to_string(&copy)
        .expect("copy")
        .replace("\"Mine\"", "\"Agent's\"");
    write_atomically(&copy, &agent);
    let message = harness.conflict_at(&published_id, &revision_of(&agent));
    assert_eq!(message["event"]["file"], "changes");
    assert!(
        harness
            .sink
            .events(mark, "boards_changed")
            .iter()
            .any(|event| event["boards"] == json!([published_id])),
        "a copy's change is reported under its published board"
    );
    let reloaded = harness.on(1, "reload_board", &published_id);
    assert_eq!(reloaded["document"]["title"], "Agent's");
    assert_eq!(reloaded["published"]["changes"], 1);
}

/// A writer that truncates the file and writes it in pieces is caught mid-write: the
/// conflicts report only the finished board, never the empty or half-written file.
#[test]
fn a_non_atomic_writer_is_reported_once_it_finishes() {
    let harness = Harness::new("ts-orders", true);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let board = harness.draft(1, "Outside");
    let id = id_of(&board);
    let mut document = board["document"].clone();
    document["title"] = json!("Written in pieces");
    let text = serde_json::to_string_pretty(&document).expect("json");
    let path = harness.path(&id);
    // Each piece lands well after the watcher's quiet period, so a check sees each one.
    let pause = std::time::Duration::from_millis(150);
    std::fs::write(&path, "").expect("truncate");
    std::thread::sleep(pause);
    std::fs::write(&path, &text[..text.len() / 2]).expect("first half");
    std::thread::sleep(pause);
    std::fs::write(&path, &text).expect("whole board");

    let message = harness.conflict_at(&id, &revision_of(&text));
    assert_eq!(message["event"]["savedRevision"], board["revision"]);
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
    let reloaded = harness.on(1, "reload_board", &id);
    assert_eq!(reloaded["document"]["title"], "Written in pieces");
}

#[test]
fn closing_a_window_waits_for_its_last_board_save() {
    // The window's final save and its detach race: the save is dispatched, then the detach
    // follows at once on another channel. The detach must let the save land.
    for round in 0..5 {
        let harness = Harness::compiled("ts-orders");
        let board = harness.draft(1, &format!("Closing {round}"));
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
        // After the detach, the window holds nothing.
        assert_eq!(harness.on_code(1, "board_health", &id), "board_not_held");
    }
}

#[test]
fn drafts_span_every_project() {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(9, "compile", json!({ "project": python }));
    let ts = harness.draft(1, "TypeScript draft");
    let published = harness.on(1, "publish_board", &id_of(&harness.draft(1, "Published")));
    std::thread::sleep(std::time::Duration::from_millis(20));
    let py = harness.ok(
        2,
        "new_board",
        json!({ "project": python, "title": "Python draft" }),
    );
    let drafts = harness.ok(1, "drafts", json!({}))["drafts"].clone();
    let rows: Vec<(Value, Value)> = drafts
        .as_array()
        .expect("drafts")
        .iter()
        .map(|row| (row["project"].clone(), row["id"].clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (json!(python), py["id"].clone()),
            (json!(harness.project), ts["id"].clone()),
        ],
        "most recently written first; published boards without changes aren't drafts"
    );
    assert_ne!(published["board"]["id"], Value::Null);
    let python_list = harness.ok(1, "list_boards", json!({ "project": python }));
    assert_eq!(python_list["project"], python.as_str());
    assert_eq!(python_list["boards"][0]["title"], "Python draft");
    assert_eq!(python_list["boards"][0]["heldBy"], 2);
}

#[test]
fn thumbnails_draw_first_pages() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.draft(1, "Thumbs");
    let id = id_of(&board);
    let mut edit = edit_of(&board);
    edit["pages"][0]["cards"] = json!([
        card("route", POST_ORDERS),
        card("create", CREATE_ORDER),
        card("place", PLACE_ORDER),
    ]);
    edit["pages"][0]["cards"][1]["folded"] = json!(true);
    edit["pages"][0]["cards"][2]["view"] = json!("flow");
    edit["pages"]
        .as_array_mut()
        .expect("pages")
        .push(json!({ "id": "page-2", "title": "More", "defaultView": "code", "groups": [], "cards": [] }));
    harness.save(1, &id, edit);
    let thumbnails = harness.ok(
        2,
        "board_thumbnails",
        json!({ "project": harness.project, "boards": [id, ".unfold/boards/gone.json"] }),
    );
    let first = &thumbnails["thumbnails"][0];
    assert_eq!(first["board"], id.as_str());
    assert_eq!(first["page"], json!({ "id": "page-1", "title": "Page 1" }));
    assert_eq!(first["pages"], 2);
    assert_eq!(first["truncated"], false);
    assert_eq!(first["error"], Value::Null);
    let cards = first["cards"].as_array().expect("cards");
    let shown = |id: &str| {
        cards
            .iter()
            .find(|card| card["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no {id} in {cards:?}"))
    };
    assert_eq!(shown("route")["kind"], "entrance");
    assert_eq!(shown("route")["lines"], Value::Null);
    assert_eq!(shown("create")["kind"], "block");
    assert_eq!(shown("create")["folded"], true);
    assert_eq!(shown("create")["view"], "code");
    assert!(shown("create")["lines"].as_u64().is_some_and(|lines| lines > 1));
    assert_eq!(shown("place")["view"], "flow");
    assert_eq!(shown("place")["folded"], false);
    assert!(cards.iter().all(|card| card["foreign"] == false));
    let edges = first["edges"].as_array().expect("edges");
    assert!(edges.contains(&json!(["route", "create"])), "{edges:?}");
    assert!(edges.contains(&json!(["create", "place"])), "{edges:?}");
    assert_eq!(first["links"], json!([]));
    let gone = &thumbnails["thumbnails"][1];
    assert_eq!(gone["page"], Value::Null);
    assert!(gone["error"].is_string());
    let many: Vec<String> = (0..61)
        .map(|index| format!(".unfold/boards/b{index}.json"))
        .collect();
    assert_eq!(
        harness.code(
            1,
            "board_thumbnails",
            json!({ "project": harness.project, "boards": many })
        ),
        "invalid_args"
    );
}

#[test]
fn health_reaches_problems_and_repairs_fix_it() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.draft(1, "Health");
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
