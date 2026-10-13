//! The app settings (`settings.json`): the editor command only.

use serde_json::{Value, json};

use crate::common::Harness;

#[test]
fn settings_round_trip_through_settings_json() {
    let harness = Harness::new("ts-orders", false);
    assert_eq!(harness.ok(1, "get_settings", json!({})), json!({}));
    let settings = json!({ "editorCommand": "code -g {path}:{line}" });
    assert_eq!(
        harness.ok(1, "set_settings", json!({ "settings": settings })),
        settings
    );
    assert_eq!(harness.ok(1, "get_settings", json!({})), settings);
    let text = std::fs::read_to_string(harness.base.join("app/settings.json")).expect("settings.json");
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), settings);
    // Clearing the command leaves nothing in the file.
    harness.ok(1, "set_settings", json!({ "settings": {} }));
    assert_eq!(harness.ok(1, "get_settings", json!({})), json!({}));
}

#[test]
fn rejected_settings_are_not_saved() {
    let harness = Harness::new("ts-orders", false);
    for settings in [
        json!({ "editorCommand": "code\u{0}" }),
        json!({ "theme": "dark" }),
        json!({ "editorCommand": "code", "threadLimit": 2 }),
    ] {
        assert_eq!(
            harness.code(1, "set_settings", json!({ "settings": settings })),
            "invalid_args",
            "{settings}"
        );
    }
    assert!(!harness.base.join("app/settings.json").exists());
}
