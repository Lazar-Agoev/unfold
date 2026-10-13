//! The workspace's rows: adding (by path and by repository), removing and adding back, order,
//! expansion, selection, recent boards, `workspace.json` across restarts, and unknown projects.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use unfold_workspace::{Host, ProjectId};

use crate::common::{Harness, is_event, options};

fn id(text: &str) -> ProjectId {
    serde_json::from_value(json!(text)).expect("a project id")
}

fn ids(workspace: &Value) -> Vec<String> {
    workspace["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
}

#[test]
fn adding_finds_existing_rows_and_never_writes_in_the_folder() {
    let harness = Harness::new("ts-orders", false);
    let row = harness.row(&harness.project);
    assert_eq!(row["name"], "ts-orders");
    assert_eq!(row["expanded"], true);
    assert_eq!(row["live"], false);
    assert_eq!(row["git"], Value::Null);
    assert_eq!(
        row["state"],
        json!({"kind": "resting", "last": {"kind": "not_compiled"}, "atMs": row["state"]["atMs"]})
    );
    assert!(harness.project.starts_with("p-") && harness.project.len() == 12);
    assert!(!harness.root.join(".unfold/local").exists());

    // The same folder, spelled another way, is the same row.
    let again = harness
        .trusted("add_project", json!({ "path": harness.root.join("src/..") }))
        .expect("add again");
    assert_eq!(again["status"], "exists");
    assert_eq!(again["project"]["id"], harness.project);

    // Only the main process names absolute paths; only folders are projects.
    assert_eq!(
        harness.code(1, "add_project", json!({ "path": harness.root })),
        "untrusted"
    );
    let missing = harness
        .trusted("add_project", json!({ "path": harness.base.join("nope") }))
        .expect_err("missing");
    assert_eq!(missing.code, "not_a_folder");
    let relative = harness
        .trusted("add_project", json!({ "path": "relative/path" }))
        .expect_err("relative");
    assert_eq!(relative.code, "invalid_args");
    let file = harness
        .trusted(
            "add_project",
            json!({ "path": harness.root.join("package.json") }),
        )
        .expect_err("a file");
    assert_eq!(file.code, "not_a_folder");
}

#[test]
fn rows_reorder_expand_and_announce_themselves() {
    let harness = Harness::new("ts-orders", false);
    let mark = harness.sink.mark();
    let (python, _) = harness.add("py-orders", "py-orders");
    let changed = harness.sink.events(mark, "workspace_changed");
    assert_eq!(changed.len(), 1, "one announcement per change");
    assert_eq!(
        ids(&changed[0]["workspace"]),
        [harness.project.clone(), python.clone()]
    );

    let view = harness.ok(
        1,
        "reorder_projects",
        json!({ "order": [python, harness.project] }),
    );
    assert_eq!(ids(&view), [python.clone(), harness.project.clone()]);
    for bad in [
        json!([python]),
        json!([python, python]),
        json!([python, harness.project, "p-nonenonen"]),
    ] {
        assert_eq!(
            harness.code(1, "reorder_projects", json!({ "order": bad })),
            "invalid_args"
        );
    }

    let mark = harness.sink.mark();
    assert_eq!(
        harness.ok(
            1,
            "set_project_expanded",
            json!({ "project": python, "expanded": false })
        ),
        Value::Null
    );
    let changed = harness.sink.events(mark, "workspace_changed");
    assert_eq!(changed[0]["workspace"]["projects"][0]["expanded"], false);
    assert_eq!(harness.row(&python)["expanded"], false);
}

#[test]
fn selecting_wakes_the_project_and_becomes_the_default() {
    let harness = Harness::new("ts-orders", false);
    let mark = harness.sink.mark();
    assert_eq!(
        harness.ok(1, "select_project", json!({ "project": harness.project })),
        Value::Null
    );
    // Live when select returns; the state change is announced.
    let row = harness.row(&harness.project);
    assert_eq!(row["live"], true);
    assert_eq!(row["state"], json!({"kind": "not_compiled"}));
    assert!(harness.sink.all().iter().skip(mark).any(|message| is_event(
        message,
        "project_state",
        &harness.project
    )));
    assert_eq!(harness.ok(1, "workspace", json!({}))["selected"], harness.project);
    assert_eq!(
        harness.ok(1, "select_project", json!({ "project": null })),
        Value::Null
    );
    assert_eq!(harness.ok(1, "workspace", json!({}))["selected"], harness.project);
    assert_eq!(
        harness.code(1, "select_project", json!({ "project": "p-nonenonen" })),
        "unknown_project"
    );
}

#[test]
fn removed_folders_can_be_added_back_and_nothing_else() {
    let harness = Harness::new("ts-orders", false);
    let (python, python_root) = harness.add("py-orders", "py-orders");
    harness.ok(1, "select_project", json!({ "project": python }));
    harness.ok(1, "compile", json!({ "project": python }));

    // An open board keeps its project in the workspace.
    let host = harness.host.projects().expect("initialized");
    let board = ".unfold/local/drafts/board-1.json";
    host.handle(&id(&python))
        .expect("project")
        .open_boards()
        .open(board, 2);
    assert_eq!(
        harness.code(1, "remove_project", json!({ "project": python })),
        "boards_open"
    );
    harness.host.detach_window(2).expect("detach");

    let mark = harness.sink.mark();
    let before = now_ms();
    assert_eq!(
        harness.ok(1, "remove_project", json!({ "project": python })),
        Value::Null
    );
    let after = now_ms();
    assert!(
        harness
            .sink
            .all()
            .iter()
            .skip(mark)
            .any(|message| is_event(message, "project_reset", &python))
    );
    let workspace = harness.ok(1, "workspace", json!({}));
    assert_eq!(ids(&workspace), std::slice::from_ref(&harness.project));
    let path = python_root.to_str().expect("utf-8");
    let removed_at = workspace["removed"][0]["removedAtMs"]
        .as_i64()
        .expect("removedAtMs");
    assert!(
        (before..=after).contains(&removed_at),
        "it records when it was removed"
    );
    assert_eq!(
        workspace["removed"],
        json!([{ "path": path, "name": "py-orders", "removedAtMs": removed_at }])
    );
    assert_eq!(
        workspace["selected"],
        Value::Null,
        "a removed project isn't the default"
    );
    assert!(
        python_root.join(".unfold/local/cache").exists(),
        "its .unfold stays"
    );
    for command in ["project_summary", "git_info", "problems"] {
        assert_eq!(
            harness.code(1, command, json!({ "project": python })),
            "unknown_project",
            "{command}"
        );
    }

    // Only a path "Opened before" lists can come back from a window.
    assert_eq!(
        harness.code(1, "readd_project", json!({ "path": harness.root })),
        "not_found"
    );
    let back = harness.ok(1, "readd_project", json!({ "path": path }));
    assert_eq!(back["status"], "added");
    assert_ne!(back["project"]["id"], python, "a new row gets a new id");
    assert_eq!(
        back["project"]["state"]["last"]["kind"], "current",
        "its graph is still there"
    );
    assert_eq!(harness.ok(1, "workspace", json!({}))["removed"], json!([]));
}

#[test]
fn recent_boards_follow_their_projects() {
    let harness = Harness::new("ts-orders", false);
    let (python, _) = harness.add("py-orders", "py-orders");
    let projects = harness.host.projects().expect("initialized");
    projects
        .record_recent_board(&id(&harness.project), ".unfold/boards/a.json", "A")
        .expect("record");
    projects
        .record_recent_board(&id(&python), ".unfold/boards/b.json", "B")
        .expect("record");
    let recents = harness.ok(1, "workspace", json!({}))["recentBoards"].clone();
    assert_eq!(recents[0]["project"], python);
    assert_eq!(recents[1]["board"], ".unfold/boards/a.json");
    assert_eq!(recents[1]["title"], "A");
    harness.ok(1, "remove_project", json!({ "project": python }));
    let recents = harness.ok(1, "workspace", json!({}))["recentBoards"].clone();
    assert_eq!(recents.as_array().map(Vec::len), Some(1));
}

#[test]
fn the_workspace_comes_back_after_a_restart_resting() {
    let harness = Harness::new("ts-orders", false);
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    harness.ok(
        1,
        "set_project_expanded",
        json!({ "project": python, "expanded": false }),
    );
    let before = harness.ok(1, "workspace", json!({}));
    harness.host.shutdown().expect("shutdown");
    let text = std::fs::read_to_string(harness.base.join("app/workspace.json")).expect("workspace.json");
    assert!(text.ends_with("}\n"));
    let saved: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["projects"][0]["lastKnown"]["state"]["kind"], "current");

    let host = Host::with_options(options(false));
    host.init(
        &json!({
            "appDataDir": harness.base.join("app"),
            "logsDir": harness.base.join("logs"),
            "fallbackCacheDir": harness.base.join("fallback"),
        })
        .to_string(),
    )
    .expect("init");
    let after: Value = serde_json::from_str(
        &host
            .dispatch(1, 1, r#"{"command":"workspace","args":{}}"#)
            .expect("workspace"),
    )
    .expect("json");
    assert_eq!(ids(&after), ids(&before));
    assert_eq!(after["selected"], before["selected"]);
    let rows = after["projects"].as_array().expect("rows");
    assert!(
        rows.iter().all(|row| row["live"] == false),
        "every project starts resting"
    );
    assert_eq!(rows[0]["state"]["kind"], "resting");
    assert_eq!(rows[0]["state"]["last"], json!({"kind": "current"}));
    assert_eq!(rows[1]["expanded"], false);
    let names: BTreeSet<&str> = rows.iter().filter_map(|row| row["name"].as_str()).collect();
    assert_eq!(names, BTreeSet::from(["ts-orders", "py-orders"]));
    host.shutdown().expect("shutdown");
}

#[test]
fn unknown_projects_are_refused_everywhere() {
    let harness = Harness::new("ts-orders", false);
    let unknown = json!({ "project": "p-nonenonen" });
    for command in [
        "project_summary",
        "compile_preview",
        "problems",
        "index_log",
        "compile",
        "project_config",
        "git_info",
        "remove_project",
    ] {
        assert_eq!(
            harness.code(1, command, unknown.clone()),
            "unknown_project",
            "{command}"
        );
    }
    assert_eq!(
        harness.code(
            1,
            "card",
            json!({ "project": "p-nonenonen", "key": "src/a.ts#function:f" })
        ),
        "unknown_project"
    );
    assert_eq!(
        harness.code(1, "search", json!({ "query": "x", "projects": ["p-nonenonen"] })),
        "unknown_project"
    );
}

#[test]
fn a_new_shared_name_reaches_every_window() {
    let harness = Harness::new("ts-orders", false);
    let mark = harness.sink.mark();
    let config = harness.ok(
        1,
        "update_project_config",
        json!({ "project": harness.project, "edits": [{ "op": "set_project_name", "name": "storefront-api" }] }),
    );
    assert_eq!(
        config["name"],
        json!({ "value": "storefront-api", "isDefault": false })
    );
    assert_eq!(harness.row(&harness.project)["name"], "storefront-api");
    let changed = harness.sink.events(mark, "workspace_changed");
    assert_eq!(
        changed.last().expect("workspace_changed")["workspace"]["projects"][0]["name"],
        "storefront-api"
    );
    assert_eq!(
        harness
            .host
            .projects()
            .expect("initialized")
            .resolve_name("storefront-api"),
        Some(id(&harness.project))
    );
    // Removing it brings the default back.
    harness.ok(
        1,
        "update_project_config",
        json!({ "project": harness.project, "edits": [{ "op": "set_project_name", "name": null }] }),
    );
    assert_eq!(harness.row(&harness.project)["name"], "ts-orders");
    assert_eq!(
        harness.code(
            1,
            "update_project_config",
            json!({ "project": harness.project, "edits": [{ "op": "set_project_name", "name": "no spaces" }] })
        ),
        "invalid_settings"
    );
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after the epoch");
    i64::try_from(elapsed.as_millis()).expect("fits")
}
