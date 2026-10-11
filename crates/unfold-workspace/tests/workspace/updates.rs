//! Background updates: the watcher, batching, cancel-and-restart, files that break and are
//! fixed, and pausing.

use unfold_core::RepoPath;
use unfold_core::vocab::FileState;
use unfold_workspace::{GraphState, ProblemCategory, WatchMode, WorkspaceEvent};

use crate::common::{Playground, RecordingSink, compile, has_block, settle, updates};

fn path(text: &str) -> RepoPath {
    RepoPath::new(text).expect("path")
}

#[test]
fn a_save_updates_the_graph_through_the_watcher() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, true);
    assert_eq!(workspace.status().watching, WatchMode::Events);
    compile(&workspace);
    let mark = sink.mark();
    playground.write(
        "src/lib/added.ts",
        "export function savedJustNow() {\n  return 2;\n}\n",
    );
    let updated = sink
        .wait_for(mark, |event| updates(event, "src/lib/added.ts"))
        .expect("graph_updated for the saved file");
    assert_eq!(updated.project(), Some(&crate::common::project()));
    assert!(
        sink.wait_for(mark, |event| matches!(event, WorkspaceEvent::FilesChanged { paths, .. } if paths.iter().any(|p| p == "src/lib/added.ts")))
            .is_some()
    );
    settle(&workspace);
    assert!(has_block(&workspace, "src/lib/added.ts", "savedJustNow"));
}

#[test]
fn changes_in_quick_succession_become_one_update() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    for index in 0..5 {
        let name = format!("src/batch/file{index}.ts");
        playground.write(&name, &format!("export function batched{index}() {{}}\n"));
        workspace.notify_changes([path(&name)]);
    }
    settle(&workspace);
    let log = workspace.index_log();
    assert_eq!(log.len(), 2, "{log:#?}");
    assert_eq!(log[1].action, "Background update");
    for index in 0..5 {
        assert!(has_block(
            &workspace,
            &format!("src/batch/file{index}.ts"),
            &format!("batched{index}")
        ));
    }
}

#[test]
fn a_newer_change_restarts_the_update_and_the_last_write_wins() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let target = "src/lib/retry.ts";
    // Each change lands while the previous update is probably queued or running; whatever the
    // timing, the graph must end on the last version.
    for round in 0..8 {
        playground.write(target, &format!("export function version{round}() {{}}\n"));
        workspace.notify_changes([path(target)]);
        std::thread::yield_now();
    }
    settle(&workspace);
    assert!(has_block(&workspace, target, "version7"));
    assert!(!has_block(&workspace, target, "version6"));
}

#[test]
fn a_file_that_breaks_keeps_its_last_good_facts_until_fixed() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    assert!(workspace.problems().is_empty(), "{:#?}", workspace.problems());
    let target = "src/services/order-service.ts";
    let original = std::fs::read_to_string(playground.path(target)).expect("read");

    let mark = sink.mark();
    playground.write(target, &format!("{original}\nexport function broken( {{\n"));
    workspace.notify_changes([path(target)]);
    settle(&workspace);
    assert!(
        matches!(workspace.status().state, GraphState::DoesntParse { files: 1, .. }),
        "{:?}",
        workspace.status()
    );
    let problems = workspace.problems();
    let problem = problems
        .iter()
        .find(|problem| problem.path == target)
        .expect("a problem for the broken file");
    assert_eq!(problem.category, ProblemCategory::DoesntParse);
    assert_eq!(problem.file_state, Some(FileState::LastGood));
    assert!(problem.line.is_some_and(|line| line > 40), "{problem:?}");
    assert!(!sink.of_kind(mark, "problems_changed").is_empty());
    // The last good facts stay in the graph.
    assert!(has_block(&workspace, target, "place"));

    playground.write(target, &original);
    workspace.notify_changes([path(target)]);
    settle(&workspace);
    assert!(
        matches!(workspace.status().state, GraphState::Current { .. }),
        "{:?}",
        workspace.status()
    );
    assert!(workspace.problems().is_empty());
}

/// Rebuild from scratch drops last-good facts (docs/engine.md#updates): a broken file becomes
/// failed, and a later edit that is still broken doesn't bring the last good version back from
/// the facts cache.
#[test]
fn rebuild_from_scratch_drops_last_good_facts_for_good() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let target = "src/services/order-service.ts";
    let original = std::fs::read_to_string(playground.path(target)).expect("read");
    playground.write(target, &format!("{original}\nexport function broken( {{\n"));
    workspace.notify_changes([path(target)]);
    settle(&workspace);
    assert!(has_block(&workspace, target, "place"), "last good facts at first");

    workspace
        .compile(unfold_workspace::CompileMode::FromScratch, None)
        .wait()
        .expect("rebuild");
    let state = |workspace: &unfold_workspace::Workspace| {
        let reader = workspace.readers().expect("readers").get().expect("reader");
        let file = reader.file_graph(target).expect("file graph").expect("the file");
        file.file.state
    };
    assert_eq!(state(&workspace), FileState::Failed);

    playground.write(target, &format!("{original}\nexport function still_broken( {{\n"));
    workspace.notify_changes([path(target)]);
    settle(&workspace);
    assert_eq!(
        state(&workspace),
        FileState::Failed,
        "no last good version came back"
    );
    assert!(!has_block(&workspace, target, "place"));
}

#[test]
fn paused_folders_count_changes_and_catch_up_when_resumed() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    workspace.set_background_updates(false).expect("pause");
    let state = std::fs::read_to_string(playground.path(".unfold/local/state.json")).expect("state");
    assert!(state.contains("\"backgroundUpdates\": false"), "{state}");

    playground.write("src/paused/a.ts", "export function whilePausedA() {}\n");
    playground.write("src/paused/b.ts", "export function whilePausedB() {}\n");
    workspace.notify_changes([path("src/paused/a.ts"), path("src/paused/b.ts")]);
    settle(&workspace);
    assert_eq!(workspace.status().state, GraphState::Paused { changed: 2 });
    assert_eq!(workspace.index_log().len(), 1);

    workspace.set_background_updates(true).expect("resume");
    settle(&workspace);
    assert!(has_block(&workspace, "src/paused/a.ts", "whilePausedA"));
    assert_eq!(workspace.index_log().len(), 2);
    assert!(workspace.background_updates());
}

#[test]
fn deleting_a_file_removes_it() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let mark = sink.mark();
    std::fs::remove_file(playground.path("src/lib/calendar.ts")).expect("delete");
    workspace.notify_changes([path("src/lib/calendar.ts")]);
    let event = sink
        .wait_for(mark, |event| event.kind() == "graph_updated")
        .expect("graph_updated");
    assert!(
        matches!(event, WorkspaceEvent::GraphUpdated { removed_files, .. } if removed_files == ["src/lib/calendar.ts"])
    );
    settle(&workspace);
    let reader = workspace.readers().expect("readers").get().expect("reader");
    assert!(reader.file("src/lib/calendar.ts").expect("file").is_none());
}
