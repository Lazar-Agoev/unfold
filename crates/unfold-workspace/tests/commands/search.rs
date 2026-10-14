//! Search: one fan-out across the projects, merged and ranked, the projects to look in, totals,
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

    // One project only; the totals add up across projects.
    let only = |project: &str| {
        let results = harness.ok(1, "search", json!({ "query": "order", "projects": [project] }));
        assert!(
            hits(&results, "definitions")
                .iter()
                .all(|hit| hit["project"] == project)
        );
        results["definitions"]["total"].as_u64().expect("total")
    };
    let sum = only(&harness.project) + only(&python);
    assert_eq!(results["definitions"]["total"].as_u64(), Some(sum));
    assert_eq!(
        results["definitions"]["truncated"],
        definitions.len() < sum as usize
    );
}

#[test]
fn kinds_and_limits_apply_across_projects() {
    let (harness, _) = two_projects();
    // Files only, by name or path.
    let files = harness.ok(1, "search", json!({ "query": "order-serv", "kinds": ["files"] }));
    assert_eq!(hits(&files, "definitions"), Vec::<Value>::new());
    let first = &hits(&files, "files")[0];
    assert_eq!(first["path"], "src/services/order-service.ts");
    assert_eq!(first["project"], harness.project);

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
    assert_eq!(empty["definitions"]["total"], 0);
}

#[test]
fn entrances_name_their_app_and_files_count_their_definitions() {
    let harness = Harness::compiled("ts-orders");
    let results = harness.ok(
        1,
        "search",
        json!({ "query": "orders", "kinds": ["entrances"], "limit": 500 }),
    );
    let hit = |key: &str| {
        hits(&results, "entrances")
            .into_iter()
            .find(|hit| hit["key"] == key)
            .unwrap_or_else(|| panic!("no hit {key}: {results}"))
    };
    // An entrance names its app; an app names none.
    let list = hit("entrance:server/list-orders");
    assert_eq!(list["kind"], "entrance");
    assert_eq!(list["name"], "List orders");
    assert_eq!(list["app"], "Server");
    assert_eq!(list["path"], "");
    assert_eq!(hit("entrance:worker/process-an-order")["app"], "Worker");
    let app = harness.ok(
        1,
        "search",
        json!({ "query": "scheduler", "kinds": ["entrances"] }),
    );
    let scheduler = hits(&app, "entrances")
        .into_iter()
        .find(|hit| hit["key"] == "entrance:scheduler")
        .unwrap_or_else(|| panic!("the app: {app}"));
    assert_eq!(scheduler["kind"], "app");
    assert_eq!(scheduler["app"], Value::Null);

    // Files are the analyzed ones, each with its module's key and its definitions counted.
    let files = harness.ok(
        1,
        "search",
        json!({ "query": "o", "kinds": ["files"], "limit": 500 }),
    );
    let file = |path: &str| {
        hits(&files, "files")
            .into_iter()
            .find(|hit| hit["path"] == path)
            .unwrap_or_else(|| panic!("no hit {path}: {files}"))
    };
    let orders = file("src/routes/orders.ts");
    assert_eq!(orders["key"], "src/routes/orders.ts#");
    assert_eq!(orders["name"], "orders.ts");
    assert!(
        orders["definitions"].as_u64().is_some_and(|count| count >= 5),
        "{orders}"
    );
    assert!(
        hits(&files, "files")
            .iter()
            .all(|hit| hit["path"] != "tsconfig.json"),
        "only analyzed files"
    );
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

#[test]
fn types_are_definitions_and_narrow_by_kind() {
    let harness = Harness::compiled_fixture("typed-lines");
    let found = |args: Value| -> Vec<(String, String)> {
        hits(&harness.ok(1, "search", args), "definitions")
            .iter()
            .map(|hit| {
                (
                    hit["key"].as_str().expect("key").to_owned(),
                    hit["kind"].as_str().expect("kind").to_owned(),
                )
            })
            .collect()
    };
    let pair = |key: &str, kind: &str| (key.to_owned(), kind.to_owned());
    // Interfaces, type aliases, and enums are types; Python classes are classes.
    let cart = found(json!({ "query": "Cart", "kinds": ["definitions"] }));
    for expected in [
        pair("ts/src/cart.ts#Cart", "type"),
        pair("ts/src/cart.ts#CartId", "type"),
        pair("py/src/shop/models.py#Cart", "class"),
    ] {
        assert!(cart.contains(&expected), "{expected:?} in {cart:?}");
    }
    assert!(found(json!({ "query": "Currency" })).contains(&pair("ts/src/cart.ts#Currency", "type")));
    // Narrowed to classes and types.
    let types = found(json!({ "query": "order", "definitionKinds": ["type", "class"] }));
    assert!(types.contains(&pair("ts/src/orders/model.ts#OrderDraft", "type")));
    assert!(types.contains(&pair("py/src/shop/accounts.py#OrderDraft", "class")));
    assert!(
        types
            .iter()
            .all(|(_, kind)| ["type", "class"].contains(&kind.as_str())),
        "{types:?}"
    );
    let functions = found(json!({ "query": "order", "definitionKinds": ["function"] }));
    assert!(!functions.is_empty());
    assert!(
        functions.iter().all(|(_, kind)| kind == "function"),
        "{functions:?}"
    );
    // Modules only when asked: files find them by path.
    assert!(
        found(json!({ "query": "cart" }))
            .iter()
            .all(|(_, kind)| kind != "module")
    );
    assert!(
        found(json!({ "query": "cart", "definitionKinds": ["module"] }))
            .contains(&pair("ts/src/cart.ts#", "module"))
    );
    // The totals count what the kinds let through.
    let total = |kinds: Value| {
        harness.ok(
            1,
            "search",
            json!({ "query": "OrderDraft", "definitionKinds": kinds }),
        )["definitions"]["total"]
            .clone()
    };
    assert_eq!(total(json!(["type"])), 1);
    assert_eq!(total(json!(["type", "class"])), 2);
    assert_eq!(total(json!(["function"])), 0);
    assert_eq!(
        harness.code(
            1,
            "search",
            json!({ "query": "Cart", "definitionKinds": ["interface"] })
        ),
        "invalid_args"
    );
}
