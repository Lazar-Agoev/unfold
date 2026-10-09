//! Copy Diagnostics: the summary carries counts and states, never paths, the home folder, or
//! the username.

use serde_json::json;

use crate::common::Harness;

/// Whether `text` has a token that looks like an absolute path (`/x`, `~/x`, or `C:\x`).
fn has_absolute_path(text: &str) -> Option<&str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | ';' | '"' | '\''))
        .find(|token| {
            let bytes = token.as_bytes();
            (token.starts_with('/') && token.len() > 1)
                || token.starts_with("~/")
                || (bytes.len() > 2
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && bytes[2] == b'\\')
        })
}

#[test]
fn diagnostics_are_redacted_counts() {
    let harness = Harness::compiled("ts-orders");
    let report = harness.ok(1, "diagnostics", json!({ "project": harness.project }));
    let text = report["text"].as_str().expect("text");
    for expected in [
        "Unfold diagnostics",
        "Engine version: ",
        "Producers: python=unfold-lang-python@",
        "Graph: current · complete",
        "Files by language: typescript ",
        "File states: current ",
        "Watching: off",
        "Fallback location in use: no",
        "Folder lock: held by this app",
        "Settings: valid · 5 manual entrances",
        "Problems: 0",
        "Last update: Compile",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
    assert_eq!(has_absolute_path(text), None, "{text}");
    let root = harness.root.to_str().expect("utf-8");
    assert!(!text.contains(root), "{text}");
    let name = harness
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert!(!text.contains(&format!("{name}/")), "{text}");
    for (variable, minimum) in [("HOME", 2), ("USERPROFILE", 2), ("USER", 3), ("USERNAME", 3)] {
        if let Ok(value) = std::env::var(variable)
            && value.chars().count() >= minimum
        {
            let standalone = text
                .split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\')))
                .any(|token| token.eq_ignore_ascii_case(&value));
            assert!(
                !text.contains(&format!("{value}/")) && !standalone,
                "{variable} leaked:\n{text}"
            );
        }
    }
    // No source text and no folder-relative paths either.
    assert!(!text.contains("src/"), "{text}");
    assert!(!text.contains("createOrder"), "{text}");
}

#[test]
fn diagnostics_work_without_a_project() {
    let harness = Harness::new("ts-orders", false);
    let report = harness.ok(7, "diagnostics", json!({}));
    let text = report["text"].as_str().expect("text");
    assert!(text.contains("Project: none"), "{text}");
    assert_eq!(has_absolute_path(text), None, "{text}");
}

#[test]
fn a_resting_project_reports_only_its_last_known_state() {
    let harness = Harness::new("ts-orders", false);
    let report = harness.ok(7, "diagnostics", json!({ "project": harness.project }));
    let text = report["text"].as_str().expect("text");
    assert!(
        text.contains("Project (resting)\n  State: last known state not compiled"),
        "{text}"
    );
    assert!(!text.contains("Files:"), "{text}");
    assert_eq!(
        harness.row(&harness.project)["live"],
        false,
        "diagnostics never wake"
    );
    assert_eq!(has_absolute_path(text), None, "{text}");
}
