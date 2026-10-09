//! `set_preferences` tells every window: windows on a folder and windows that show none.

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
    // Window 1 shows the folder; window 2 is a welcome window, attached to nothing.
    harness.open(1);
    let mut preferences = harness.ok(2, "get_preferences", json!({}));
    preferences["theme"] = json!("dark");
    preferences["keyBindings"] = json!({"palette.open": "Mod+Shift+K"});

    let saved = harness.ok(2, "set_preferences", json!({ "preferences": preferences }));
    assert_eq!(saved, preferences);

    // One broadcast, which the engine host delivers to every window it has, folder or not.
    let messages = preference_messages(&harness);
    assert_eq!(
        messages,
        vec![json!({
            "kind": "broadcast",
            "event": {"type": "preferences_changed", "preferences": preferences},
        })]
    );

    // A window on a folder saving reaches the welcome window the same way.
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
