//! Mermaid export against the proof of concept and against snapshots.
//!
//! Every entrance diagram of both playgrounds is compared with the PoC reader's
//! (`codegraph export --entrance <key> --mermaid`, in `testdata/reference/poc-reader/<playground>/
//! mermaid.json`) **modulo the labels decision U-27 changed**. Both texts are reduced to their
//! structure and the structures must be equal line for line:
//!
//! - comment lines (`%% …`, warnings and health notes, whose wording differs) are dropped;
//! - node lines keep their id and shape; their label is kept only where the PoC did not show
//!   source text: nodes outside a card's subgraph (entrances, Code-view cards, review and missing
//!   placeholders) and `End`. Labels of other Flow nodes (the PoC's exact source, Unfold's
//!   authored label or fallback name) and of unresolved-target placeholders (whose detail now
//!   uses the display reason) are replaced by `…`;
//! - edge lines keep their ends, connector, and label, with the PoC's `handler N` read as
//!   `handler` (no step numbers in any view);
//! - subgraph, `direction`, `end`, `class`, and `classDef` lines are kept exactly.
//!
//! Unfold's own diagrams (every entrance and every definition's Flow, per playground) are also
//! snapshots in `testdata/mermaid/`, which `tools/oracles/check-mermaid.mjs` parses with the
//! real Mermaid parser. `UNFOLD_BLESS=1` rewrites the snapshots.

mod common;

use std::collections::BTreeMap;

use common::{load, playground_root, reference};
use serde_json::Value;
use unfold_core::testkit::assert_golden;
use unfold_reader::board::BoardGraph;
use unfold_reader::flow::FlowContext;
use unfold_reader::mermaid;

/// Reduces a diagram to its structure (see the module documentation).
pub fn structure(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut depth = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("%%") {
            continue;
        }
        if trimmed.starts_with("subgraph ") {
            depth += 1;
            lines.push(trimmed.to_owned());
            continue;
        }
        if trimmed == "end" {
            depth = depth.saturating_sub(1);
            lines.push(trimmed.to_owned());
            continue;
        }
        if trimmed.starts_with("direction ")
            || trimmed.starts_with("class")
            || trimmed.starts_with("flowchart")
        {
            lines.push(trimmed.to_owned());
            continue;
        }
        if let Some(edge) = edge(trimmed) {
            lines.push(edge);
            continue;
        }
        lines.push(node(trimmed, depth > 0));
    }
    lines
}

fn edge(line: &str) -> Option<String> {
    let (from, rest) = line.split_once(' ')?;
    let (connector, rest) = if let Some(rest) = rest.strip_prefix("-.->") {
        ("-.->", rest)
    } else {
        ("-->", rest.strip_prefix("-->")?)
    };
    let (label, target) = match rest.strip_prefix('|') {
        Some(labeled) => labeled.split_once("| ")?,
        None => ("", rest),
    };
    let label = if label.starts_with("\"handler") {
        "\"handler\""
    } else {
        label
    };
    Some(format!("{from} {connector}|{label}| {}", target.trim()))
}

fn node(line: &str, in_card: bool) -> String {
    let Some(open) = line.find('"') else {
        return line.to_owned();
    };
    let close = line.rfind('"').unwrap_or(open);
    let (id_and_open, label, closing) = (&line[..open], &line[open + 1..close], &line[close + 1..]);
    let placeholder = id_and_open.contains("_target");
    let keep = !placeholder && (!in_card || label == "End");
    format!("{id_and_open}\"{}\"{closing}", if keep { label } else { "…" })
}

struct Names<'a>(&'a BoardGraph);

impl FlowContext for Names<'_> {
    fn target_name(&self, key: &str) -> Option<String> {
        self.0.qualified_name(key)
    }
}

fn check(playground: &str, corpus: &str) {
    let loaded = load(corpus);
    let graph = BoardGraph::from_graph(&loaded.graph, &playground_root(playground));
    let expected = reference(playground, "mermaid");
    let expected = expected.as_object().expect("diagrams by entrance key");
    assert_eq!(
        expected.len(),
        loaded.graph.entrances.len(),
        "one diagram per entrance"
    );
    let mut entrances = BTreeMap::new();
    let mut differences = Vec::new();
    for entrance in &loaded.graph.entrances {
        let key = entrance.key.as_str();
        let ours = mermaid::entrance(&graph, key).expect("the entrance is in the graph");
        assert_eq!(
            mermaid::entrance(&graph, key).as_deref(),
            Some(ours.as_str()),
            "deterministic"
        );
        let poc = expected
            .get(key)
            .and_then(Value::as_str)
            .expect("the PoC exported this entrance");
        let (poc_structure, our_structure) = (structure(poc), structure(&ours));
        if poc_structure != our_structure {
            let line = poc_structure
                .iter()
                .zip(&our_structure)
                .position(|(a, b)| a != b)
                .unwrap_or(poc_structure.len().min(our_structure.len()));
            differences.push(format!(
                "{key} differs at structural line {line}:\n    poc:    {:?}\n    unfold: {:?}",
                poc_structure.get(line),
                our_structure.get(line)
            ));
        }
        entrances.insert(key.to_owned(), ours);
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));

    let names = Names(&graph);
    let mut flows = BTreeMap::new();
    for file in &loaded.graph.files {
        for block in file.blocks.iter().filter(|block| block.kind.is_function_like()) {
            let flow = graph
                .flow(block.key.as_str(), &names)
                .expect("the block is in the graph");
            flows.insert(block.key.to_string(), mermaid::flow(&flow));
        }
    }
    let snapshot = serde_json::json!({"entrances": entrances, "flows": flows});
    let text = serde_json::to_string_pretty(&snapshot).expect("serialize the snapshot") + "\n";
    assert_golden(format!("mermaid/{playground}.json"), &text);
}

#[test]
fn typescript_entrance_diagrams_match_the_poc_structure() {
    check("ts-orders", "playground-ts-native");
}

#[test]
fn python_entrance_diagrams_match_the_poc_structure() {
    check("py-orders", "python-playground");
}

#[test]
fn structure_drops_source_labels_and_handler_numbers() {
    let text = "flowchart LR\n  %% note\n  c0([\"POST /orders\"])\n  subgraph c1_card[\"createOrder\"]\n    direction TB\n    c1_n0([\"createOrder(req)\"])\n    c1_n1([\"End\"])\n  end\n  c0 -->|\"handler 1\"| c1_n0\n  c0_target2[\"x (unresolved: wrapper)\"]\n";
    assert_eq!(
        structure(text),
        [
            "flowchart LR",
            "c0([\"POST /orders\"])",
            "subgraph c1_card[\"createOrder\"]",
            "direction TB",
            "c1_n0([\"…\"])",
            "c1_n1([\"End\"])",
            "end",
            "c0 -->|\"handler\"| c1_n0",
            "c0_target2[\"…\"]",
        ]
    );
}
