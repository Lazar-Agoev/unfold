//! Source tabs (`file_view`) and cards (`card`, `relations`): paging, freshness, and latency.

use std::time::{Duration, Instant};

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
        json!({ "path": path, "range": { "startLine": start, "endLine": end } }),
    )
}

#[test]
fn pages_are_slices_of_the_whole_file() {
    let harness = Harness::new("ts-orders", false);
    harness.open(1);
    harness.write("src/generated.ts", &generated(40));
    harness.ok(1, "compile", json!({}));
    let path = "src/generated.ts";
    let whole = harness.ok(1, "file_view", json!({ "path": path }));
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
    harness.ok(1, "compile", json!({}));
    let compiled = page(&harness, path, 1, 20);
    assert_eq!(compiled["graph"], "current");
    assert!(!compiled["links"].as_array().expect("links").is_empty());
}

fn percentile(mut timings: Vec<Duration>, percent: usize) -> Duration {
    timings.sort_unstable();
    timings[(timings.len() * percent / 100).min(timings.len() - 1)]
}

/// Prints page, card, relations, and search latency on a large generated file. Run it with
/// `cargo test -p unfold-workspace --release --test commands -- --ignored --nocapture views`.
#[test]
#[ignore = "a benchmark"]
fn view_latency() {
    let harness = Harness::new("ts-orders", false);
    harness.open(1);
    harness.write("src/generated.ts", &generated(3_000));
    harness.ok(1, "compile", json!({}));
    let path = "src/generated.ts";
    let mut pages = Vec::new();
    for start in (1..20_000).step_by(400) {
        let started = Instant::now();
        page(&harness, path, start, start + 99);
        pages.push(started.elapsed());
    }
    let mut cards = Vec::new();
    let mut relations = Vec::new();
    for index in (0..3_000).step_by(60) {
        let key = format!("{path}#function:step{index}");
        let started = Instant::now();
        harness.ok(1, "card", json!({ "key": key }));
        cards.push(started.elapsed());
        let started = Instant::now();
        harness.ok(1, "relations", json!({ "key": key }));
        relations.push(started.elapsed());
    }
    let mut searches = Vec::new();
    for query in ["step", "step1", "value", "s2", "generated", "stp9"] {
        for _ in 0..10 {
            let started = Instant::now();
            harness.ok(1, "search", json!({ "query": query }));
            searches.push(started.elapsed());
        }
    }
    println!(
        "search p50 {:?} p95 {:?}",
        percentile(searches.clone(), 50),
        percentile(searches, 95)
    );
    println!(
        "file_view page p50 {:?} p95 {:?}; card p50 {:?} p95 {:?}; relations p50 {:?} p95 {:?}",
        percentile(pages.clone(), 50),
        percentile(pages, 95),
        percentile(cards.clone(), 50),
        percentile(cards, 95),
        percentile(relations.clone(), 50),
        percentile(relations, 95),
    );
}
