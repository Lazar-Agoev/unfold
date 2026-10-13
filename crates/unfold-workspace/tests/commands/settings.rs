//! The app settings (`settings.json`): the editor command only, and opening a project's
//! `unfold.toml` with it.

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

#[test]
fn opening_unfold_toml_creates_an_empty_one_first() {
    let harness = Harness::new("ts-orders", false);
    // An editor command that does nothing, so the test opens no app.
    harness.ok(
        1,
        "set_settings",
        json!({ "settings": { "editorCommand": "/usr/bin/true" } }),
    );
    let config = harness.root.join("unfold.toml");
    std::fs::remove_file(&config).expect("remove unfold.toml");
    let open = json!({ "project": harness.project, "path": "unfold.toml" });
    assert_eq!(harness.ok(1, "open_in_editor", open.clone()), Value::Null);
    assert_eq!(std::fs::read_to_string(&config).expect("created"), "");
    // An existing file is opened as it is.
    std::fs::write(&config, "languages = [\"typescript\"]\n").expect("write");
    harness.ok(1, "open_in_editor", open);
    assert_eq!(
        std::fs::read_to_string(&config).expect("read"),
        "languages = [\"typescript\"]\n"
    );
    // Any other missing file is not created.
    assert_eq!(
        harness.code(
            1,
            "open_in_editor",
            json!({ "project": harness.project, "path": "src/missing.ts" })
        ),
        "not_found"
    );
    assert!(!harness.root.join("src/missing.ts").exists());
}
