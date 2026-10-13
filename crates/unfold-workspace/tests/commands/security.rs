//! Path confinement per project, trusted-only commands, malformed commands, and errors from
//! Rust.

use serde_json::json;

use crate::common::{Harness, PATIENCE};

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
        for command in ["file_view", "open_in_editor"] {
            assert_eq!(
                harness.code(1, command, json!({ "project": project, "path": bad })),
                "invalid_path",
                "{command} {bad}"
            );
        }
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(
            outside.path().join("secret.ts"),
            "export const secretValue42 = 1;\n",
        )
        .expect("write");
        std::os::unix::fs::symlink(outside.path(), harness.root.join("escape")).expect("link");
        for command in ["file_view", "open_in_editor"] {
            assert_eq!(
                harness.code(
                    1,
                    command,
                    json!({ "project": project, "path": "escape/secret.ts" })
                ),
                "invalid_path",
                "{command}"
            );
        }
        // Text search never reads through it either.
        let found = harness.ok(1, "search_text", json!({ "query": "secretValue42" }));
        assert_eq!(found["matches"], 0);
    }
}

#[test]
fn a_project_id_never_reaches_another_projects_files() {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    let python_file = "src/orders_playground/server.py";
    // The path is relative to the project named, so it is checked against that folder.
    assert_eq!(
        harness.code(
            1,
            "file_view",
            json!({ "project": harness.project, "path": format!("../py-orders/{python_file}") })
        ),
        "invalid_path"
    );
    assert_eq!(
        harness.code(
            1,
            "file_view",
            json!({ "project": harness.project, "path": python_file })
        ),
        "not_found"
    );
    let view = harness.ok(1, "file_view", json!({ "project": python, "path": python_file }));
    assert_eq!(view["path"], python_file);
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
fn only_the_main_process_exports_to_a_file() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.ok(
        1,
        "new_board",
        json!({ "project": harness.project, "title": "Export" }),
    );
    let target = harness.base.join("export.mmd");
    let export = json!({
        "project": harness.project,
        "board": { "id": board["id"], "page": "page-1" },
        "path": target,
    });
    let trusted = |window: u32, args: &serde_json::Value| {
        harness.call_with(
            window,
            json!({ "command": "export_mermaid", "args": args, "trusted": true }),
        )
    };
    assert_eq!(harness.code(1, "export_mermaid", export.clone()), "untrusted");
    assert!(!target.exists(), "a window never writes outside its projects");
    // The main process exports an open board to the path its dialog chose.
    let written = trusted(1, &export).expect("export");
    assert_eq!(written["path"], target.to_str().expect("utf-8"));
    let text = std::fs::read_to_string(&target).expect("the export");
    assert!(text.starts_with("flowchart LR"), "{text}");
    assert_eq!(written["bytes"], text.len());
    let mut relative = export;
    relative["path"] = json!("export.mmd");
    assert_eq!(trusted(1, &relative).expect_err("relative").code, "invalid_args");
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
        let json: serde_json::Value = serde_json::from_str(&error.to_json()).expect("error json");
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
            json!({ "project": harness.project, "key": "src/nope.ts#function:x" })
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
