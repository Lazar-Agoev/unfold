//! The remembered window (`window.json`): the main process saves and reads it, nothing else may,
//! and its last save survives the window's detach.

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE};

fn state(project: &str, board: Option<&str>) -> Value {
    json!({
        "bounds": { "x": 80, "y": 60, "width": 1440, "height": 900 },
        "maximized": false,
        "board": board.map(|board| json!({ "project": project, "board": board })),
    })
}

#[test]
fn the_window_comes_back_with_its_bounds_and_last_board() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(
        harness.trusted("window_state", json!({})).expect("state"),
        Value::Null
    );
    let shown = state(&harness.project, Some(".unfold/boards/orders.json"));
    harness
        .trusted("save_window_state", json!({ "state": shown }))
        .expect("save");
    assert_eq!(harness.trusted("window_state", json!({})).expect("state"), shown);
    // Home shows no board.
    let home = state(&harness.project, None);
    harness
        .trusted("save_window_state", json!({ "state": home }))
        .expect("save");
    assert_eq!(harness.trusted("window_state", json!({})).expect("state"), home);
    let text = std::fs::read_to_string(harness.base.join("app/window.json")).expect("window.json");
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), home);
    // Nothing about the window is written in a project folder.
    assert!(!harness.root.join(".unfold/local/window.json").exists());
}

#[test]
fn only_the_main_process_keeps_the_window_state() {
    let harness = Harness::new("ts-orders", false);
    let shown = state(&harness.project, None);
    assert_eq!(harness.code(1, "window_state", json!({})), "untrusted");
    assert_eq!(
        harness.code(1, "save_window_state", json!({ "state": shown })),
        "untrusted"
    );
    let mut extra = shown;
    extra["tabs"] = json!([]);
    assert_eq!(
        harness
            .trusted("save_window_state", json!({ "state": extra }))
            .expect_err("unknown field")
            .code,
        "invalid_args"
    );
}

#[test]
fn detaching_a_window_waits_for_its_last_window_state_save() {
    // When the app quits, the main process saves the window's state one last time and detaches
    // the window at once. The detach must let the save land, or the window comes back without
    // its last board.
    let harness = Harness::compiled("ts-orders");
    for round in 0..5 {
        let board = format!(".unfold/local/drafts/board-{round}.json");
        let shown = state(&harness.project, Some(&board));
        let (sender, receiver) = std::sync::mpsc::channel();
        harness.host.dispatch_async(
            1,
            harness.request(),
            json!({ "command": "save_window_state", "args": { "state": shown }, "trusted": true })
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
        let text = std::fs::read_to_string(harness.base.join("app/window.json")).expect("window.json");
        let saved: Value = serde_json::from_str(&text).expect("window JSON");
        assert_eq!(saved["board"]["board"], board, "round {round}");
    }
}
