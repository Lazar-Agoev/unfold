//! Streamed commands: compile progress arrives as stream messages tagged with its window and
//! request, the result resolves the request, and `cancel` or `cancel_compile` stops it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};
use unfold_workspace::{CommandError, WorkspaceError};

use crate::common::{Harness, PATIENCE, is_event};

/// Dispatches on a command thread, as the addon does; returns the request ID and the result.
fn start(
    harness: &Harness,
    window: u32,
    command: Value,
) -> (u64, mpsc::Receiver<Result<String, CommandError>>) {
    let request = harness.request();
    let (sender, receiver) = mpsc::channel();
    harness
        .host
        .dispatch_async(window, request, command.to_string(), move |result| {
            let _ = sender.send(result);
        });
    (request, receiver)
}

/// Adds `count` TypeScript files of several lines each.
fn add_files(harness: &Harness, count: usize) {
    for index in 0..count {
        let mut text = String::new();
        for line in 0..20 {
            text.push_str(&format!(
                "export function f{index}_{line}(value: number): number {{ return value + {line}; }}\n"
            ));
        }
        harness.write(&format!("src/generated/file{index:04}.ts"), &text);
    }
}

fn stream_messages(harness: &Harness, request: u64) -> Vec<Value> {
    harness
        .sink
        .all()
        .into_iter()
        .filter(|message| message["kind"] == "stream" && message["requestId"] == request)
        .collect()
}

#[test]
fn compile_streams_progress_and_resolves_with_its_outcome() {
    let harness = Harness::new("ts-orders", false);
    let (request, result) = start(
        &harness,
        1,
        json!({ "command": "compile", "args": { "project": harness.project } }),
    );
    let outcome: Value =
        serde_json::from_str(&result.recv_timeout(PATIENCE).expect("done").expect("compile")).expect("json");
    assert!(
        outcome["files"].as_u64().is_some_and(|files| files > 20),
        "{outcome}"
    );
    let progress = stream_messages(&harness, request);
    assert!(!progress.is_empty(), "no progress was streamed");
    for message in &progress {
        assert_eq!(message["windowId"], 1);
        assert!(message["message"]["stage"].is_string());
        assert!(message["message"]["done"].is_u64());
    }
    // Events went to every window, naming the project.
    assert!(
        harness
            .sink
            .all()
            .iter()
            .any(|message| is_event(message, "graph_updated", &harness.project))
    );
    assert!(
        harness
            .sink
            .all()
            .iter()
            .any(|message| is_event(message, "compile_progress", &harness.project))
    );
    assert!(harness.sink.all().iter().any(|message| is_event(
        message,
        "entrances_changed",
        &harness.project
    )));
}

#[test]
fn cancelling_a_compile_rejects_it_with_cancelled() {
    let harness = Harness::new("ts-orders", false);
    add_files(&harness, 400);
    let (request, result) = start(
        &harness,
        1,
        json!({ "command": "compile", "args": { "project": harness.project } }),
    );
    harness.host.cancel(request);
    let error = result
        .recv_timeout(PATIENCE)
        .expect("done")
        .expect_err("the compile was cancelled");
    assert_eq!(error.code, "cancelled");
    // Nothing was committed; a later compile works.
    harness.ok(1, "compile", json!({ "project": harness.project }));
}

/// Whether `search` finds a definition named `name`.
fn finds(harness: &Harness, name: &str) -> bool {
    harness.ok(1, "search", json!({ "query": name }))["definitions"]["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .any(|hit| hit["qualifiedName"] == name)
}

#[test]
fn cancel_compile_stops_the_running_compile_and_drops_the_queued_one() {
    let harness = Harness::compiled("ts-orders");
    harness.write("src/late.ts", "export function lateArrival(): void {}\n");
    // The sink runs on the thread that reports a compile's progress, so holding it at the
    // running compile's first progress message holds the compile there.
    let request = harness.request();
    let (reached_tx, reached) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(Some((reached_tx, release_rx)));
    harness.host.subscribe(move |message| {
        let message: Value = serde_json::from_str(&message).expect("sink messages are JSON");
        if message["kind"] != "stream" || message["requestId"] != request {
            return;
        }
        let held = gate.lock().expect("the gate").take();
        if let Some((reached, release)) = held {
            reached.send(()).expect("signal the test");
            release.recv().expect("wait for the test");
        }
    });
    let (sender, running) = mpsc::channel();
    let command = json!({ "command": "compile", "args": { "project": harness.project } });
    harness
        .host
        .dispatch_async(1, request, command.to_string(), move |result| {
            let _ = sender.send(result);
        });
    reached
        .recv_timeout(PATIENCE)
        .expect("the compile reports progress");

    // A second Compile waits behind the running one.
    let id = serde_json::from_value(json!(harness.project)).expect("a project id");
    let workspace = harness
        .host
        .projects()
        .expect("initialized")
        .handle(&id)
        .expect("the project")
        .live()
        .expect("a compiling project is live");
    let progressed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&progressed);
    let queued = workspace.compile(Some(Box::new(move |_| flag.store(true, Ordering::SeqCst))));

    assert_eq!(
        harness.ok(1, "cancel_compile", json!({ "project": harness.project })),
        json!({ "cancelled": true })
    );
    release.send(()).expect("release the running compile");
    let error = running
        .recv_timeout(PATIENCE)
        .expect("the running compile settles")
        .expect_err("it was cancelled");
    assert_eq!(error.code, "cancelled");
    let outcome = queued.wait_timeout(PATIENCE).expect("the queued compile settles");
    assert!(matches!(outcome, Err(WorkspaceError::Cancelled)), "{outcome:?}");
    assert!(!progressed.load(Ordering::SeqCst), "the queued compile never ran");

    // Neither committed, and nothing is left to cancel; the next compile works.
    assert_eq!(
        harness.ok(1, "cancel_compile", json!({ "project": harness.project })),
        json!({ "cancelled": false })
    );
    assert!(!finds(&harness, "lateArrival"));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    assert!(finds(&harness, "lateArrival"));
}
