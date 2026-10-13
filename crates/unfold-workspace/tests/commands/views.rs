//! The peek sheet (`file_view`) and cards (`card`, `peek`, `relations`, `callers`), each naming
//! its project: paging, freshness, and shapes.

use serde_json::{Value, json};

use crate::common::Harness;

/// A TypeScript file of `functions` small functions that call each other.
fn generated(functions: usize) -> String {
    let mut text = String::new();
    for index in 0..functions {
        let next = (index + 1) % functions;
        text.push_str(&format!(
            "export function step{index}(value: number): number {{\n  if (value > {index}) {{\n    return step{next}(value - 1);\n  }}\n  // A comment with é and 😀.\n  return value + {index};\n}}\n\n"
        ));
    }
    text
}

fn page(harness: &Harness, path: &str, start: u32, end: u32) -> Value {
    harness.ok(
        1,
        "file_view",
        json!({ "project": harness.project, "path": path, "range": { "startLine": start, "endLine": end } }),
    )
}

#[test]
fn pages_are_slices_of_the_whole_file() {
    let harness = Harness::compiled("ts-orders");
    harness.write("src/generated.ts", &generated(40));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let path = "src/generated.ts";
    let whole = harness.ok(
        1,
        "file_view",
        json!({ "project": harness.project, "path": path }),
    );
    assert_eq!(whole["graph"], "current");
    let line_count = whole["lineCount"].as_u64().expect("line count") as u32;
    let text: Vec<u16> = whole["text"].as_str().expect("text").encode_utf16().collect();
    for (start, end) in [(1, 1), (5, 37), (100, 180), (line_count - 3, line_count + 50)] {
        let page = page(&harness, path, start, end);
        let from = page["textStart"].as_u64().expect("start") as usize;
        let shown: Vec<u16> = page["text"].as_str().expect("text").encode_utf16().collect();
        assert_eq!(&text[from..from + shown.len()], &shown[..], "lines {start}-{end}");
        let last = end.min(line_count);
        let within = |key: &str, line: &dyn Fn(&Value) -> bool| -> Vec<Value> {
            whole[key]
                .as_array()
                .expect(key)
                .iter()
                .filter(|item| line(item))
                .cloned()
                .collect()
        };
        let lines = within("lines", &|line| {
            let number = line["number"].as_u64().unwrap_or(0) as u32;
            number >= start && number <= last
        });
        assert_eq!(page["lines"], Value::Array(lines));
        let links = within("links", &|link| {
            let number = link["line"].as_u64().unwrap_or(0) as u32;
            number >= start && number <= last
        });
        assert_eq!(page["links"], Value::Array(links));
        let page_end = (from + shown.len()) as u64;
        let spans = within("spans", &|span| {
            span[0].as_u64().unwrap_or(0) < page_end && span[1].as_u64().unwrap_or(0) > from as u64
        });
        assert_eq!(page["spans"], Value::Array(spans));
        assert_eq!(page["startLine"], start);
        assert_eq!(page["endLine"], last);
    }

    // New bytes are never shown with the old graph's ranges.
    harness.write(path, &format!("// edited\n{}", generated(40)));
    let changed = page(&harness, path, 1, 20);
    assert_eq!(changed["graph"], "not_compiled");
    assert_eq!(changed["links"], json!([]));
    assert!(changed["text"].as_str().expect("text").starts_with("// edited"));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let compiled = page(&harness, path, 1, 20);
    assert_eq!(compiled["graph"], "current");
    assert!(!compiled["links"].as_array().expect("links").is_empty());
}

#[test]
fn evidence_files_show_as_text_with_line_numbers() {
    let harness = Harness::new("ts-orders", false);
    harness.ok(1, "select_project", json!({ "project": harness.project }));
    let config = harness.ok(
        1,
        "update_project_config",
        json!({ "project": harness.project, "edits": [{ "op": "add_evidence_pattern", "pattern": "db/*.sql" }] }),
    );
    assert_eq!(config["settings"]["evidence"], json!(["db/*.sql"]));
    assert!(
        config["text"]
            .as_str()
            .is_some_and(|text| text.ends_with("[evidence]\nfiles = [\"db/*.sql\"]\n")),
        "{config}"
    );
    let read = harness.ok(1, "project_config", json!({ "project": harness.project }));
    assert_eq!(read["settings"]["evidence"], json!(["db/*.sql"]));
    let schema = "create table orders (\n  id text primary key,\n  customer_id text\n);\n";
    harness.write("db/schema.sql", schema);
    harness.ok(1, "compile", json!({ "project": harness.project }));

    let view = |range: Value| {
        let mut args = json!({ "project": harness.project, "path": "db/schema.sql" });
        if !range.is_null() {
            args["range"] = range;
        }
        harness.ok(1, "file_view", args)
    };
    let whole = view(Value::Null);
    assert_eq!(whole["graph"], "evidence");
    assert_eq!(whole["fileState"], Value::Null);
    assert_eq!(whole["text"], schema);
    assert_eq!(whole["lineCount"], 4);
    assert_eq!(whole["outline"], json!([]));
    assert_eq!(whole["links"], json!([]));
    let page = view(json!({ "startLine": 2, "endLine": 3 }));
    assert_eq!(page["text"], "  id text primary key,\n  customer_id text\n");
    assert_eq!(page["lines"][0]["number"], 2);

    // Changed bytes aren't what the graph recorded until the next update hashes them.
    harness.write("db/schema.sql", "create table customers (id text);\n");
    assert_eq!(view(Value::Null)["graph"], "not_compiled");
    harness.ok(1, "compile", json!({ "project": harness.project }));
    assert_eq!(view(Value::Null)["graph"], "evidence");
    // A file no pattern lists is just not indexed.
    harness.write("db/seed.txt", "seed\n");
    assert_eq!(
        harness.ok(
            1,
            "file_view",
            json!({ "project": harness.project, "path": "db/seed.txt" })
        )["graph"],
        "not_indexed"
    );
}

#[test]
fn cards_peeks_relations_and_callers_name_their_project() {
    let harness = Harness::compiled("ts-orders");
    harness.write("src/generated.ts", &generated(6));
    harness.ok(1, "compile", json!({ "project": harness.project }));
    let key = "src/generated.ts#function:step1";
    let at = |command: &str, extra: Value| {
        let mut args = json!({ "project": harness.project, "key": key });
        if let (Some(args), Some(extra)) = (args.as_object_mut(), extra.as_object()) {
            args.extend(extra.clone());
        }
        harness.ok(1, command, args)
    };
    let card = at("card", json!({ "lens": "code" }));
    assert_eq!(card["type"], "block");
    assert!(card["flow"].is_null() && card["code"].is_object());
    assert_eq!(
        harness.code(
            1,
            "card",
            json!({ "project": harness.project, "key": key, "board": { "project": harness.project, "id": "x", "page": "p" } })
        ),
        "invalid_args",
        "a card takes no board (labels are the UI's)"
    );
    let peek = at("peek", json!({}));
    assert_eq!(peek["name"], "step1");
    assert!(
        peek["signature"]
            .as_str()
            .is_some_and(|line| line.starts_with("export function step1"))
    );

    let relations = at(
        "relations",
        json!({ "onPage": ["src/generated.ts#function:step2"] }),
    );
    assert_eq!(relations["callersTotal"], 1);
    assert_eq!(
        relations["outgoing"]["onBoard"][0]["target"],
        "src/generated.ts#function:step2"
    );
    let too_many: Vec<String> = (0..2_001).map(|index| format!("k{index}")).collect();
    assert_eq!(
        harness.code(
            1,
            "relations",
            json!({ "project": harness.project, "key": key, "onPage": too_many })
        ),
        "invalid_args"
    );

    let callers = at("callers", json!({}));
    assert_eq!(callers["project"], harness.project);
    assert_eq!(callers["total"], 1);
    assert_eq!(
        callers["callers"][0],
        json!({
            "key": "src/generated.ts#function:step0", "name": "step0", "qualifiedName": "step0",
            "path": "src/generated.ts", "line": 1, "sites": 1,
        })
    );
    assert_eq!(
        harness.code(
            1,
            "callers",
            json!({ "project": harness.project, "key": "src/generated.ts#function:nope" })
        ),
        "not_found"
    );
}
