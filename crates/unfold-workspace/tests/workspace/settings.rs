//! Settings edits through a workspace: comments survive, defaults aren't written, the graph
//! follows, an edit of only `[[app]]` tables resolves the entrances without compiling, and
//! invalid edits are refused.

use unfold_core::RepoPath;
use unfold_core::vocab::TargetState;
use unfold_engine::settings::edit::{EntranceSpec, SettingsEdit};
use unfold_workspace::{Workspace, WorkspaceError, WorkspaceEvent};

use crate::common::{
    Playground, RecordingSink, committed_at, compile, drain_watcher, has_block, settle, starts_work,
};

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
    let workspace = playground.open(&sink, true);
    compile(&workspace);
    drain_watcher(&playground, &workspace, &sink);
    let before = committed_at(&workspace);

    let mark = sink.mark();
    let outcome = workspace
        .edit_settings(&[SettingsEdit::SetEntrance {
            app: "Server".into(),
            entrance: EntranceSpec {
                name: "Admin stats".into(),
                code: Some(vec!["src/routes/admin.ts#adminStats".into()]),
                ..EntranceSpec::default()
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
    // The watcher sees the write and its temporary file; neither is an outside change.
    drain_watcher(&playground, &workspace, &sink);
    let changed = sink.of_kind(mark, "files_changed");
    assert!(changed.is_empty(), "no source changed: {changed:?}");
    let started = sink.events_after(mark, starts_work);
    assert!(started.is_empty(), "no update ran: {started:?}");
    let events = sink.of_kind(mark, "graph_updated");
    assert!(events.is_empty(), "nothing compiled: {events:?}");
    assert_eq!(
        committed_at(&workspace),
        before,
        "the graph's commit time is kept"
    );

    let reader = workspace.readers().expect("readers").get().expect("reader");
    let apps = reader.apps().expect("apps");
    let admin = apps[0]
        .entrances
        .iter()
        .find(|entrance| entrance.key.as_str() == "entrance:server/admin-stats")
        .expect("the entrance");
    assert_eq!(admin.code[0].code.as_str(), "src/routes/admin.ts#adminStats");
    assert_eq!(admin.code[0].state, TargetState::Found);
    drop(reader);

    // The store holds what the graph and its apps came from: an unrelated change doesn't commit
    // the graph again.
    playground.write("README.md", "# Changed\n");
    drain_watcher(&playground, &workspace, &sink);
    assert!(
        !sink.events_after(mark, starts_work).is_empty(),
        "README.md was checked"
    );
    assert_eq!(committed_at(&workspace), before, "nothing to commit");

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
    drain_watcher(&playground, &workspace, &sink);
    let mark = sink.mark();
    let text = std::fs::read_to_string(playground.path("unfold.toml")).expect("read");
    playground.write("unfold.toml", &moved_admin_stats(&text));
    assert!(
        sink.wait_for(mark, |event| event.kind() == "entrances_changed")
            .is_some(),
        "entrances_changed"
    );
    settle(&workspace);
    let events = sink.of_kind(mark, "graph_updated");
    assert!(events.is_empty(), "nothing compiled: {events:?}");
    let started = sink.events_after(mark, starts_work);
    assert!(started.is_empty(), "no update ran: {started:?}");
    assert_eq!(admin_stats(&workspace), TargetState::Found);
}

#[test]
fn an_app_only_edit_never_updates_the_graph_whatever_changes_with_it() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    settle(&workspace);
    let settings = std::fs::read_to_string(playground.path("unfold.toml")).expect("read");
    let server = std::fs::read_to_string(playground.path("src/server.ts")).expect("read");

    // In one batch with a source file written again unchanged.
    let mark = sink.mark();
    playground.write("unfold.toml", &moved_admin_stats(&settings));
    playground.write("src/server.ts", &server);
    workspace.notify_changes(["unfold.toml", "src/server.ts"].map(|path| RepoPath::new(path).expect("path")));
    settle(&workspace);
    assert_eq!(sink.of_kind(mark, "entrances_changed").len(), 1);
    let events = sink.of_kind(mark, "graph_updated");
    assert!(events.is_empty(), "nothing compiled: {events:?}");
    assert_eq!(admin_stats(&workspace), TargetState::Found);

    // In a check of the whole folder.
    let mark = sink.mark();
    playground.write("unfold.toml", &settings);
    workspace.rescan();
    settle(&workspace);
    assert_eq!(sink.of_kind(mark, "entrances_changed").len(), 1);
    let events = sink.of_kind(mark, "graph_updated");
    assert!(events.is_empty(), "nothing compiled: {events:?}");
    assert_eq!(admin_stats(&workspace), TargetState::Missing);

    // A changed source in the same batch is still reported, and only it.
    let mark = sink.mark();
    playground.write("unfold.toml", &moved_admin_stats(&settings));
    playground.write(
        "src/server.ts",
        &format!("{server}\nexport function added() {{}}\n"),
    );
    workspace.notify_changes(["unfold.toml", "src/server.ts"].map(|path| RepoPath::new(path).expect("path")));
    settle(&workspace);
    assert_eq!(sink.of_kind(mark, "entrances_changed").len(), 1);
    let events = sink.of_kind(mark, "graph_updated");
    assert!(
        matches!(events.as_slice(), [WorkspaceEvent::GraphUpdated { changed_files, inputs_changed: false, .. }] if changed_files == &["src/server.ts"]),
        "{events:?}"
    );
}

/// `unfold.toml`'s text with the deliberately missing `adminStats` pointed at where it moved.
fn moved_admin_stats(text: &str) -> String {
    text.replace(
        "code = \"src/server.ts#adminStats\"",
        "code = \"src/routes/admin.ts#adminStats\"",
    )
}

/// The state of the stored `Admin stats` entrance's code.
fn admin_stats(workspace: &Workspace) -> TargetState {
    let reader = workspace.readers().expect("readers").get().expect("reader");
    let apps = reader.apps().expect("apps");
    let admin = apps[0]
        .entrances
        .iter()
        .find(|entrance| entrance.key.as_str() == "entrance:server/admin-stats")
        .expect("the entrance");
    admin.code[0].state
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
