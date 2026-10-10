//! Search: one fan-out across the projects, merged and ranked, "Look in", per-project totals,
//! resting projects, and latency.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::common::Harness;

/// `ts-orders` and `py-orders`, both compiled and selected (so both stay live).
fn two_projects() -> (Harness, String) {
    let harness = Harness::compiled("ts-orders");
    let (python, _) = harness.add("py-orders", "py-orders");
    harness.ok(2, "select_project", json!({ "project": python }));
    harness.ok(2, "compile", json!({ "project": python }));
    (harness, python)
}

fn hits(results: &Value, kind: &str) -> Vec<Value> {
    results[kind]["hits"].as_array().cloned().expect("hits")
}

#[test]
fn search_fans_out_and_merges_by_score_then_workspace_order() {
    let (harness, python) = two_projects();
    let results = harness.ok(1, "search", json!({ "query": "order" }));
    let definitions = hits(&results, "definitions");
    let projects: Vec<&str> = definitions
        .iter()
        .filter_map(|hit| hit["project"].as_str())
        .collect();
    assert!(projects.contains(&harness.project.as_str()), "{results}");
    assert!(projects.contains(&python.as_str()), "{results}");
    let position = |project: &str| usize::from(project == python);
    for pair in definitions.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (a_score, b_score) = (
            a["score"].as_u64().expect("score"),
            b["score"].as_u64().expect("score"),
        );
        assert!(a_score >= b_score, "by score");
        if a_score == b_score {
            let (a_at, b_at) = (
                position(a["project"].as_str().expect("project")),
                position(b["project"].as_str().expect("project")),
            );
            assert!(a_at <= b_at, "then workspace order");
            if a_at == b_at {
                let (a_name, b_name) = (
                    a["name"].as_str().expect("name"),
                    b["name"].as_str().expect("name"),
                );
                assert!(a_name.len() <= b_name.len(), "then shorter name");
            }
        }
    }
    assert!(definitions.iter().all(|hit| hit["line"].is_u64()), "{results}");
    let totals = results["projects"].as_array().expect("projects");
    assert_eq!(totals.len(), 2);
    assert_eq!(totals[0]["project"], harness.project);
    assert_eq!(totals[1]["project"], python);
    let sum: u64 = totals
        .iter()
        .map(|total| total["definitions"].as_u64().expect("count"))
        .sum();
    assert_eq!(results["definitions"]["total"].as_u64(), Some(sum));
    assert_eq!(
        results["definitions"]["truncated"],
        definitions.len() < sum as usize
    );

    // "Look in" one project.
    let python_only = harness.ok(1, "search", json!({ "query": "order", "projects": [python] }));
    assert!(
        hits(&python_only, "definitions")
            .iter()
            .all(|hit| hit["project"] == python)
    );
    assert_eq!(python_only["projects"].as_array().map(Vec::len), Some(1));
}

#[test]
fn kinds_and_limits_apply_across_projects() {
    let (harness, _) = two_projects();
    // Files only, fuzzy on the path.
    let files = harness.ok(1, "search", json!({ "query": "ordsvc", "kinds": ["files"] }));
    assert_eq!(hits(&files, "definitions"), Vec::<Value>::new());
    let first = &hits(&files, "files")[0];
    assert_eq!(first["path"], "src/services/order-service.ts");
    assert_eq!(first["project"], harness.project);
    assert!(
        first["matches"]
            .as_array()
            .is_some_and(|matches| !matches.is_empty())
    );

    // Definitions only, limited across every project.
    let symbols = harness.ok(
        1,
        "search",
        json!({ "query": "order", "kinds": ["definitions"], "limit": 3 }),
    );
    assert_eq!(hits(&symbols, "definitions").len(), 3);
    assert_eq!(symbols["definitions"]["truncated"], true);
    assert_eq!(hits(&symbols, "files"), Vec::<Value>::new());

    let empty = harness.ok(1, "search", json!({ "query": "   " }));
    assert_eq!(hits(&empty, "files"), Vec::<Value>::new());
    assert_eq!(empty["projects"], json!([]));
}

#[test]
fn entrances_name_their_application_and_files_count_their_definitions() {
    let harness = Harness::compiled("ts-orders");
    let results = harness.ok(
        1,
        "search",
        json!({ "query": "orders", "kinds": ["entrances"], "limit": 500 }),
    );
    let application = |key: &str| {
        hits(&results, "entrances")
            .into_iter()
            .find(|hit| hit["key"] == key)
            .map(|hit| hit["application"].clone())
            .unwrap_or_else(|| panic!("no hit {key}: {results}"))
    };
    // A member names its first application; a launch, the application it starts.
    assert_eq!(
        application(r#"entrance:["express.route","http","GET","/orders","listOrders"]"#),
        "server"
    );
    assert_eq!(
        application(r#"entrance:["bullmq.worker","worker","","orders","processOrder"]"#),
        "worker"
    );
    let launch = harness.ok(
        1,
        "search",
        json!({ "query": "scheduler", "kinds": ["entrances"] }),
    );
    assert!(
        hits(&launch, "entrances")
            .iter()
            .any(|hit| hit["kind"] == "process" && hit["application"] == "scheduler"),
        "{launch}"
    );

    // An indexed file counts its definitions; any other file has no count.
    let files = harness.ok(
        1,
        "search",
        json!({ "query": "o", "kinds": ["files"], "limit": 500 }),
    );
    let definitions = |path: &str| {
        hits(&files, "files")
            .into_iter()
            .find(|hit| hit["path"] == path)
            .map(|hit| hit["definitions"].clone())
            .unwrap_or_else(|| panic!("no hit {path}: {files}"))
    };
    assert_eq!(definitions("src/routes/orders.ts"), 4);
    assert_eq!(
        definitions("src/queues/orders-worker.ts"),
        0,
        "indexed, without definitions"
    );
    assert_eq!(definitions("tsconfig.json"), Value::Null);
}

#[test]
fn search_is_fast() {
    let harness = Harness::compiled("ts-orders");
    let queries = [
        "place",
        "order",
        "o",
        "req",
        "worker",
        "ordsvc",
        "createOrder",
        "src/routes",
        "zz",
    ];
    // Warm the file list and the statement cache once, as the first keystroke would.
    harness.ok(1, "search", json!({ "query": "warm" }));
    let mut timings: Vec<Duration> = Vec::new();
    for _ in 0..10 {
        for query in queries {
            let started = Instant::now();
            harness.ok(1, "search", json!({ "query": query }));
            timings.push(started.elapsed());
        }
    }
    timings.sort_unstable();
    let p95 = timings[timings.len() * 95 / 100];
    assert!(p95 < Duration::from_millis(100), "search p95 {p95:?}");
}
