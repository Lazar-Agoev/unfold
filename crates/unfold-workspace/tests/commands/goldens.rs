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
                if matches!(key.as_str(), "durationMs" | "atMs" | "updatedAtMs") && !item.is_null() {
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
    golden(&harness, "entrances", "entrances", json!({}));
    let create_order = json!({ "key": "src/routes/orders.ts#createOrder" });
    golden(&harness, "card_create_order", "card", create_order.clone());
    golden(
        &harness,
        "card_place_an_order",
        "card",
        json!({ "key": "entrance:server/place-an-order" }),
    );
    golden(
        &harness,
        "code_view_create_order",
        "code_view",
        json!({ "target": { "kind": "block", "key": "src/routes/orders.ts#createOrder" } }),
    );
    golden(
        &harness,
        "code_text_create_order",
        "code_text",
        create_order.clone(),
    );
    golden(
        &harness,
        "flow_text_create_order",
        "flow_text",
        json!({
            "key": "src/routes/orders.ts#createOrder",
            "overlay": { "names": [{ "key": "src/routes/orders.ts#createOrder/if[0]/then", "text": "Asked to queue it?" }] },
            "open": [["src/routes/orders.ts#createOrder/call:orderService.place"]],
        }),
    );
    golden(&harness, "relations_create_order", "relations", create_order);
    golden(
        &harness,
        "callers_place",
        "callers",
        json!({ "key": "src/services/order-service.ts#OrderService.place" }),
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
