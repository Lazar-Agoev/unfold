//! Home: `project_summary` (the state line and the Health line), `entrances` (the apps and
//! entrances as written, with each `code`'s state and fix), `problems`, and Home's boards
//! (`list_boards`, the Health line's boards), live and resting.

use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::common::{Harness, eventually, restless};

/// A board with a card whose code is gone.
const GONE_BOARD: &str = r#"{
  "format": "unfold-board",
  "version": 1,
  "title": "Gone code",
  "pages": [{
    "id": "page-1", "title": "Checkout", "defaultView": "compact",
    "cards": [{ "id": "gone", "at": [0, 0], "ref": { "block": "src/routes/orders.ts#gone" } }]
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
    // One `code` key is missing (moved), two name JavaScript files Unfold doesn't analyze.
    let health = &summary["health"];
    assert_eq!(
        *health,
        json!({ "doesntParse": 0, "boardsNeedingReview": 0, "boards": [], "entrancesToCheck": 3 })
    );

    // A file that stops parsing shows in the row's state and the Health line.
    harness.write("src/broken.ts", "export function broken( {\n");
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["doesntParse"], 1);
    assert_eq!(summary["state"], json!({"kind": "problems", "count": 1}));
    let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
    assert_eq!(problems.as_array().map(Vec::len), Some(1));

    // An entrance's file that stops parsing keeps its last good block: the target is stale,
    // still reachable, and counted once, as a file that doesn't parse.
    let cleanup = std::fs::read_to_string(harness.root.join("src/jobs/cleanup.ts")).expect("read");
    harness.write(
        "src/jobs/cleanup.ts",
        &format!("{cleanup}\nexport function broken( {{\n"),
    );
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let stale = harness
        .apps()
        .iter()
        .flat_map(|app| app["entrances"].as_array().cloned().unwrap_or_default())
        .flat_map(|entrance| entrance["code"].as_array().cloned().unwrap_or_default())
        .filter(|code| code["state"] == "stale")
        .count();
    assert_eq!(stale, 1);
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["doesntParse"], 2);
    assert_eq!(summary["health"]["entrancesToCheck"], 3);
}

#[test]
fn entrances_list_the_apps_as_written_with_states_and_fixes() {
    let harness = Harness::compiled("ts-orders");
    let apps = harness.apps();
    let names: Vec<&str> = apps.iter().filter_map(|app| app["name"].as_str()).collect();
    assert_eq!(
        names,
        [
            "Server",
            "Dev server",
            "Worker",
            "Scheduler",
            "Orders CLI",
            "Dev CLI",
            "Legacy export"
        ],
        "file order"
    );
    let server = &apps[0];
    assert_eq!(server["key"], "entrance:server");
    assert_eq!(server["run"], "npm run start");
    assert_eq!(
        server["code"],
        json!({ "key": "src/server.ts#", "state": "found" })
    );
    assert!(
        server["lines"][0].as_u64().is_some_and(|line| line > 1),
        "{server}"
    );
    let entrances = server["entrances"].as_array().expect("entrances");
    assert_eq!(entrances.len(), 9);
    let place = &entrances[4];
    assert_eq!(place["key"], "entrance:server/place-an-order");
    assert_eq!(place["on"], "POST /orders");
    let states: Vec<&Value> = place["code"]
        .as_array()
        .expect("code")
        .iter()
        .map(|code| &code["state"])
        .collect();
    assert_eq!(states, ["found", "found", "found"], "in the order they run");
    // The deliberately missing key, with its one fix.
    let admin = &entrances[7];
    assert_eq!(admin["name"], "Admin stats");
    assert_eq!(
        admin["code"],
        json!([{
            "key": "src/server.ts#adminStats",
            "state": "missing",
            "fix": {
                "kind": "moved_definition",
                "code": "src/routes/admin.ts#adminStats",
                "path": "src/routes/admin.ts"
            }
        }])
    );
    let lines = admin["lines"].as_array().expect("lines");
    assert_eq!(lines[0], lines[1], "one line in unfold.toml");
    assert_eq!(apps[6]["code"]["state"], "not_analyzed");

    // An entry with a problem is listed with it, and stays out of the graph.
    let text = std::fs::read_to_string(harness.root.join("unfold.toml")).expect("read");
    harness.write(
        "unfold.toml",
        &format!("{text}\n[[app]]\nname = \"Server\"\ncolor = \"red\"\n"),
    );
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let apps = harness.apps();
    let duplicate = apps.last().expect("the new app");
    assert_eq!(duplicate["name"], "Server");
    let problem = &duplicate["problem"];
    assert!(
        matches!(problem["kind"].as_str(), Some("unknown_field" | "duplicate_key")),
        "{problem}"
    );
    assert!(problem["line"].as_u64().is_some(), "{problem}");
    assert!(duplicate.get("code").is_none());
    assert_eq!(apps[0]["code"]["state"], "found", "the first app still works");
}

#[test]
fn the_health_line_counts_each_entry_that_needs_a_look_once() {
    let harness = Harness::compiled("ts-orders");
    let config = std::fs::read_to_string(harness.root.join("unfold.toml")).expect("read");
    // Three keys of one entrance aren't there, and another entrance has a field entrances
    // don't have: each counts once, beside the three the playground has.
    let config = config
        .replace(
            "[\"src/routes/middleware.ts#requireUser\", \"src/routes/middleware.ts#validateBody\", \"src/routes/orders.ts#createOrder\"]",
            "[\"src/gone.ts#a\", \"src/gone.ts#b\", \"src/gone.ts#c\"]",
        )
        .replace(
            "code = \"src/server.ts#healthCheck\" }",
            "code = \"src/server.ts#healthCheck\", kind = \"http\" }",
        );
    harness.write("unfold.toml", &config);
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let summary = harness.ok(1, "project_summary", json!({ "project": harness.project }));
    assert_eq!(summary["health"]["entrancesToCheck"], 5);
}

#[test]
fn entrances_list_one_app_by_its_name_or_key() {
    let harness = Harness::compiled("ts-orders");
    for given in ["Worker", "worker!", "entrance:worker"] {
        let view = harness.ok(
            1,
            "entrances",
            json!({ "project": harness.project, "app": given }),
        );
        let names: Vec<&str> = view["apps"]
            .as_array()
            .expect("apps")
            .iter()
            .filter_map(|app| app["name"].as_str())
            .collect();
        assert_eq!(names, ["Worker"], "{given}");
    }
    let error = harness
        .call(
            1,
            "entrances",
            json!({ "project": harness.project, "app": "web" }),
        )
        .expect_err("no such app");
    assert_eq!(error.code, "not_found");
    assert_eq!(
        error.message,
        "no app “web” is in unfold.toml. Its apps: Server, Dev server, Worker, Scheduler, Orders CLI, Dev CLI, Legacy export"
    );
}

#[test]
fn problems_list_files_that_dont_parse() {
    let harness = Harness::compiled("ts-orders");
    assert_eq!(
        harness.ok(1, "problems", json!({ "project": harness.project })),
        json!([])
    );
    harness.write("src/broken.ts", "export function broken( {\n");
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let problems = harness.ok(1, "problems", json!({ "project": harness.project }));
    let problems = problems.as_array().expect("problems");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0]["category"], "doesnt_parse");
    assert_eq!(problems[0]["path"], "src/broken.ts");
    assert!(problems[0]["line"].is_u64(), "{problems:?}");
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
        json!({ "project": harness.project, "key": "src/routes/orders.ts#createOrder" }),
    );
    assert_eq!(refs["refs"][0]["board"], ".unfold/boards/order-placement.json");
    assert_eq!(refs["refs"][0]["pageTitle"], "Placing an order");
    assert!(!live(), "none of that woke it");
}
