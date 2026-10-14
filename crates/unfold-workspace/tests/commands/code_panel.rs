//! The code panel (`code_view`): one model per selection, for every target kind (a block, lines
//! with a mark, a type card's field, a file, an evidence file, an app or entrance), widened with
//! `show`, never trusting changed bytes, with its paths confined to the project.

use serde_json::{Value, json};

use crate::common::Harness;

const CREATE_ORDER: &str = "src/routes/orders.ts#createOrder";
const PLACE: &str = "src/routes/orders.ts#createOrder/call:orderService.place";

fn view(harness: &Harness, target: Value) -> Value {
    harness.ok(
        1,
        "code_view",
        json!({ "project": harness.project, "target": target }),
    )
}

/// The shown lines' numbers.
fn numbers(view: &Value) -> Vec<u64> {
    view["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| line["number"].as_u64().expect("a number"))
        .collect()
}

/// The shown lines' text, joined.
fn text(view: &Value) -> String {
    view["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| line["text"].as_str().expect("text"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_block_shows_its_lines_marked_with_its_path() {
    let harness = Harness::compiled("ts-orders");
    let definition = view(&harness, json!({ "kind": "block", "key": CREATE_ORDER }));
    assert_eq!(definition["file"], "src/routes/orders.ts");
    assert_eq!(definition["state"], "current");
    assert!(
        text(&definition).starts_with("export async function createOrder("),
        "{definition}"
    );
    let labels: Vec<&Value> = definition["path"]
        .as_array()
        .expect("path")
        .iter()
        .map(|step| &step["label"])
        .collect();
    assert_eq!(labels, ["src/routes/orders.ts", "createOrder"]);
    assert!(
        definition["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .any(|line| line["tokens"].as_array().is_some_and(|tokens| !tokens.is_empty())),
        "highlighted"
    );
    assert_eq!(definition["definitions"][0]["key"], CREATE_ORDER);

    // A call shows its statement, its expression marked; `show` widens to its definition with
    // the call still marked.
    let call = view(&harness, json!({ "kind": "block", "key": PLACE }));
    assert_eq!(numbers(&call).len(), 1, "{call}");
    assert!(
        !call["mark"]["expression"]
            .as_array()
            .expect("expression")
            .is_empty()
    );
    let widened = harness.ok(
        1,
        "code_view",
        json!({
            "project": harness.project,
            "target": { "kind": "block", "key": PLACE },
            "show": { "kind": "block", "key": CREATE_ORDER },
        }),
    );
    assert_eq!(numbers(&widened), numbers(&definition));
    let marked = |view: &Value| -> Vec<Value> {
        view["mark"]["expression"]
            .as_array()
            .expect("expression")
            .iter()
            .map(|range| range["line"].clone())
            .collect()
    };
    assert_eq!(marked(&widened), marked(&call), "the call stays marked");
    let showing: Vec<&Value> = widened["path"]
        .as_array()
        .expect("path")
        .iter()
        .filter(|step| step["showing"] == true)
        .map(|step| &step["key"])
        .collect();
    assert_eq!(showing, [CREATE_ORDER]);

    // A block no graph has is not found.
    let gone = view(
        &harness,
        json!({ "kind": "block", "key": "src/routes/orders.ts#gone" }),
    );
    assert_eq!(gone["state"], "notFound");
    assert_eq!(gone["lines"], json!([]));
}

#[test]
fn lines_fields_and_files_are_targets_too() {
    let harness = Harness::compiled("ts-orders");
    let lines = view(
        &harness,
        json!({ "kind": "lines", "path": "src/routes/orders.ts", "lines": [32, 36], "mark": [33, 34], "title": "Queue it" }),
    );
    assert_eq!(numbers(&lines), [32, 33, 34, 35, 36]);
    assert_eq!(lines["mark"]["lines"], json!([33, 34]));
    assert_eq!(
        lines["path"].as_array().expect("path").last().expect("a step")["label"],
        "Queue it"
    );

    let field = view(
        &harness,
        json!({ "kind": "field", "key": "src/types.ts#Order", "index": 0 }),
    );
    assert_eq!(field["file"], "src/types.ts");
    assert_eq!(field["state"], "current");
    assert!(text(&field).contains("id: string"), "{field}");

    let file = view(&harness, json!({ "kind": "file", "path": "src/server.ts" }));
    assert_eq!(file["state"], "current");
    assert_eq!(numbers(&file)[0], 1);
    let outside = view(&harness, json!({ "kind": "file", "path": "package.json" }));
    assert_eq!(outside["state"], "notAnalyzed");
    assert!(text(&outside).contains("\"name\""), "{outside}");

    // New bytes are never shown with the old graph's ranges: the whole file, nothing marked.
    let path = harness.root.join("src/routes/orders.ts");
    let source = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, format!("// edited\n{source}")).expect("write");
    let changed = view(&harness, json!({ "kind": "block", "key": CREATE_ORDER }));
    assert_eq!(changed["state"], "changed");
    assert_eq!(numbers(&changed)[0], 1);
    assert!(changed.get("mark").is_none(), "{changed}");
    assert_eq!(changed["definitions"], json!([]));

    // Paths stay inside the project; lines are one-based and in order.
    for bad in [
        json!({ "kind": "file", "path": "../outside.ts" }),
        json!({ "kind": "file", "path": "/etc/hosts" }),
        json!({ "kind": "lines", "path": "../x.ts", "lines": [1, 2] }),
    ] {
        assert_eq!(
            harness.code(
                1,
                "code_view",
                json!({ "project": harness.project, "target": bad })
            ),
            "invalid_path",
            "{bad}"
        );
    }
    for bad in [
        json!({ "kind": "file", "path": "" }),
        json!({ "kind": "lines", "path": "src/server.ts", "lines": [0, 2] }),
        json!({ "kind": "lines", "path": "src/server.ts", "lines": [3, 2] }),
        json!({ "kind": "block", "key": "" }),
        json!({ "kind": "range", "path": "src/server.ts" }),
    ] {
        assert_eq!(
            harness.code(
                1,
                "code_view",
                json!({ "project": harness.project, "target": bad })
            ),
            "invalid_args",
            "{bad}"
        );
    }
}

#[test]
fn evidence_files_show_as_plain_text_with_line_numbers() {
    let harness = Harness::new("ts-orders", false);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    harness.ok(
        1,
        "update_project_config",
        json!({ "project": harness.project, "edits": [{ "op": "add_evidence_pattern", "pattern": "db/*.sql" }] }),
    );
    let schema = "create table orders (\n  id text primary key,\n  customer_id text\n);\n";
    harness.write("db/schema.sql", schema);
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let whole = view(&harness, json!({ "kind": "file", "path": "db/schema.sql" }));
    assert_eq!(whole["state"], "current");
    assert_eq!(numbers(&whole), [1, 2, 3, 4]);
    assert!(
        whole["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .all(|line| line["tokens"] == json!([])),
        "plain text"
    );
    let cited = view(
        &harness,
        json!({ "kind": "lines", "path": "db/schema.sql", "lines": [1, 4], "mark": [2, 3] }),
    );
    assert_eq!(cited["mark"]["lines"], json!([2, 3]));
    // Changed bytes aren't what the graph recorded until the next update hashes them.
    harness.write("db/schema.sql", "create table customers (id text);\n");
    assert_eq!(
        view(&harness, json!({ "kind": "file", "path": "db/schema.sql" }))["state"],
        "changed"
    );
}

#[test]
fn apps_and_entrances_show_their_lines_in_unfold_toml() {
    let harness = Harness::compiled("ts-orders");
    let entrance = view(
        &harness,
        json!({ "kind": "start", "key": "entrance:server/place-an-order" }),
    );
    assert_eq!(entrance["file"], "unfold.toml");
    assert_eq!(entrance["state"], "current");
    assert!(
        text(&entrance).contains("name = \"Place an order\""),
        "{entrance}"
    );
    let labels: Vec<&Value> = entrance["path"]
        .as_array()
        .expect("path")
        .iter()
        .map(|step| &step["label"])
        .collect();
    assert_eq!(labels, ["unfold.toml", "Server", "Place an order"]);
    let app = view(&harness, json!({ "kind": "start", "key": "entrance:server" }));
    assert!(text(&app).starts_with("[[app]]"), "{app}");
    assert!(numbers(&app).len() > numbers(&entrance).len());
    let unknown = view(&harness, json!({ "kind": "start", "key": "entrance:nope" }));
    assert_eq!(unknown["state"], "notFound");
}
