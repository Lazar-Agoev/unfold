//! Home: `project_summary` (the state line and the Health line), `entrances` (paged and
//! filtered per application), and Home's boards (`list_boards`, the Health line's boards), live
//! and resting.

use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::common::{Harness, eventually, restless};

/// A board with a card whose code is gone.
const GONE_BOARD: &str = r#"{
  "format": "unfold-board",
  "version": 1,
  "title": "Gone code",
  "pages": [{
    "id": "page-1", "title": "Checkout", "defaultView": "code", "groups": [],
    "cards": [{ "id": "gone", "at": [0, 0], "ref": { "block": "src/routes/orders.ts#function:gone" } }]
  }]
}
"#;

/// The sample board (`testdata/boards/order-placement.json`), drawn on `ts-orders`.
fn sample_board() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/boards/order-placement.json"),
    )
    .expect("the sample board")
}

/// A `ts-orders` copy with two boards, as a teammate committed them, the sample edited last.
fn with_boards(harness: &Harness) {
    let edited = |relative: &str, text: &str, seconds: u64| {
        harness.write(relative, text);
        std::fs::File::options()
            .write(true)
            .open(harness.root.join(relative))
            .expect("open")
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .expect("set the modification time");
    };
    edited(".unfold/boards/order-placement.json", &sample_board(), 2_000);
    edited(".unfold/boards/gone.json", GONE_BOARD, 1_000);
}

#[test]
fn the_summary_counts_the_graph_and_its_health() {
    let harness = Harness::compiled("ts-orders");
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["project"], harness.project);
    assert_eq!(summary["name"], "ts-orders");
    assert_eq!(summary["path"], harness.root.to_str().expect("utf-8"));
    assert_eq!(summary["state"], json!({"kind": "current"}));
    assert_eq!(summary["status"]["state"]["kind"], "current");
    assert!(summary["updatedAtMs"].as_i64().is_some_and(|at| at > 0));
    assert!(summary["files"].as_u64().is_some_and(|files| files > 20));
    assert_eq!(summary["languages"], json!(["typescript"]));
    let keys: Vec<&str> = summary
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "project",
            "name",
            "path",
            "state",
            "status",
            "updatedAtMs",
            "files",
            "languages",
            "health"
        ]
    );
    let health = &summary["health"];
    assert_eq!(
        *health,
        json!({ "doesntParse": 0, "boardsNeedingReview": 0, "boards": [] })
    );

    // A file that stops parsing shows in the row's state and the Health line.
    harness.write("src/broken.ts", "export function broken( {\n");
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["doesntParse"], 1);
    assert_eq!(summary["state"], json!({"kind": "problems", "count": 1}));
    let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
    assert_eq!(problems.as_array().map(Vec::len), Some(1));
}

#[test]
fn entrances_are_paged_filtered_and_ordered_per_application() {
    let harness = Harness::compiled("ts-orders");
    let overview = harness.ok(1, "entrances", json!({ "project": harness.project }));
    assert_eq!(overview["view"], "overview");
    assert_eq!(overview["project"], harness.project);
    let categories = overview["categories"].as_array().expect("categories");
    let order: Vec<&str> = categories
        .iter()
        .filter_map(|category| category["category"].as_str())
        .collect();
    assert_eq!(order, ["services", "workers", "scheduled"], "display order");
    let applications: Vec<Value> = categories
        .iter()
        .flat_map(|category| category["applications"].as_array().cloned().unwrap_or_default())
        .collect();
    assert_eq!(overview["counts"]["applications"], applications.len());
    let launches: usize = applications
        .iter()
        .map(|application| application["launches"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(overview["counts"]["launches"], launches);
    for launch in applications
        .iter()
        .flat_map(|application| application["launches"].as_array().cloned().unwrap_or_default())
    {
        assert_eq!(
            launch["folder"], "ts-orders",
            "the project's folder shows its name"
        );
        assert!(
            launch["command"]
                .as_str()
                .is_some_and(|command| !command.is_empty())
        );
        assert_eq!(launch["editable"], "override");
        assert!(
            launch["reaches"].is_u64(),
            "a live project counts reach: {launch}"
        );
    }

    let server = applications
        .iter()
        .find(|application| application["routes"].as_u64().is_some_and(|routes| routes > 0))
        .expect("an application with routes");
    // An Express server isn't an ASGI application: each launch serves the routes its
    // application proves it reaches.
    let server_launches = server["launches"].as_array().expect("launches");
    assert_eq!(server_launches.len(), 2, "npm run start and npm run dev");
    for launch in server_launches {
        assert_eq!(launch["routes"], server["routes"], "{launch}");
    }
    let scope = |args: Value| {
        let mut args = args;
        args["project"] = json!(harness.project);
        args["application"] = server["id"].clone();
        harness.ok(1, "entrances", args)
    };
    let all = scope(json!({ "limit": 500 }));
    assert_eq!(all["view"], "scope");
    let rows = all["rows"].as_array().expect("rows");
    assert_eq!(all["total"], rows.len());
    assert_eq!(all["nextCursor"], Value::Null);
    assert_eq!(server["entrances"], rows.len(), "launches and handlers");
    let sections = all["sections"].as_array().expect("sections");
    assert_eq!(sections[0]["title"], "Launch");
    let kinds: Vec<&str> = rows
        .iter()
        .map(|row| {
            sections[row["section"].as_u64().expect("section") as usize]["kind"]
                .as_str()
                .expect("kind")
        })
        .collect();
    let rank = |kind: &str| {
        ["launch", "http", "worker", "schedule", "command", "event"]
            .iter()
            .position(|known| *known == kind)
    };
    assert!(
        kinds.windows(2).all(|pair| rank(pair[0]) <= rank(pair[1])),
        "{kinds:?}"
    );
    for section in sections.iter().filter(|section| section["kind"] == "http") {
        assert!(
            section["title"]
                .as_str()
                .is_some_and(|title| title.starts_with('/')),
            "{section}"
        );
    }
    assert!(rows.iter().all(|row| row["reaches"].is_u64()));

    // Pages follow each other through the cursor, in the same order.
    let mut paged = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let mut args = json!({ "limit": 2 });
        if !cursor.is_null() {
            args["cursor"] = cursor.clone();
        }
        let page = scope(args);
        assert_eq!(page["total"], all["total"]);
        assert_eq!(
            page["sections"], all["sections"],
            "sections don't depend on the page"
        );
        paged.extend(page["rows"].as_array().cloned().unwrap_or_default());
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(&paged, rows);

    // The filter applies before paging.
    let http = rows.iter().find(|row| row["kind"] == "http").expect("a route");
    let filter = http["title"].as_str().expect("title").to_uppercase();
    let filtered = scope(json!({ "filter": filter, "limit": 1 }));
    let needle = filter.to_lowercase();
    let matching = rows
        .iter()
        .filter(|row| {
            [&row["title"], &row["trigger"], &row["handler"], &row["path"]]
                .iter()
                .filter_map(|value| value.as_str())
                .any(|text| text.to_lowercase().contains(&needle))
        })
        .count();
    assert_eq!(filtered["total"], matching);
    assert_eq!(filtered["rows"][0]["key"], http["key"]);

    for bad in [0, 501] {
        assert_eq!(
            harness.code(
                1,
                "entrances",
                json!({ "project": harness.project, "limit": bad })
            ),
            "invalid_args"
        );
    }
    assert_eq!(
        harness.code(
            1,
            "entrances",
            json!({ "project": harness.project, "application": server["id"], "unreached": true })
        ),
        "invalid_args"
    );
    assert_eq!(
        harness.code(
            1,
            "entrances",
            json!({ "project": harness.project, "application": "nope" })
        ),
        "not_found"
    );
}

#[test]
fn home_lists_every_board_with_its_health() {
    let harness = Harness::new("ts-orders", false);
    with_boards(&harness);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    // The health of every board, open or not, reaches Home's Health line.
    let gone = json!([{ "id": ".unfold/boards/gone.json", "title": "Gone code", "items": 1 }]);
    eventually("the boards' health reaches Home", || {
        harness.ok(1, "project_summary", json!({ "project": harness.project }))["health"]["boards"] == gone
    });
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["boardsNeedingReview"], 1);
    // A board that needs review is listed in Problems and shows in the Health line, not in the
    // row's state.
    eventually("the boards' health reaches Problems", || {
        let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
        problems
            .as_array()
            .expect("problems")
            .iter()
            .any(|problem| problem["category"] == "board")
    });
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["state"], json!({"kind": "current"}));
    let workspace = harness.ok(1, "workspace", json!({}));
    assert_eq!(workspace["projects"][0]["state"], json!({"kind": "current"}));

    let list = harness.ok(1, "list_boards", json!({ "project": harness.project }));
    assert_eq!(list["project"], harness.project.as_str());
    assert_eq!(list["emptyFolders"], json!([]));
    let boards = list["boards"].as_array().expect("boards");
    let ids: Vec<&Value> = boards.iter().map(|board| &board["id"]).collect();
    assert_eq!(
        ids,
        [".unfold/boards/order-placement.json", ".unfold/boards/gone.json"],
        "the last edited first"
    );
    let sample = &boards[0];
    assert_eq!(sample["title"], "Order placement");
    assert_eq!(sample["updatedAtMs"], 2_000_000);
    assert_eq!(
        (sample["pages"].clone(), sample["cards"].clone()),
        (json!(1), json!(6))
    );
    assert_eq!(sample["label"], Value::Null, "the sample is healthy");
    assert_eq!(boards[1]["label"], "1 missing");
}

#[test]
fn a_resting_projects_boards_are_read_from_disk() {
    let harness = Harness::with("ts-orders", restless());
    with_boards(&harness);
    harness.ok(9, "compile", json!({ "project": harness.project }));
    let live = || harness.row(&harness.project)["live"] == true;
    eventually("the project rests", || !live());

    let list = harness.ok(1, "list_boards", json!({ "project": harness.project }));
    let boards = list["boards"].as_array().expect("boards");
    assert_eq!(boards.len(), 2);
    assert!(
        boards
            .iter()
            .all(|board| board["label"].is_null() && board["error"].is_null()),
        "no health while resting: {boards:?}"
    );
    let refs = harness.ok(
        1,
        "board_refs",
        json!({ "project": harness.project, "key": "src/routes/orders.ts#function:createOrder" }),
    );
    assert_eq!(refs["refs"][0]["board"], ".unfold/boards/order-placement.json");
    assert_eq!(refs["refs"][0]["pageTitle"], "Placing an order");
    assert!(!live(), "none of that woke it");
}
