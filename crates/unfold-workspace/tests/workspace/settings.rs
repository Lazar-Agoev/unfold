//! Settings edits through a workspace: comments survive, defaults aren't written, the graph
//! follows, and invalid edits are refused.

use unfold_workspace::{SettingsEdit, WorkspaceError};

use crate::common::{Playground, RecordingSink, compile, has_block, settle};

#[test]
fn excluding_and_including_keeps_comments_and_updates_the_graph() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let original = workspace.settings_text().expect("read").expect("present");
    assert!(original.contains("# Entrances the detectors can't see"));

    let mark = sink.mark();
    let outcome = workspace
        .edit_settings(&[SettingsEdit::ExcludeFromGraph {
            path: "src/lib".into(),
        }])
        .expect("exclude");
    assert!(outcome.written);
    let text = std::fs::read_to_string(playground.path("unfold.toml")).expect("read");
    assert_eq!(text, outcome.text);
    assert!(text.starts_with("# Unfold settings for the ts-orders playground."));
    assert!(
        text.contains(
            "# Entrances the detectors can't see: CLI dispatch and a signal handler.\n[[entrance]]"
        )
    );
    assert!(text.contains("exclude = [\"src/lib\"]"), "{text}");
    assert_eq!(
        workspace.settings().expect("settings").scope.exclude(),
        ["src/lib".into()]
    );
    assert_eq!(sink.of_kind(mark, "config_changed").len(), 1);
    settle(&workspace);
    assert!(!has_block(&workspace, "src/lib/retry.ts", "retry"));

    let outcome = workspace
        .edit_settings(&[SettingsEdit::IncludeInGraph {
            path: "src/lib".into(),
        }])
        .expect("include");
    assert!(outcome.settings.scope.exclude().is_empty());
    assert!(outcome.text.contains("# Entrances the detectors can't see"));
    settle(&workspace);
    let reader = workspace.readers().expect("readers").get().expect("reader");
    assert!(reader.file("src/lib/retry.ts").expect("file").is_some());
}

#[test]
fn defaults_are_never_written_and_invalid_edits_are_refused() {
    let playground = Playground::new("ts-orders", false);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let outcome = workspace
        .edit_settings(&[SettingsEdit::IncludeInGraph { path: "src".into() }])
        .expect("no-op edit");
    assert!(!outcome.written);
    assert!(!playground.path(".unfold").exists());

    let refused = workspace
        .edit_settings(&[SettingsEdit::AddEvidencePattern {
            pattern: "../secrets.sql".into(),
        }])
        .expect_err("a pattern outside the folder");
    assert!(
        matches!(refused, WorkspaceError::InvalidSettings { line: Some(_), .. }),
        "{refused}"
    );
    assert!(!playground.path(".unfold").exists());

    let written = workspace
        .edit_settings(&[SettingsEdit::GraphOnly { path: "src".into() }])
        .expect("graph only src");
    assert!(written.written);
    assert_eq!(
        workspace.settings().expect("settings").scope.include(),
        ["src".into()]
    );
    // A settings file appeared, but nothing else under .unfold/ until the first Compile.
    assert!(!playground.path(".unfold/local").exists());
}

#[test]
fn an_invalid_settings_file_is_a_problem_with_a_position() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    playground.write("unfold.toml", "[scope]\nbogus = [\"x\"]\n");
    compile(&workspace);
    let problems = workspace.problems();
    let settings = problems
        .iter()
        .find(|problem| problem.path == "unfold.toml")
        .expect("a settings problem");
    assert_eq!(settings.line, Some(2), "{settings:?}");
    assert!(!workspace.status().complete);
    assert!(matches!(
        workspace.settings(),
        Err(WorkspaceError::InvalidSettings { line: Some(2), .. })
    ));
}
