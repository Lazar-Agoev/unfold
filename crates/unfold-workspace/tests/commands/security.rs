//! Path confinement per project, trusted-only commands, malformed commands, and errors from
//! Rust.

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE};

/// The arguments of the commands that name a project's file: the code panel's file target, and
/// opening it in the editor.
fn naming(command: &str, project: &str, path: &str) -> Value {
    match command {
        "code_view" => json!({ "project": project, "target": { "kind": "file", "path": path } }),
        _ => json!({ "project": project, "path": path }),
    }
}

const BAD_PATHS: [&str; 7] = [
    "../",
    "..",
    "/etc/passwd",
    "src/../../x",
    "src//orders.ts",
    "C:\\x",
    "src\\routes",
];

#[test]
fn every_path_a_window_names_stays_inside_its_project() {
    let harness = Harness::compiled("ts-orders");
    let project = &harness.project;
    for bad in BAD_PATHS {
        for command in ["code_view", "open_in_editor"] {
            assert_eq!(
                harness.code(1, command, naming(command, project, bad)),
                "invalid_path",
                "{command} {bad}"
            );
        }
    }
    {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(
            outside.path().join("secret.ts"),
            "export const secretValue42 = 1;\n",
        )
        .expect("write");
        std::os::unix::fs::symlink(outside.path(), harness.root.join("escape")).expect("link");
        for command in ["code_view", "open_in_editor"] {
            assert_eq!(
                harness.code(1, command, naming(command, project, "escape/secret.ts")),
                "invalid_path",
                "{command}"
            );
        }
        // Search never lists files through it either.
        let found = harness.ok(1, "search", json!({ "query": "secret", "kinds": ["files"] }));
        let hits = found["files"]["hits"].as_array().expect("hits");
        assert!(
            hits.iter().all(|hit| hit["path"] != "escape/secret.ts"),
            "{hits:?}"
        );
    }
}

#[test]
fn a_project_id_never_reaches_another_projects_files() {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(2, "select_project", json!({ "project": python }));
    harness.ok(2, "compile", json!({ "project": python }));
    let python_file = "src/orders_playground/server.py";
    // The path is relative to the project named, so it is checked against that folder.
    assert_eq!(
        harness.code(
            1,
            "code_view",
            naming(
                "code_view",
                &harness.project,
                &format!("../py-orders/{python_file}")
            )
        ),
        "invalid_path"
    );
    let elsewhere = harness.ok(1, "code_view", naming("code_view", &harness.project, python_file));
    assert_eq!(elsewhere["state"], "notFound");
    assert_eq!(elsewhere["lines"], json!([]));
    let view = harness.ok(1, "code_view", naming("code_view", &python, python_file));
    assert_eq!(view["file"], python_file);
    assert_eq!(view["state"], "current");
}

#[test]
fn absolute_paths_and_the_window_state_need_the_main_process() {
    let harness = Harness::new("ts-orders", false);
    for (command, args) in [
        ("add_project", json!({ "path": harness.root })),
        ("window_state", json!({})),
        (
            "save_window_state",
            json!({ "state": { "bounds": { "x": 0, "y": 0, "width": 1, "height": 1 }, "maximized": false, "board": null } }),
        ),
    ] {
        assert_eq!(harness.code(1, command, args.clone()), "untrusted", "{command}");
        assert!(harness.trusted(command, args).is_ok(), "{command}");
    }
}

#[test]
fn malformed_commands_are_rejected_with_codes() {
    let harness = Harness::compiled("ts-orders");
    for (command, code) in [
        ("not json", "invalid_command"),
        (r#"{"command":"drop_tables"}"#, "invalid_command"),
        (r#"{"command":"graph_status"}"#, "invalid_command"),
        (r#"{"command":"card","args":{"key":1}}"#, "invalid_args"),
        (r#"{"command":"card","args":{"key":"x"}}"#, "invalid_args"),
        (
            r#"{"command":"create_folder","args":{"project":"p-x","path":"","extra":true}}"#,
            "invalid_args",
        ),
    ] {
        let error = harness
            .host
            .dispatch(1, harness.request(), command)
            .expect_err(command);
        assert_eq!(error.code, code, "{command}");
        let json: Value = serde_json::from_str(&error.to_json()).expect("error json");
        assert_eq!(json["code"], code);
        assert!(
            json["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }
    assert_eq!(
        harness.code(
            1,
            "card",
            json!({ "project": harness.project, "key": "src/nope.ts#x" })
        ),
        "not_found"
    );
}

#[test]
fn async_dispatch_answers_through_its_callback() {
    // The addon resolves its promise from `done`, on the command thread. (A panic there becomes
    // an `internal` error: see `host::tests::spawned_panics_reach_done_as_internal_errors`.)
    let harness = Harness::new("ts-orders", false);
    let (sender, receiver) = std::sync::mpsc::channel();
    harness.host.dispatch_async(
        1,
        harness.request(),
        r#"{"command":"workspace"}"#.to_owned(),
        move |result| {
            let _ = sender.send(result);
        },
    );
    let workspace = receiver
        .recv_timeout(PATIENCE)
        .expect("done is called")
        .expect("workspace");
    assert!(workspace.contains(&harness.project));
    let (sender, receiver) = std::sync::mpsc::channel();
    harness
        .host
        .dispatch_async(1, harness.request(), "{".to_owned(), move |result| {
            let _ = sender.send(result);
        });
    let error = receiver
        .recv_timeout(PATIENCE)
        .expect("done is called")
        .expect_err("malformed");
    assert_eq!(error.code, "invalid_command");
}
