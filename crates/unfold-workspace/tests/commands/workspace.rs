//! The workspace's rows: adding (and compiling at once), folders Unfold can't write into,
//! removing, order, expansion, selection, `workspace.json` across restarts, and unknown
//! projects.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use unfold_workspace::{Host, ProjectId};

use crate::common::{Harness, copy_dir, eventually, is_event, options, playground};

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

/// Waits until project `project`'s row says its graph is current.
fn wait_current(harness: &Harness, project: &str) {
    eventually("the project is compiled", || {
        harness.row(project)["state"] == json!({"kind": "current"})
    });
}

#[test]
fn adding_compiles_at_once_and_finds_existing_rows() {
    let harness = Harness::new("ts-orders", false);
    let row = harness.row(&harness.project);
    assert_eq!(row["name"], "ts-orders");
    assert_eq!(row["expanded"], true);
    assert_eq!(row["live"], true, "adding wakes the project to compile it");
    assert!(harness.project.starts_with("p-") && harness.project.len() == 12);
    wait_current(&harness, &harness.project);
    assert!(harness.root.join(".unfold/local/cache/graph.sqlite").exists());

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
fn every_folder_is_a_project_of_its_own() {
    let harness = Harness::empty(options(false));
    // Two checkouts of one repository, and a subfolder of one of them.
    for checkout in ["repo", "copy"] {
        std::fs::create_dir_all(harness.base.join(checkout).join(".git")).expect("mkdir");
        copy_dir(&playground("ts-orders"), &harness.base.join(checkout).join("app"));
    }
    let mut ids = BTreeSet::new();
    for (folder, name) in [("repo", "repo"), ("copy", "copy"), ("repo/app", "app")] {
        let added = harness
            .trusted("add_project", json!({ "path": harness.base.join(folder) }))
            .expect("add");
        assert_eq!(added["status"], "added", "{folder}");
        assert_eq!(added["project"]["name"], name, "{folder}");
        ids.insert(added["project"]["id"].as_str().expect("id").to_owned());
    }
    assert_eq!(ids.len(), 3, "every folder is its own row");
}

#[test]
fn a_folder_unfold_cant_write_into_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let harness = Harness::new("ts-orders", false);
    let folder = harness.base.join("read-only");
    copy_dir(&playground("py-orders"), &folder);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let refused = harness.trusted("add_project", json!({ "path": folder }));
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    let error = refused.expect_err("read-only");
    assert_eq!(error.code, "folder_not_writable");
    assert!(
        error.message.contains("Unfold needs to write `.unfold/`"),
        "{}",
        error.message
    );
    assert_eq!(
        ids(&harness.ok(1, "workspace", json!({}))),
        std::slice::from_ref(&harness.project),
        "no row was added"
    );
    assert!(!folder.join(".unfold").exists());
}

#[test]
fn rows_expand_and_announce_themselves() {
    let harness = Harness::new("ts-orders", false);
    let mark = harness.sink.mark();
    let (python, _) = harness.add("py-orders", "py-orders");
    let changed = harness.sink.events(mark, "workspace_changed");
    assert_eq!(changed.len(), 1, "one announcement per change");
    assert_eq!(
        ids(&changed[0]["workspace"]),
        [harness.project.clone(), python.clone()]
    );

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
    assert_eq!(changed[0]["workspace"]["projects"][1]["expanded"], false);
    assert_eq!(harness.row(&python)["expanded"], false);
}

#[test]
fn selecting_keeps_the_project_live_and_becomes_the_default() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(
        harness.ok(1, "select_project", json!({ "project": harness.project })),
        Value::Null
    );
    assert_eq!(harness.row(&harness.project)["live"], true);
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
fn removing_keeps_the_folder_and_adding_it_again_makes_a_new_row() {
    let harness = Harness::new("ts-orders", false);
    let (python, python_root) = harness.add("py-orders", "py-orders");
    harness.ok(1, "select_project", json!({ "project": python }));
    harness.ok(1, "compile", json!({ "project": python }));

    // An open board keeps its project in the workspace.
    let host = harness.host.projects().expect("initialized");
    let board = ".unfold/boards/board-1.json";
    host.handle(&id(&python))
        .expect("project")
        .open_board(board, 2)
        .expect("open");
    assert_eq!(
        harness.code(1, "remove_project", json!({ "project": python })),
        "boards_open"
    );
    harness.host.detach_window(2).expect("detach");

    let mark = harness.sink.mark();
    assert_eq!(
        harness.ok(1, "remove_project", json!({ "project": python })),
        Value::Null
    );
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
    assert_eq!(
        workspace
            .as_object()
            .map(|fields| fields.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["projects".to_owned(), "selected".to_owned()]),
        "project rows only"
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
    for command in ["project_summary", "problems"] {
        assert_eq!(
            harness.code(1, command, json!({ "project": python })),
            "unknown_project",
            "{command}"
        );
    }

    let back = harness
        .trusted("add_project", json!({ "path": python_root }))
        .expect("add again");
    assert_eq!(back["status"], "added");
    assert_ne!(back["project"]["id"], python, "a new row gets a new id");
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
    assert_eq!(saved["projects"][0]["lastKnown"]["state"]["kind"], "current");

    let host = Host::with_options(options(false));
    host.init(&json!({ "appDataDir": harness.base.join("app") }).to_string())
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
        "problems",
        "compile",
        "project_config",
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
            json!({ "project": "p-nonenonen", "key": "src/a.ts#f" })
        ),
        "unknown_project"
    );
    assert_eq!(
        harness.code(1, "search", json!({ "query": "x", "projects": ["p-nonenonen"] })),
        "unknown_project"
    );
}

#[test]
fn a_project_is_named_after_its_folder() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(harness.row(&harness.project)["name"], "ts-orders");
    let config = harness.ok(1, "project_config", json!({ "project": harness.project }));
    assert!(config.get("name").is_none(), "{config}");
    assert_eq!(
        harness.code(
            1,
            "update_project_config",
            json!({ "project": harness.project, "edits": [{ "op": "set_project_name", "name": "shop" }] })
        ),
        "invalid_args"
    );
}
