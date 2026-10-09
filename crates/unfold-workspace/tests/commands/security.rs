//! Path confinement, trusted-only commands, malformed commands, and errors from Rust.

use serde_json::json;

use crate::common::Harness;

#[test]
fn every_path_a_window_names_stays_inside_the_folder() {
    let harness = Harness::compiled("ts-orders");
    for bad in [
        "../",
        "..",
        "/etc/passwd",
        "src/../../x",
        "src//orders.ts",
        "C:\\x",
        "src\\routes",
    ] {
        assert_eq!(
            harness.code(1, "list_dir", json!({ "path": bad })),
            "invalid_path",
            "{bad}"
        );
        assert_eq!(
            harness.code(1, "file_view", json!({ "path": bad })),
            "invalid_path",
            "{bad}"
        );
        assert_eq!(
            harness.code(1, "file_outline", json!({ "path": bad })),
            "invalid_path",
            "{bad}"
        );
        assert_eq!(
            harness.code(1, "reveal_in_finder", json!({ "path": bad })),
            "invalid_path",
            "{bad}"
        );
        assert_eq!(
            harness.code(1, "open_in_editor", json!({ "path": bad })),
            "invalid_path",
            "{bad}"
        );
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret.ts"), "export const s = 1;\n").expect("write");
        std::os::unix::fs::symlink(outside.path(), harness.root.join("escape")).expect("link");
        assert_eq!(
            harness.code(1, "list_dir", json!({ "path": "escape" })),
            "invalid_path"
        );
        assert_eq!(
            harness.code(1, "file_view", json!({ "path": "escape/secret.ts" })),
            "invalid_path"
        );
        // The link itself is listed, marked, and never followed.
        let root = harness.ok(1, "list_dir", json!({ "path": "" }));
        let link = root["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|entry| entry["name"] == "escape")
            .expect("the link is listed");
        assert_eq!(link["kind"], "symlink");
    }
    // Board IDs are paths too.
    assert_eq!(
        harness.code(1, "open_board", json!({ "id": ".unfold/boards/../../x.json" })),
        "invalid_args"
    );
}

#[test]
fn absolute_paths_and_other_folders_need_the_main_process() {
    let harness = Harness::compiled("ts-orders");
    let board = harness.ok(1, "new_board", json!({ "title": "Export me" }));
    let target = harness.root.parent().expect("parent").join("export.mmd");
    let args = json!({ "board": board["id"], "path": target });
    assert_eq!(harness.code(1, "export_mermaid", args.clone()), "untrusted");
    assert!(!target.exists());
    let written = harness
        .call_with(
            1,
            json!({ "command": "export_mermaid", "args": args, "trusted": true }),
        )
        .expect("trusted export");
    assert!(written["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    let text = std::fs::read_to_string(&target).expect("exported");
    assert!(text.starts_with("flowchart LR"));
    let clipboard = harness.ok(1, "export_mermaid_text", json!({ "board": board["id"] }));
    assert_eq!(clipboard["text"], text);
    assert_eq!(
        harness.code(
            1,
            "export_mermaid_text",
            json!({ "board": board["id"], "entrance": "x" })
        ),
        "invalid_args"
    );

    let root = harness.root.to_str().expect("utf-8");
    assert_eq!(
        harness.code(1, "folder_windows", json!({ "root": root })),
        "untrusted"
    );
    let windows = harness
        .call_with(
            1,
            json!({ "command": "folder_windows", "args": { "root": root }, "trusted": true }),
        )
        .expect("trusted");
    assert_eq!(windows["folder"], root);
}

#[test]
fn malformed_commands_are_rejected_with_codes() {
    let harness = Harness::compiled("ts-orders");
    for (command, code) in [
        ("not json", "invalid_command"),
        (r#"{"command":"drop_tables"}"#, "invalid_command"),
        (r#"{"command":"card","args":{"key":1}}"#, "invalid_args"),
        (
            r#"{"command":"list_dir","args":{"path":"","extra":true}}"#,
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
        harness.code(1, "card", json!({ "key": "src/nope.ts#function:x" })),
        "not_found"
    );
    assert_eq!(harness.code(2, "graph_status", json!({})), "not_attached");
}

#[test]
fn async_dispatch_answers_through_its_callback() {
    // The addon resolves its promise from `done`, on the command thread. (A panic there becomes
    // an `internal` error: see `host::tests::spawned_panics_reach_done_as_internal_errors`.)
    let harness = Harness::compiled("ts-orders");
    let (sender, receiver) = std::sync::mpsc::channel();
    harness.host.dispatch_async(
        1,
        harness.request(),
        r#"{"command":"graph_status"}"#.to_owned(),
        move |result| {
            let _ = sender.send(result);
        },
    );
    let status = receiver
        .recv_timeout(crate::common::PATIENCE)
        .expect("done is called")
        .expect("graph_status");
    assert!(status.contains("\"state\""));
    let (sender, receiver) = std::sync::mpsc::channel();
    harness
        .host
        .dispatch_async(1, harness.request(), "{".to_owned(), move |result| {
            let _ = sender.send(result);
        });
    let error = receiver
        .recv_timeout(crate::common::PATIENCE)
        .expect("done is called")
        .expect_err("malformed");
    assert_eq!(error.code, "invalid_command");
}
