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
    // Files only, fuzzy on the path.
    let files = harness.ok(1, "search", json!({ "query": "ordsvc", "kinds": ["files"] }));
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
    // Every kind of type declaration, with its kind.
    let cart = found(json!({ "query": "Cart", "kinds": ["definitions"] }));
    for expected in [
        pair("ts/src/cart.ts#interface:Cart", "interface"),
        pair("ts/src/cart.ts#type_alias:CartId", "type_alias"),
        pair("py/src/shop/models.py#class:Cart", "class"),
    ] {
        assert!(cart.contains(&expected), "{expected:?} in {cart:?}");
    }
    assert!(found(json!({ "query": "Currency" })).contains(&pair("ts/src/cart.ts#enum:Currency", "enum")));
    // Narrowed to types: interfaces, type aliases, enums, and classes only.
    let types = found(json!({
        "query": "order",
        "definitionKinds": ["interface", "type_alias", "enum", "class"],
    }));
    assert!(types.contains(&pair("ts/src/orders/model.ts#interface:OrderDraft", "interface")));
    assert!(types.contains(&pair("py/src/shop/accounts.py#class:OrderDraft", "class")));
    assert!(
        types
            .iter()
            .all(|(_, kind)| ["interface", "type_alias", "enum", "class"].contains(&kind.as_str())),
        "{types:?}"
    );
    let functions = found(json!({ "query": "order", "definitionKinds": ["function"] }));
    assert!(!functions.is_empty());
    assert!(
        functions.iter().all(|(_, kind)| kind == "function"),
        "{functions:?}"
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
    assert_eq!(total(json!(["interface"])), 1);
    assert_eq!(total(json!(["interface", "class"])), 2);
    assert_eq!(total(json!(["enum"])), 0);
    assert_eq!(
        harness.code(
            1,
            "search",
            json!({ "query": "Cart", "definitionKinds": ["struct"] })
        ),
        "invalid_args"
    );
}
