//! Remembered windows (`windows.json`): what the UI saves, what the main process saves and
//! forgets, the trusted-only commands, and a window's last save surviving its detach.

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE};

fn state(key: &str, project: &str, board: &str) -> Value {
    json!({
        "key": key,
        "selectedProject": project,
        "tabs": [{ "project": project, "board": board, "page": "page-1" }],
        "activeTab": 0,
        "layout": { "sidebar": { "width": 280 } },
    })
}

#[test]
fn windows_remember_state_and_bounds_separately() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(harness.ok(1, "window_state", json!({ "key": "w1" })), Value::Null);
    let board = ".unfold/boards/orders.json";
    harness.ok(
        1,
        "save_window_state",
        json!({ "state": state("w1", &harness.project, board) }),
    );
    let bounds = json!({ "x": 80, "y": 60, "width": 1440, "height": 900 });
    harness
        .trusted(
            "save_window_bounds",
            json!({ "key": "w1", "bounds": bounds, "maximized": true }),
        )
        .expect("bounds");
    // A later state save keeps the bounds.
    harness.ok(
        1,
        "save_window_state",
        json!({ "state": state("w1", &harness.project, ".unfold/boards/other.json") }),
    );
    let saved = harness.ok(1, "window_state", json!({ "key": "w1" }));
    assert_eq!(saved["bounds"], bounds);
    assert_eq!(saved["maximized"], true);
    assert_eq!(saved["selectedProject"], harness.project);
    assert_eq!(saved["tabs"][0]["board"], ".unfold/boards/other.json");
    assert_eq!(saved["activeTab"], 0);
    assert_eq!(saved["layout"]["sidebar"]["width"], 280);

    // Bounds alone make a window come back too.
    harness
        .trusted(
            "save_window_bounds",
            json!({ "key": "w2", "bounds": bounds, "maximized": false }),
        )
        .expect("bounds");
    let windows = harness.trusted("saved_windows", json!({})).expect("saved");
    let keys: Vec<&str> = windows["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .filter_map(|window| window["key"].as_str())
        .collect();
    assert_eq!(keys, ["w1", "w2"]);
    harness
        .trusted("forget_window", json!({ "key": "w1" }))
        .expect("forget");
    assert_eq!(harness.ok(1, "window_state", json!({ "key": "w1" })), Value::Null);
    let text = std::fs::read_to_string(harness.base.join("app/windows.json")).expect("windows.json");
    let file: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(file["version"], 1);
    assert_eq!(file["windows"][0]["key"], "w2");
    // Nothing about windows is written in a project folder.
    assert!(!harness.root.join(".unfold/local").exists());
}

#[test]
fn the_main_process_owns_window_lifecycle() {
    let harness = Harness::new("ts-orders", false);
    let bounds = json!({ "x": 0, "y": 0, "width": 800, "height": 600 });
    for (command, args) in [
        ("saved_windows", json!({})),
        (
            "save_window_bounds",
            json!({ "key": "w1", "bounds": bounds, "maximized": false }),
        ),
        ("forget_window", json!({ "key": "w1" })),
    ] {
        assert_eq!(harness.code(1, command, args), "untrusted", "{command}");
    }
}

#[test]
fn window_state_is_bounded() {
    let harness = Harness::new("ts-orders", false);
    let tab = json!({ "project": harness.project, "board": ".unfold/boards/a.json", "page": null });
    let mut too_many = state("w1", &harness.project, "x");
    too_many["tabs"] = Value::Array(vec![tab; 101]);
    assert_eq!(
        harness.code(1, "save_window_state", json!({ "state": too_many })),
        "invalid_args"
    );
    let mut too_big = state("w1", &harness.project, "x");
    too_big["layout"] = json!({ "blob": "x".repeat(70_000) });
    assert_eq!(
        harness.code(1, "save_window_state", json!({ "state": too_big })),
        "invalid_args"
    );
    assert_eq!(
        harness.code(
            1,
            "save_window_state",
            json!({ "state": state("", &harness.project, "x") })
        ),
        "invalid_args"
    );
    for index in 0..20 {
        harness.ok(
            1,
            "save_window_state",
            json!({ "state": state(&format!("w{index}"), &harness.project, "x") }),
        );
    }
    let windows = harness.trusted("saved_windows", json!({})).expect("saved");
    let windows = windows["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 16, "the oldest go first");
    assert_eq!(windows[0]["key"], "w4");
}

#[test]
fn detaching_a_window_waits_for_its_last_window_state_save() {
    // When the app quits, a window sends its pending state save as it closes, and the main
    // process detaches it at once on another channel. The detach must let the save land, or the
    // window comes back without its last tabs.
    let harness = Harness::compiled("ts-orders");
    for round in 0..5 {
        let board = format!(".unfold/local/drafts/board-{round}.json");
        let (sender, receiver) = std::sync::mpsc::channel();
        harness.host.dispatch_async(
            1,
            harness.request(),
            json!({ "command": "save_window_state", "args": { "state": state("w1", &harness.project, &board) } })
                .to_string(),
            move |result| {
                let _ = sender.send(result);
            },
        );
        harness.host.detach_window(1).expect("detach");
        receiver
            .recv_timeout(PATIENCE)
            .expect("the save finishes")
            .unwrap_or_else(|error| panic!("the save failed in round {round}: {error}"));
        let text = std::fs::read_to_string(harness.base.join("app/windows.json")).expect("windows.json");
        let saved: Value = serde_json::from_str(&text).expect("windows JSON");
        assert_eq!(saved["windows"][0]["key"], "w1", "round {round}");
        assert_eq!(saved["windows"][0]["tabs"][0]["board"], board, "round {round}");
    }
}
