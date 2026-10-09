//! Remembered windows through the host: a window's last state save survives its detach.

use serde_json::{Value, json};

use crate::common::{Harness, PATIENCE};

#[test]
fn closing_a_window_waits_for_its_last_window_state_save() {
    // When the app quits, a window sends its pending state save as it closes, and the main
    // process detaches it at once on another channel. The close must let the save land, or the
    // window comes back without its last tabs.
    let harness = Harness::compiled("ts-orders");
    for round in 0..5 {
        if round > 0 {
            harness.open(1);
        }
        let board = format!(".unfold/local/drafts/board-{round}.json");
        let state = json!({
            "key": "w1",
            "tabs": [{ "kind": "board", "board": board }],
            "activeTab": 0,
            "openBoards": [board],
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        harness.host.dispatch_async(
            1,
            harness.request(),
            json!({ "command": "save_window_state", "args": { "state": state } }).to_string(),
            move |result| {
                let _ = sender.send(result);
            },
        );
        harness.host.close_workspace(1).expect("close");
        receiver
            .recv_timeout(PATIENCE)
            .expect("the save finishes")
            .unwrap_or_else(|error| panic!("the save failed in round {round}: {error}"));
        let text =
            std::fs::read_to_string(harness.root.join(".unfold/local/state.json")).expect("state.json");
        let saved: Value = serde_json::from_str(&text).expect("state JSON");
        assert_eq!(saved["windows"][0]["key"], "w1", "round {round}");
        assert_eq!(saved["windows"][0]["openBoards"], json!([board]), "round {round}");
    }
}
