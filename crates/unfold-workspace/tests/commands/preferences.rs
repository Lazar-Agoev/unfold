//! `set_preferences` tells every window, whichever project it shows.

use serde_json::{Value, json};

use crate::common::Harness;

fn preference_messages(harness: &Harness) -> Vec<Value> {
    harness
        .sink
        .all()
        .into_iter()
        .filter(|message| message["event"]["type"] == "preferences_changed")
        .collect()
}

#[test]
fn saved_preferences_reach_every_window_with_the_new_values() {
    let harness = Harness::new("ts-orders", false);
    // Window 1 shows the project on Home; window 2 shows nothing yet.
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    let mut preferences = harness.ok(2, "get_preferences", json!({}));
    preferences["theme"] = json!("dark");
    preferences["keyBindings"] = json!({"palette.open": "Mod+Shift+K"});

    let saved = harness.ok(2, "set_preferences", json!({ "preferences": preferences }));
    assert_eq!(saved, preferences);

    // One broadcast, which the engine host delivers to every window it has.
    let messages = preference_messages(&harness);
    assert_eq!(
        messages,
        vec![json!({
            "kind": "broadcast",
            "event": {"type": "preferences_changed", "preferences": preferences},
        })]
    );

    // A window showing a project saving reaches the other window the same way.
    preferences["theme"] = json!("light");
    harness.ok(1, "set_preferences", json!({ "preferences": preferences }));
    let messages = preference_messages(&harness);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1]["kind"], "broadcast");
    assert_eq!(messages[1]["event"]["preferences"]["theme"], "light");
    assert_eq!(harness.ok(2, "get_preferences", json!({})), preferences);
}

#[test]
fn rejected_preferences_send_nothing() {
    let harness = Harness::new("ts-orders", false);
    let mut preferences = harness.ok(2, "get_preferences", json!({}));
    preferences["threadLimit"] = json!(0);
    assert_eq!(
        harness.code(2, "set_preferences", json!({ "preferences": preferences })),
        "invalid_args"
    );
    assert!(preference_messages(&harness).is_empty());
}

#[test]
fn agents_connect_off_by_default_and_never_on_a_system_port() {
    let harness = Harness::new("ts-orders", false);
    let mut preferences = harness.ok(2, "get_preferences", json!({}));
    assert_eq!(preferences["agents"], json!({"enabled": false, "port": 29180}));
    for port in [json!(80), json!(70_000)] {
        preferences["agents"] = json!({"enabled": true, "port": port});
        assert_eq!(
            harness.code(2, "set_preferences", json!({ "preferences": preferences })),
            "invalid_args"
        );
    }
    assert!(preference_messages(&harness).is_empty());

    preferences["agents"] = json!({"enabled": true, "port": 31000});
    let saved = harness.ok(2, "set_preferences", json!({ "preferences": preferences }));
    assert_eq!(saved["agents"], json!({"enabled": true, "port": 31000}));
}
