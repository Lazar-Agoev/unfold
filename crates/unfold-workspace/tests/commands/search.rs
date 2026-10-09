//! Search: grouped, ranked results, quick open, and latency.

use std::time::{Duration, Instant};

use serde_json::json;

use crate::common::Harness;

#[test]
fn search_groups_and_ranks_definitions_entrances_and_files() {
    let harness = Harness::compiled("ts-orders");
    let results = harness.ok(1, "search", json!({ "query": "place" }));
    let definitions = results["definitions"].as_array().expect("definitions");
    assert!(
        definitions
            .iter()
            .any(|hit| hit["qualifiedName"] == "OrderService.place" && hit["line"].is_u64()),
        "{results}"
    );
    let scores: Vec<u64> = definitions
        .iter()
        .filter_map(|hit| hit["score"].as_u64())
        .collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "ranked best first"
    );

    // ⌘P: files only, fuzzy on the path.
    let files = harness.ok(1, "search", json!({ "query": "ordsvc", "kinds": ["files"] }));
    assert!(files["definitions"].as_array().is_some_and(Vec::is_empty));
    assert_eq!(files["files"][0]["path"], "src/services/order-service.ts");
    assert!(
        files["files"][0]["matches"]
            .as_array()
            .is_some_and(|matches| !matches.is_empty())
    );

    // ⌘T: definitions only, limited.
    let symbols = harness.ok(
        1,
        "search",
        json!({ "query": "order", "kinds": ["definitions"], "limit": 3 }),
    );
    assert!(
        symbols["definitions"]
            .as_array()
            .is_some_and(|hits| hits.len() <= 3)
    );
    assert!(symbols["files"].as_array().is_some_and(Vec::is_empty));

    let empty = harness.ok(1, "search", json!({ "query": "   " }));
    assert!(empty["files"].as_array().is_some_and(Vec::is_empty));
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
