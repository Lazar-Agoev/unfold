//! Live and resting projects (decision U-45): what wakes a project, when it rests, what
//! keeps it live, and how a resting project answers reads from its stored graph.

use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::{Value, json};
use unfold_workspace::ProjectId;

use crate::common::{Harness, eventually, restless};

fn id(text: &str) -> ProjectId {
    serde_json::from_value(json!(text)).expect("a project id")
}

fn live(harness: &Harness, project: &str) -> bool {
    harness.row(project)["live"] == true
}

/// Compiles `project` from window 9 without selecting it, so nothing keeps it live afterwards.
fn compile_unselected(harness: &Harness, project: &str) {
    harness.ok(9, "compile", json!({ "project": project }));
}

#[test]
fn an_unused_project_rests_and_remembers_its_state() {
    let harness = Harness::with("ts-orders", restless());
    assert!(!live(&harness, &harness.project), "projects start resting");
    let mark = harness.sink.mark();
    compile_unselected(&harness, &harness.project);
    let resting_event = || {
        harness
            .sink
            .events(mark, "project_state")
            .into_iter()
            .find(|event| event["state"]["kind"] == "resting")
    };
    eventually("the project rests", || resting_event().is_some());
    let resting = resting_event().expect("a resting project_state");
    assert!(!live(&harness, &harness.project));
    assert_eq!(resting["project"], harness.project);
    assert_eq!(resting["state"]["last"], json!({"kind": "current"}));
    let row = harness.row(&harness.project);
    assert_eq!(row["state"]["kind"], "resting");
    assert_eq!(row["state"]["last"], json!({"kind": "current"}));
    let saved: Value = serde_json::from_str(
        &std::fs::read_to_string(harness.base.join("app/workspace.json")).expect("workspace.json"),
    )
    .expect("json");
    assert_eq!(
        saved["projects"][0]["lastKnown"]["state"],
        json!({"kind": "current"})
    );
    // A resting project holds no lock: another copy could open it.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(harness.root.join(".unfold/local/cache/lock"))
        .expect("the lock file");
    assert!(lock.try_lock().is_ok(), "the folder lock was released");
}

#[test]
fn a_selected_project_stays_live_and_rests_after_its_lease() {
    let harness = Harness::with("ts-orders", restless());
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    std::thread::sleep(Duration::from_millis(400));
    assert!(live(&harness, &harness.project), "a selection keeps it live");
    harness.ok(1, "select_project", json!({ "project": null }));
    eventually("the project rests once unselected", || {
        !live(&harness, &harness.project)
    });
    // Closing the window that selected a project releases it the same way.
    harness.ok(2, "select_project", json!({ "project": harness.project }));
    assert!(live(&harness, &harness.project));
    std::thread::sleep(Duration::from_millis(300));
    assert!(live(&harness, &harness.project));
    harness.host.detach_window(2).expect("detach");
    eventually("the project rests once its window closed", || {
        !live(&harness, &harness.project)
    });
}

#[test]
fn using_a_result_wakes_its_project() {
    let harness = Harness::with("ts-orders", restless());
    compile_unselected(&harness, &harness.project);
    eventually("the project rests", || !live(&harness, &harness.project));
    let card = harness.ok(
        1,
        "card",
        json!({ "project": harness.project, "key": "src/routes/orders.ts#function:createOrder" }),
    );
    assert_eq!(card["type"], "block");
    assert!(live(&harness, &harness.project), "a card wakes its project");
    eventually("it rests again", || !live(&harness, &harness.project));
    harness.ok(
        1,
        "file_view",
        json!({ "project": harness.project, "path": "src/server.ts", "range": { "startLine": 1, "endLine": 3 } }),
    );
    assert!(live(&harness, &harness.project), "a file view wakes its project");
}

#[test]
fn a_running_compile_keeps_its_project_live() {
    let harness = Harness::with("ts-orders", restless());
    for index in 0..800 {
        harness.write(
            &format!("src/generated/file{index:04}.ts"),
            &format!("export function f{index}(value: number): number {{ return value + {index}; }}\n"),
        );
    }
    let host = harness.host.clone();
    let request = json!({ "command": "compile", "args": { "project": harness.project } }).to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    host.dispatch_async(9, harness.request(), request, move |result| {
        let _ = sender.send(result);
    });
    eventually("the compile wakes the project", || {
        live(&harness, &harness.project)
    });
    // Longer than the lease: only the compile pin keeps it live.
    while receiver.try_recv().is_err() {
        assert!(live(&harness, &harness.project), "rested while compiling");
        std::thread::sleep(Duration::from_millis(30));
    }
    eventually("it rests after the compile", || !live(&harness, &harness.project));
}

#[test]
fn an_open_board_keeps_the_projects_its_cards_use_live() {
    let harness = Harness::with("ts-orders", restless());
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "select_project", json!({ "project": python }));
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    let projects = harness.host.projects().expect("initialized");
    let board = ".unfold/local/drafts/board-1.json";
    let own = projects.handle(&id(&harness.project)).expect("project");
    own.claims().claim(board, 1).expect("claim");
    own.claims().set_references(board, BTreeSet::from([id(&python)]));
    std::thread::sleep(Duration::from_millis(400));
    assert!(live(&harness, &python), "a board with its cards keeps it live");
    own.claims().release(board, 1);
    eventually("it rests once the board closed", || !live(&harness, &python));
    assert!(
        live(&harness, &harness.project),
        "the selected project stays live"
    );
    // An open board gaining cards from a resting project wakes it.
    own.claims().claim(board, 1).expect("claim");
    own.claims().set_references(board, BTreeSet::from([id(&python)]));
    eventually("the referenced project wakes", || live(&harness, &python));
}

#[test]
fn a_resting_project_answers_reads_from_its_stored_graph() {
    let harness = Harness::with("ts-orders", restless());
    compile_unselected(&harness, &harness.project);
    eventually("the project rests", || !live(&harness, &harness.project));

    let results = harness.ok(1, "search", json!({ "query": "place" }));
    assert!(
        results["definitions"]["hits"]
            .as_array()
            .expect("hits")
            .iter()
            .any(|hit| hit["qualifiedName"] == "OrderService.place" && hit["project"] == harness.project),
        "{results}"
    );
    let files = harness.ok(1, "search", json!({ "query": "ordsvc", "kinds": ["files"] }));
    assert_eq!(
        files["files"]["hits"][0]["path"], "src/services/order-service.ts",
        "files come from the stored graph"
    );
    let map = harness.ok(1, "entrances", json!({ "project": harness.project }));
    let row = map["applications"][0]["entrances"][0].clone();
    assert!(row["key"].is_string(), "{map}");
    assert_eq!(row["reaches"], Value::Null, "reach needs the live index");
    let text = harness.ok(1, "search_text", json!({ "query": "orderService" }));
    assert!(text["matches"].as_u64().is_some_and(|matches| matches > 0));
    let config = harness.ok(1, "project_config", json!({ "project": harness.project }));
    assert_eq!(config["name"], json!({"value": "ts-orders", "isDefault": true}));
    assert_eq!(
        harness.ok(1, "git_info", json!({ "project": harness.project })),
        Value::Null
    );
    assert_eq!(
        harness.ok(1, "cancel_compile", json!({ "project": harness.project })),
        json!({ "cancelled": false })
    );
    assert!(!live(&harness, &harness.project), "none of that woke it");
}

#[test]
fn a_project_whose_folder_is_gone_is_unavailable() {
    let harness = Harness::with("ts-orders", restless());
    std::fs::remove_dir_all(&harness.root).expect("delete the folder");
    let row = harness.row(&harness.project);
    assert_eq!(row["state"]["kind"], "unavailable");
    assert_eq!(
        harness.code(1, "select_project", json!({ "project": harness.project })),
        "not_a_folder"
    );
    assert_eq!(
        harness.code(1, "project_summary", json!({ "project": harness.project })),
        "not_a_folder"
    );
    assert_eq!(harness.row(&harness.project)["state"]["kind"], "unavailable");
    // A resting read of a missing graph is "not compiled", never a failure.
    assert_eq!(
        harness.code(1, "entrances", json!({ "project": harness.project })),
        "not_compiled"
    );
}
