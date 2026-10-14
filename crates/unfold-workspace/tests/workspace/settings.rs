//! Settings edits through a workspace: comments survive, defaults aren't written, the graph
//! follows, an edit of only `[[app]]` tables resolves the entrances without compiling, and
//! invalid edits are refused.

use unfold_core::vocab::TargetState;
use unfold_engine::settings::edit::{EntranceSpec, SettingsEdit};
use unfold_workspace::{WorkspaceError, WorkspaceEvent};

use crate::common::{Playground, RecordingSink, compile, has_block, settle};

#[test]
fn excluding_and_including_keeps_comments_and_updates_the_graph() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let original = workspace.settings_text().expect("read").expect("present");
    assert!(original.contains("# Deliberately missing: adminStats moved"));

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
    assert!(text.contains("# Deliberately missing: adminStats moved"));
    assert!(text.contains("exclude = [\"src/lib\"]"), "{text}");
    assert!(
        outcome.diff.contains("+exclude = [\"src/lib\"]"),
        "{}",
        outcome.diff
    );
    assert_eq!(
        workspace.settings().expect("settings").settings.scope.exclude(),
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
    assert!(outcome.file.settings.scope.exclude().is_empty());
    assert!(outcome.text.contains("# Deliberately missing: adminStats moved"));
    settle(&workspace);
    let reader = workspace.readers().expect("readers").get().expect("reader");
    assert!(reader.file("src/lib/retry.ts").expect("file").is_some());
}

#[test]
fn an_app_only_edit_resolves_entrances_without_compiling() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    settle(&workspace);
    let compiled_at = || {
        let reader = workspace.readers().expect("readers").get().expect("reader");
        reader.meta().expect("meta").graph.expect("graph").compiled_at_ms
    };
    let before = compiled_at();

    let mark = sink.mark();
    let outcome = workspace
        .edit_settings(&[SettingsEdit::SetEntrance {
            app: "Server".into(),
            entrance: EntranceSpec {
                name: "Admin stats".into(),
                on: Some("GET /admin/stats".into()),
                code: vec!["src/routes/admin.ts#adminStats".into()],
                about: None,
            },
        }])
        .expect("set entrance");
    assert!(outcome.written);
    assert!(
        outcome.diff.contains("src/routes/admin.ts#adminStats"),
        "{}",
        outcome.diff
    );
    // The edit returns once the entrances are resolved again: the event is already out.
    assert_eq!(
        sink.of_kind(mark, "entrances_changed"),
        [WorkspaceEvent::EntrancesChanged {
            project: crate::common::project()
        }]
    );
    settle(&workspace);
    assert!(sink.of_kind(mark, "graph_updated").is_empty(), "nothing compiled");
    assert!(
        sink.of_kind(mark, "compile_progress").is_empty(),
        "nothing compiled"
    );
    assert_eq!(compiled_at(), before, "the graph's commit time is kept");

    let reader = workspace.readers().expect("readers").get().expect("reader");
    let apps = reader.apps().expect("apps");
    let admin = apps[0]
        .entrances
        .iter()
        .find(|entrance| entrance.key.as_str() == "entrance:server/admin-stats")
        .expect("the entrance");
    assert_eq!(admin.code[0].code.as_str(), "src/routes/admin.ts#adminStats");
    assert_eq!(admin.code[0].state, TargetState::Found);

    // A scope edit goes through an update instead.
    let mark = sink.mark();
    workspace
        .edit_settings(&[SettingsEdit::ExcludeFromGraph {
            path: "src/lib".into(),
        }])
        .expect("exclude");
    settle(&workspace);
    assert!(!sink.of_kind(mark, "graph_updated").is_empty());
}

#[test]
fn an_outside_app_only_edit_resolves_entrances_without_compiling() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, true);
    compile(&workspace);
    settle(&workspace);
    let mark = sink.mark();
    let text = std::fs::read_to_string(playground.path("unfold.toml")).expect("read");
    playground.write(
        "unfold.toml",
        &text.replace(
            "code = \"src/server.ts#adminStats\"",
            "code = \"src/routes/admin.ts#adminStats\"",
        ),
    );
    assert!(
        sink.wait_for(mark, |event| event.kind() == "entrances_changed")
            .is_some(),
        "entrances_changed"
    );
    settle(&workspace);
    assert!(sink.of_kind(mark, "graph_updated").is_empty(), "nothing compiled");
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
    assert!(outcome.diff.is_empty());
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
    let refused = workspace
        .edit_settings(&[SettingsEdit::RemoveEntrance {
            app: "Nowhere".into(),
            entrance: None,
        }])
        .expect_err("no such app");
    assert!(
        matches!(refused, WorkspaceError::SettingsConflict(_)),
        "{refused}"
    );
    assert!(!playground.path(".unfold").exists());

    let written = workspace
        .edit_settings(&[SettingsEdit::GraphOnly { path: "src".into() }])
        .expect("graph only src");
    assert!(written.written);
    assert_eq!(
        workspace.settings().expect("settings").settings.scope.include(),
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
