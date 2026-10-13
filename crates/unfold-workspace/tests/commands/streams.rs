//! Streamed commands: compile progress and text-search matches (across the projects) arrive as
//! stream messages tagged with their window and request, the result resolves the request, and
//! `cancel` stops them.

use std::sync::mpsc;

use serde_json::{Value, json};
use unfold_workspace::CommandError;

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

/// Adds `count` TypeScript files that each mention `needle` on several lines.
fn add_files(harness: &Harness, count: usize, needle: &str) {
    for index in 0..count {
        let mut text = String::new();
        for line in 0..20 {
            text.push_str(&format!(
                "export function f{index}_{line}(value: number): number {{ return value + {line}; }} // {needle}\n"
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
}

#[test]
fn cancelling_a_compile_rejects_it_with_cancelled() {
    let harness = Harness::new("ts-orders", false);
    add_files(&harness, 400, "padding");
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

#[test]
fn text_search_streams_utf16_matches_in_project_then_path_order() {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(2, "select_project", json!({ "project": python }));
    harness.write("src/unicode.ts", "// é orderService ünd orderService\n");
    std::fs::write(
        harness.base.join("py-orders/notes.txt"),
        "orderService in python too\n",
    )
    .expect("write");
    let (request, result) = start(
        &harness,
        1,
        json!({ "command": "search_text", "args": { "query": "orderService", "regex": false, "caseSensitive": true } }),
    );
    let summary: Value =
        serde_json::from_str(&result.recv_timeout(PATIENCE).expect("done").expect("search")).expect("json");
    let files = stream_messages(&harness, request);
    assert_eq!(summary["files"].as_u64(), Some(files.len() as u64));
    assert_eq!(summary["truncated"], false);
    let order: Vec<(usize, &str)> = files
        .iter()
        .map(|message| {
            (
                usize::from(message["message"]["project"] == python),
                message["message"]["path"].as_str().expect("path"),
            )
        })
        .collect();
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(order, sorted, "projects in workspace order, files in path order");
    assert!(
        order
            .iter()
            .any(|(project, path)| *project == 1 && *path == "notes.txt")
    );
    let paths: Vec<&str> = order.iter().map(|(_, path)| *path).collect();
    let projects = summary["projects"].as_array().expect("projects");
    assert_eq!(projects[0]["project"], harness.project);
    assert_eq!(projects[1]["project"], python);
    assert_eq!(projects[1]["files"], 1);
    assert!(paths.iter().all(|path| !path.starts_with(".unfold/")));
    let unicode = files
        .iter()
        .find(|message| message["message"]["path"] == "src/unicode.ts")
        .expect("the unicode file matched");
    let line = &unicode["message"]["matches"][0];
    assert_eq!(line["line"], 1);
    // "// é " is five UTF-16 units; "orderService" is twelve.
    assert_eq!(line["ranges"][0], json!([5, 17]));
    assert_eq!(line["ranges"][1], json!([22, 34]));

    let regex = harness.ok(
        1,
        "search_text",
        json!({ "query": "order(Service|Repo)", "regex": true, "projects": [harness.project] }),
    );
    assert!(regex["matches"].as_u64().is_some_and(|matches| matches > 0));
    assert_eq!(regex["projects"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        harness.code(
            1,
            "search_text",
            json!({ "query": "(", "regex": true, "caseSensitive": false })
        ),
        "invalid_args"
    );
}

#[test]
fn text_search_stops_at_its_limit_and_when_cancelled() {
    let harness = Harness::new("ts-orders", false);
    add_files(&harness, 2_000, "needle");
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    let summary = harness.ok(
        1,
        "search_text",
        json!({ "query": "needle", "regex": false, "caseSensitive": false }),
    );
    assert_eq!(summary["matches"], 2_000);
    assert_eq!(summary["truncated"], true);

    // A search that has to read every one of the 2,000 files is cancelled long before the end.
    let (request, result) = start(
        &harness,
        1,
        json!({ "command": "search_text", "args": { "query": "zzz-not-there", "regex": false, "caseSensitive": false } }),
    );
    harness.host.cancel(request);
    match result.recv_timeout(PATIENCE).expect("done") {
        Err(error) => assert_eq!(error.code, "cancelled"),
        Ok(summary) => panic!("the search finished before the cancel arrived: {summary}"),
    }
}
