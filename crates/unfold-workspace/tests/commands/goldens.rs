//! Golden JSON for representative commands on `ts-orders`, as the current front end and linker
//! produce them. Re-bless with `UNFOLD_BLESS=1` when the engine's output changes on purpose.

use serde_json::{Value, json};
use unfold_core::testkit::assert_golden;

use crate::common::Harness;

/// Replaces the temporary folder with `<folder>` and zeroes timings, so goldens are stable.
fn normalize(value: &mut Value, root: &str) {
    match value {
        Value::String(text) if text.contains(root) => *text = text.replace(root, "<folder>"),
        Value::Array(items) => items.iter_mut().for_each(|item| normalize(item, root)),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if matches!(
                    key.as_str(),
                    "tookMs" | "durationMs" | "atMs" | "updateMs" | "commitMs"
                ) {
                    *item = json!(0);
                } else {
                    normalize(item, root);
                }
            }
        }
        _ => {}
    }
}

fn golden(harness: &Harness, name: &str, command: &str, args: Value) -> Value {
    let mut value = harness.ok(1, command, args);
    let result = value.clone();
    normalize(&mut value, harness.root.to_str().expect("utf-8"));
    let text = serde_json::to_string_pretty(&value).expect("json") + "\n";
    assert_golden(format!("workspace/commands/{name}.json"), &text);
    result
}

#[test]
fn representative_commands_match_their_goldens() {
    let harness = Harness::compiled("ts-orders");
    golden(&harness, "folder_summary", "folder_summary", json!({}));
    golden(&harness, "list_dir_root", "list_dir", json!({ "path": "" }));
    golden(
        &harness,
        "list_dir_routes",
        "list_dir",
        json!({ "path": "src/routes" }),
    );
    let outline = golden(
        &harness,
        "file_outline_orders",
        "file_outline",
        json!({ "path": "src/routes/orders.ts" }),
    );
    assert!(
        outline["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["name"] == "createOrder")),
        "{outline}"
    );
    golden(
        &harness,
        "file_view_orders_page",
        "file_view",
        json!({ "path": "src/routes/orders.ts", "range": { "startLine": 1, "endLine": 12 } }),
    );
    let tree = golden(&harness, "entrance_tree", "entrance_tree", json!({}));
    golden(
        &harness,
        "card_create_order",
        "card",
        json!({ "key": "src/routes/orders.ts#function:createOrder" }),
    );
    golden(
        &harness,
        "peek_create_order",
        "peek",
        json!({ "key": "src/routes/orders.ts#function:createOrder" }),
    );
    golden(
        &harness,
        "relations_create_order",
        "relations",
        json!({ "key": "src/routes/orders.ts#function:createOrder", "onBoard": [] }),
    );
    let entrance = tree["applications"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|application| application["entrances"].as_array().cloned().unwrap_or_default())
        .chain(tree["unreached"].as_array().cloned().unwrap_or_default())
        .next();
    if let Some(entrance) = entrance {
        golden(
            &harness,
            "card_first_entrance",
            "card",
            json!({ "key": entrance["key"] }),
        );
    }
    golden(&harness, "search_place", "search", json!({ "query": "place" }));
    golden(&harness, "problems", "problems", json!({}));
}
