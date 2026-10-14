//! Opening, the first Compile, reopening, the lock, read-only folders, losing the cache,
//! cancellation, confinement, and remembered state.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use unfold_core::vocab::Language;
use unfold_workspace::{GraphState, LocalState, ProjectState, Workspace, WorkspaceError, WorkspaceEvent};

use crate::common::{Playground, RecordingSink, compile, has_block, project, settle, updates};

#[test]
fn opening_writes_nothing_and_remembered_state_is_written_before_any_compile() {
    let playground = Playground::new("ts-orders", false);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    assert_eq!(workspace.status().state, GraphState::NotCompiled);
    assert_eq!(workspace.project_state(), ProjectState::NotCompiled);
    assert_eq!(workspace.project(), &project());
    assert!(matches!(workspace.readers(), Err(WorkspaceError::NotCompiled)));
    settle(&workspace);
    assert!(
        !playground.path(".unfold").exists(),
        "opening wrote into the folder"
    );
    // Remembered state is written at once, before any compile, in a hidden `local/`.
    workspace
        .update_local_state(|state| state.empty_folders.push("Later".into()))
        .expect("state written");
    assert_eq!(
        std::fs::read_to_string(playground.path(".unfold/local/.gitignore")).expect(".gitignore"),
        "*\n"
    );
    assert!(playground.path(".unfold/local/state.json").is_file());

    let mark = sink.mark();
    let outcome = compile(&workspace);
    assert!(outcome.files > 20, "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(playground.path(".unfold/local/.gitignore")).expect(".gitignore"),
        "*\n"
    );
    for path in [
        ".unfold/local/cache/graph.sqlite",
        ".unfold/local/cache/lock",
        ".unfold/local/state.json",
    ] {
        assert!(playground.path(path).exists(), "{path} is missing");
    }
    assert!(!playground.path("unfold.toml").exists());
    let saved: LocalState = serde_json::from_slice(
        &std::fs::read(playground.path(".unfold/local/state.json")).expect("state.json"),
    )
    .expect("state.json parses");
    assert_eq!(saved.empty_folders, ["Later"], "the compile kept the state");
    match workspace.status().state {
        GraphState::Current { files, languages }
        | GraphState::DoesntParse {
            total: files,
            languages,
            ..
        } => {
            assert_eq!(files, outcome.files);
            assert_eq!(languages, [Language::TypeScript]);
        }
        other => panic!("unexpected status {other:?}"),
    }
    let updated = sink
        .wait_for(mark, |event| event.kind() == "graph_updated")
        .expect("graph_updated");
    assert!(matches!(
        updated,
        WorkspaceEvent::GraphUpdated { everything: true, .. }
    ));
    assert!(!sink.of_kind(mark, "compile_progress").is_empty());
    assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
    let reader = workspace.readers().expect("readers").get().expect("reader");
    let file = reader
        .file_graph("src/services/order-service.ts")
        .expect("query")
        .expect("indexed");
    let index = workspace.reader_index().expect("index");
    assert!(file.nodes.iter().all(|node| index.contains(node.key.as_str())));
}

#[test]
fn reopening_is_readable_at_once_then_catches_up() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    workspace.close();
    drop(workspace);

    playground.write(
        "src/added.ts",
        "export function addedWhileClosed() {\n  return 1;\n}\n",
    );
    let mark = sink.mark();
    let workspace = playground.open(&sink, false);
    // The stored graph is there before the reopen check finishes.
    let reader = workspace.readers().expect("readers").get().expect("reader");
    assert!(reader.files().expect("files").len() > 20);
    drop(reader);
    sink.wait_for(mark, |event| updates(event, "src/added.ts"))
        .expect("the reopen check found the new file");
    settle(&workspace);
    assert!(has_block(&workspace, "src/added.ts", "addedWhileClosed"));
    assert!(matches!(
        workspace.status().state,
        GraphState::Current { .. } | GraphState::DoesntParse { .. }
    ));
}

#[test]
fn an_engine_version_change_recompiles_in_the_background() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    workspace.close();
    drop(workspace);
    // Another engine version takes the database over; ours must start it over.
    let database = playground.path(".unfold/local/cache/graph.sqlite");
    drop(
        unfold_store::Store::open(&database, &unfold_store::StoreConfig::new("an-older-engine"))
            .expect("reopen"),
    );

    let mark = sink.mark();
    let workspace = playground.open(&sink, false);
    sink.wait_for(mark, |event| event.kind() == "graph_updated")
        .expect("the recompile committed");
    settle(&workspace);
    assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
}

#[test]
fn a_second_copy_sees_the_folder_open_elsewhere() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    // A second open of the same folder outside the registry behaves like another process.
    let error = Workspace::open(&playground.root, playground.context(RecordingSink::new(), false))
        .expect_err("the lock is held");
    assert!(matches!(error, WorkspaceError::OpenElsewhere { .. }), "{error}");
    assert_eq!(error.code(), "open_elsewhere");
    workspace.close();
    let again = Workspace::open(&playground.root, playground.context(RecordingSink::new(), false))
        .expect("the lock was released");
    again.close();
}

#[test]
fn a_read_only_folder_is_refused_at_the_first_compile() {
    use std::os::unix::fs::PermissionsExt;

    let playground = Playground::new("ts-orders", false);
    let read_only = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&playground.root, read_only).expect("chmod");
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let outcome = workspace.compile(None).wait();
    std::fs::set_permissions(&playground.root, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    let error = outcome.expect_err("read-only");
    assert_eq!(error.code(), "folder_not_writable");
    assert!(!playground.path(".unfold").exists());
    assert_eq!(workspace.status().state, GraphState::NotCompiled);
}

#[test]
fn deleting_the_cache_while_open_recreates_it() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    std::fs::remove_dir_all(playground.path(".unfold/local/cache")).expect("delete cache");
    let mark = sink.mark();
    workspace.rescan();
    sink.wait_for(mark, |event| event.kind() == "graph_updated")
        .expect("recompiled");
    settle(&workspace);
    assert!(playground.path(".unfold/local/cache/graph.sqlite").exists());
    assert!(playground.path(".unfold/local/cache/lock").exists());
    assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
}

#[test]
fn deleting_the_cache_is_noticed_by_the_watcher() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, true);
    compile(&workspace);
    let mark = sink.mark();
    std::fs::remove_dir_all(playground.path(".unfold/local/cache")).expect("delete cache");
    sink.wait_for(mark, |event| event.kind() == "graph_updated")
        .expect("recompiled after the watcher saw the cache go");
    settle(&workspace);
    assert!(playground.path(".unfold/local/cache/graph.sqlite").exists());
}

#[test]
fn cancellation_and_progress() {
    let playground = Playground::new("py-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let ticket = workspace.compile(None);
    ticket.cancel();
    assert!(matches!(ticket.wait(), Err(WorkspaceError::Cancelled)));
    assert!(matches!(
        workspace.status().state,
        GraphState::NotCompiled | GraphState::Current { .. }
    ));
    let first = compile(&workspace);
    assert!(first.files > 10);
    let progress = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&progress);
    let again = workspace
        .compile(Some(Box::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        })))
        .wait()
        .expect("compile again");
    assert_eq!(again.files, first.files);
    assert!(progress.load(Ordering::Relaxed) > 0);
    assert!(matches!(
        workspace.status().state,
        GraphState::Current { ref languages, .. } | GraphState::DoesntParse { ref languages, .. }
            if languages == &[Language::Python]
    ));
}

/// Cancelling a Compile at any point (reading, extracting, linking, committing) is acknowledged
/// promptly and leaves the previous graph readable: the store only changes when the compiled
/// graph commits.
#[test]
fn a_cancelled_compile_keeps_the_previous_graph() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    let stored = || {
        let reader = workspace.readers().expect("readers").get().expect("reader");
        reader.graph().expect("graph")
    };
    let mut cancelled = 0;
    for delay in [0, 1, 2, 5, 10, 20, 40, 80, 160] {
        let before = stored();
        playground.write(
            "src/changing.ts",
            &format!("export function changed{delay}() {{\n  return {delay};\n}}\n"),
        );
        let ticket = workspace.compile(None);
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let asked = std::time::Instant::now();
        ticket.cancel();
        let result = ticket.wait();
        let acknowledged = asked.elapsed();
        // Generous for an unoptimized build under a parallel test run.
        assert!(
            acknowledged < std::time::Duration::from_secs(1),
            "{delay} ms: {acknowledged:?}"
        );
        match result {
            Err(WorkspaceError::Cancelled) => {
                cancelled += 1;
                assert_eq!(
                    stored(),
                    before,
                    "a compile cancelled after {delay} ms kept the graph"
                );
            }
            Ok(_) => assert!(has_block(
                &workspace,
                "src/changing.ts",
                &format!("changed{delay}")
            )),
            Err(error) => panic!("{delay} ms: {error}"),
        }
        settle(&workspace);
        assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
    }
    assert!(cancelled > 0, "some compiles were cancelled");
    compile(&workspace);
    assert!(has_block(&workspace, "src/changing.ts", "changed160"));
}

#[test]
fn paths_are_confined_to_the_folder() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    assert!(workspace.confine("src/server.ts").is_ok());
    assert!(workspace.confine("").is_ok());
    for bad in ["../ts-orders/src/server.ts", "/etc/passwd", "src/../../x", "a\0b"] {
        assert_eq!(workspace.confine(bad).expect_err(bad).code(), "invalid_path");
    }
    {
        let outside = tempfile::tempdir().expect("temp dir");
        std::os::unix::fs::symlink(outside.path(), playground.path("outside")).expect("link");
        assert!(workspace.confine("outside/anything").is_err());
    }
}

#[test]
fn concurrent_local_state_updates_leave_the_file_equal_to_the_final_state() {
    let playground = Playground::new("ts-orders", false);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    compile(&workspace);
    settle(&workspace);

    const THREADS: i32 = 8;
    const UPDATES: i32 = 40;
    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let workspace = &workspace;
            scope.spawn(move || {
                for update in 0..UPDATES {
                    workspace
                        .update_local_state(|state| {
                            state.empty_folders.push(format!("t{thread}-u{update}"));
                        })
                        .expect("concurrent writes all succeed");
                }
            });
        }
    });

    let on_disk: LocalState = serde_json::from_slice(
        &std::fs::read(playground.path(".unfold/local/state.json")).expect("state.json"),
    )
    .expect("state.json parses");
    let in_memory = workspace.local_state();
    assert_eq!(on_disk, in_memory);
    assert_eq!(in_memory.empty_folders.len(), (THREADS * UPDATES) as usize);
    let leftovers: Vec<_> = std::fs::read_dir(playground.path(".unfold/local"))
        .expect("local/")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
}
