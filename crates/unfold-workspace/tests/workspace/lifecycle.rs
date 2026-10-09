//! Opening, the first Compile, reopening, the lock, the fallback location, losing the cache,
//! Rebuild from scratch, and several windows on one folder.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use unfold_core::vocab::Language;
use unfold_workspace::{
    AppConfig, CompileMode, FallbackReason, GraphState, Registry, Workspace, WorkspaceError, WorkspaceEvent,
};

use crate::common::{Playground, RecordingSink, compile, has_block, options, settle, updates};

#[test]
fn opening_writes_nothing_and_the_first_compile_creates_local() {
    let playground = Playground::new("ts-orders", false);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    assert_eq!(workspace.status().state, GraphState::NotCompiled);
    assert!(matches!(workspace.readers(), Err(WorkspaceError::NotCompiled)));
    workspace
        .update_folder_state(|state| state.windows.clear())
        .expect("state in memory");
    settle(&workspace);
    assert!(
        !playground.path(".unfold").exists(),
        "opening wrote into the folder"
    );

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
        ".unfold/local/drafts",
        ".unfold/local/state.json",
    ] {
        assert!(playground.path(path).exists(), "{path} is missing");
    }
    assert!(!playground.path(".unfold/config.toml").exists());
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
    let (_, updated) = sink
        .wait_for(mark, |_, event| event.kind() == "graph_updated")
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
    assert!(file.blocks.iter().all(|block| index.contains(block.key.as_str())));
    assert_eq!(workspace.index_log().len(), 1);
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
    sink.wait_for(mark, |_, event| updates(event, "src/added.ts"))
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
    sink.wait_for(mark, |_, event| event.kind() == "graph_updated")
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

#[cfg(unix)]
#[test]
fn a_read_only_folder_uses_the_fallback_location() {
    use std::os::unix::fs::PermissionsExt;

    let playground = Playground::new("ts-orders", false);
    let read_only = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&playground.root, read_only).expect("chmod");
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let outcome = workspace.compile(CompileMode::Check, None).wait();
    std::fs::set_permissions(&playground.root, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    let outcome = outcome.expect("compile");
    assert!(outcome.files > 20);
    assert!(!playground.path(".unfold").exists());
    let status = workspace.status();
    assert_eq!(status.fallback, Some(FallbackReason::ReadOnly));
    let layout = workspace.layout();
    assert!(layout.local().starts_with(&playground.fallback));
    assert!(layout.database().exists());
    assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
    workspace.close();
    drop(workspace);
    // Reopening finds the graph in the fallback location.
    let workspace = playground.open(&sink, false);
    assert_eq!(workspace.status().fallback, Some(FallbackReason::ReadOnly));
    assert!(workspace.readers().is_ok());
    settle(&workspace);
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
    sink.wait_for(mark, |_, event| event.kind() == "graph_updated")
        .expect("recompiled");
    settle(&workspace);
    assert!(playground.path(".unfold/local/cache/graph.sqlite").exists());
    assert!(playground.path(".unfold/local/cache/lock").exists());
    assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
    assert!(playground.path(".unfold/local/state.json").exists());
}

#[test]
fn deleting_the_cache_is_noticed_by_the_watcher() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, true);
    compile(&workspace);
    let mark = sink.mark();
    std::fs::remove_dir_all(playground.path(".unfold/local/cache")).expect("delete cache");
    sink.wait_for(mark, |_, event| event.kind() == "graph_updated")
        .expect("recompiled after the watcher saw the cache go");
    settle(&workspace);
    assert!(playground.path(".unfold/local/cache/graph.sqlite").exists());
}

#[test]
fn rebuild_from_scratch_and_cancellation() {
    let playground = Playground::new("py-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let ticket = workspace.compile(CompileMode::Check, None);
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
    let rebuilt = workspace
        .compile(
            CompileMode::FromScratch,
            Some(Box::new(move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
            })),
        )
        .wait()
        .expect("rebuild");
    assert_eq!(rebuilt.files, first.files);
    assert!(progress.load(Ordering::Relaxed) > 0);
    assert!(matches!(
        workspace.status().state,
        GraphState::Current { ref languages, .. } | GraphState::DoesntParse { ref languages, .. }
            if languages == &[Language::Python]
    ));
}

/// Cancelling Rebuild from scratch at any point (reading, extracting, linking, committing) is
/// acknowledged promptly and leaves the previous graph readable: the store is only replaced
/// when the rebuilt graph commits.
#[test]
fn a_cancelled_rebuild_keeps_the_previous_graph() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let workspace = playground.open(&sink, false);
    let first = compile(&workspace);
    let stored = || {
        let reader = workspace.readers().expect("readers").get().expect("reader");
        reader.graph().expect("graph")
    };
    let before = stored();
    let mut cancelled = 0;
    for delay in [0, 1, 2, 5, 10, 20, 40, 80, 160] {
        let ticket = workspace.compile(CompileMode::FromScratch, None);
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let asked = std::time::Instant::now();
        ticket.cancel();
        let result = ticket.wait();
        let acknowledged = asked.elapsed();
        // Generous for an unoptimized build under a parallel test run; the release budget
        // (250 ms, architecture §17) is measured by the e2e budgets.
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
                    "a rebuild cancelled after {delay} ms kept the graph"
                );
            }
            Ok(rebuilt) => assert_eq!(rebuilt.files, first.files),
            Err(error) => panic!("{delay} ms: {error}"),
        }
        settle(&workspace);
        assert!(has_block(&workspace, "src/services/order-service.ts", "place"));
    }
    assert!(cancelled > 0, "some rebuilds were cancelled");
    let again = compile(&workspace);
    assert_eq!(again.files, first.files);
}

#[test]
fn two_windows_share_one_workspace() {
    let playground = Playground::new("ts-orders", true);
    let sink = RecordingSink::new();
    let config = AppConfig {
        app_data_dir: playground.fallback.join("app"),
        logs_dir: playground.fallback.join("logs"),
        fallback_cache_dir: playground.fallback.clone(),
    };
    let registry = Registry::with_options(config, Arc::clone(&sink) as _, options(false));
    let closed = Arc::new(AtomicUsize::new(0));
    let hook_count = Arc::clone(&closed);
    registry.set_close_hook(Arc::new(move |_| {
        hook_count.fetch_add(1, Ordering::Relaxed);
    }));
    let first = registry.open(1, &playground.root).expect("open 1");
    let second = registry.open(2, &playground.root.join("src/..")).expect("open 2");
    assert_eq!(first.folder, second.folder);
    assert_eq!(second.windows, [1, 2]);
    let one = registry.workspace(1).expect("window 1");
    let two = registry.workspace(2).expect("window 2");
    assert!(Arc::ptr_eq(&one, &two));
    assert_eq!(registry.workspaces().len(), 1);
    assert_eq!(registry.app().recent().folders[0].path, first.folder);
    assert!(
        sink.of_kind(0, "folder_opened")
            .iter()
            .any(|(windows, _)| windows == &[2])
    );

    let mark = sink.mark();
    compile(&one);
    let (windows, _) = sink
        .wait_for(mark, |_, event| event.kind() == "graph_updated")
        .expect("graph_updated");
    assert_eq!(windows, [1, 2]);

    one.claims().claim("orders", 1).expect("claim");
    assert!(matches!(
        two.claims().claim("orders", 2),
        Err(WorkspaceError::BoardHeld { window: 1, .. })
    ));
    one.transfer_board("orders", 1, 2).expect("move to window 2");
    assert!(one.transfer_board("orders", 2, 9).is_err());

    drop((one, two));
    registry.close(1);
    assert_eq!(closed.load(Ordering::Relaxed), 0);
    assert!(registry.workspace(2).is_ok());
    assert!(registry.workspace(1).is_err());
    registry.close(2);
    assert_eq!(closed.load(Ordering::Relaxed), 1);
    assert!(registry.workspaces().is_empty());
    // The lock went with the workspace.
    let again = registry.open(3, &playground.root).expect("reopen");
    assert!(again.compiled);
    registry.shutdown();
    assert_eq!(closed.load(Ordering::Relaxed), 2);
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
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&playground.fallback, playground.path("outside")).expect("link");
        assert!(workspace.confine("outside/anything").is_err());
    }
}

#[test]
fn concurrent_folder_state_updates_leave_the_file_equal_to_the_final_state() {
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
                        .update_folder_state(|state| {
                            state.put_window(unfold_workspace::WindowState {
                                key: format!("window-{thread}"),
                                bounds: Some(unfold_workspace::WindowBounds {
                                    x: update,
                                    y: thread,
                                    width: 800,
                                    height: 600,
                                }),
                                ..unfold_workspace::WindowState::default()
                            });
                        })
                        .expect("concurrent writes all succeed");
                }
            });
        }
    });

    let on_disk: unfold_workspace::FolderState = serde_json::from_slice(
        &std::fs::read(playground.path(".unfold/local/state.json")).expect("state.json"),
    )
    .expect("state.json parses");
    let in_memory = workspace.folder_state();
    assert_eq!(on_disk, in_memory);
    assert_eq!(in_memory.windows.len(), THREADS as usize);
    assert!(
        in_memory
            .windows
            .iter()
            .all(|window| window.bounds.is_some_and(|bounds| bounds.x == UPDATES - 1))
    );
    let leftovers: Vec<_> = std::fs::read_dir(playground.path(".unfold/local"))
        .expect("local/")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
}
