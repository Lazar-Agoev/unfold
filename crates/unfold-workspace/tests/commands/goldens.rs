//! Golden JSON for representative commands on `ts-orders`, as the current front end and linker
//! produce them. Re-bless with `UNFOLD_BLESS=1` when the engine's output changes on purpose.

use serde_json::{Value, json};
use unfold_core::testkit::assert_golden;

use crate::common::Harness;

/// Replaces the temporary folder with `<folder>`, the random project id with `<project>`, and
/// zeroes timings, so goldens are stable.
fn normalize(value: &mut Value, root: &str, project: &str) {
    match value {
        Value::String(text) if text == project => *text = "<project>".to_owned(),
        Value::String(text) if text.contains(root) => *text = text.replace(root, "<folder>"),
        Value::Array(items) => items.iter_mut().for_each(|item| normalize(item, root, project)),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if matches!(key.as_str(), "tookMs" | "durationMs" | "atMs" | "updatedAtMs") && !item.is_null()
                {
                    *item = json!(0);
                } else {
                    normalize(item, root, project);
                }
            }
        }
        _ => {}
    }
}

/// Runs `command` on the harness's project (search: across the workspace) and compares its
/// normalized result with the golden file `name`.
fn golden(harness: &Harness, name: &str, command: &str, mut args: Value) -> Value {
    if command != "search" {
        args["project"] = json!(harness.project);
    }
    let mut value = harness.ok(1, command, args);
    let result = value.clone();
    normalize(
        &mut value,
        harness.root.to_str().expect("utf-8"),
        &harness.project,
    );
    let text = serde_json::to_string_pretty(&value).expect("json") + "\n";
    assert_golden(format!("workspace/commands/{name}.json"), &text);
    result
}

#[test]
fn representative_commands_match_their_goldens() {
    let harness = Harness::compiled("ts-orders");
    golden(&harness, "project_summary", "project_summary", json!({}));
    golden(
        &harness,
        "file_view_orders_page",
        "file_view",
        json!({ "path": "src/routes/orders.ts", "range": { "startLine": 1, "endLine": 12 } }),
    );
    let map = golden(&harness, "entrances", "entrances", json!({}));
    let create_order = json!({ "key": "src/routes/orders.ts#function:createOrder" });
    golden(&harness, "card_create_order", "card", create_order.clone());
    golden(&harness, "peek_create_order", "peek", create_order.clone());
    golden(
        &harness,
        "relations_create_order",
        "relations",
        json!({ "key": "src/routes/orders.ts#function:createOrder", "onPage": [] }),
    );
    golden(&harness, "callers_create_order", "callers", create_order.clone());
    golden(&harness, "preview_create_order", "preview", create_order);
    let application = &map["categories"][0]["applications"][0];
    golden(
        &harness,
        "entrances_first_application",
        "entrances",
        json!({ "application": application["id"], "limit": 4 }),
    );
    let entrance = harness
        .entrance_rows(None)
        .into_iter()
        .find(|row| row["id"] == "import")
        .expect("ts-orders configures the import command");
    golden(
        &harness,
        "card_first_entrance",
        "card",
        json!({ "key": entrance["key"] }),
    );
    golden(
        &harness,
        "preview_first_entrance",
        "preview",
        json!({ "key": entrance["key"] }),
    );
    golden(&harness, "search_place", "search", json!({ "query": "place" }));
    golden(
        &harness,
        "search_worker",
        "search",
        json!({ "query": "worker", "kinds": ["entrances", "files"], "limit": 4 }),
    );
    golden(&harness, "problems", "problems", json!({}));

    // Boards: a teammate's board, listed and then opened.
    harness.write(
        ".unfold/boards/order-placement.json",
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/boards/order-placement.json"),
        )
        .expect("the sample board"),
    );
    golden(&harness, "list_boards", "list_boards", json!({}));
    golden(
        &harness,
        "open_board_order_placement",
        "open_board",
        json!({ "id": ".unfold/boards/order-placement.json" }),
    );
}
