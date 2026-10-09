//! Flow parity with the proof of concept: the Flow of every definition in both playgrounds is
//! compared with what the PoC's `reader/core/flow.ts` built from the same facts
//! (`testdata/reference/poc-reader/<playground>/flows.json`, exported by `tools/oracles`).
//!
//! Compared exactly: node count, order, kinds (`stub` is `definition`), keys, lanes, `else if`
//! chain checks and outcomes, chips (occurrence and edge kind, in order), the main call
//! (occurrence, kind, and name), nested definitions (key, lines, whether they throw), every
//! route (from, to, label, dashed), the lane tree, Start, End, the lane count, and the number of
//! warnings. Labels are compared too, except where Unfold deliberately differs:
//!
//! - **Start** shows the block's name, not the written text of its name slot.
//! - **Variables without a name or named bindings** fall back to the names written in the name
//!   slot instead of the clipped slot text (a pattern is code, never a label).
//! - **Nested definitions** are listed with their catalog label (`callback to …`), so the PoC's
//!   `callback` label is not compared.
//! - **The catch binding** is the first name written in the binding slot, not its text.

mod common;

use common::{Differences, load, playground_root, reference};
use serde_json::{Value, json};
use unfold_reader::board::BoardGraph;
use unfold_reader::flow::{FlowContext, FlowItem, FlowModel, FlowNode, FlowNodeKind};

struct Names<'a>(&'a BoardGraph);

impl FlowContext for Names<'_> {
    fn target_name(&self, key: &str) -> Option<String> {
        self.0.qualified_name(key)
    }
}

fn kind(node: &FlowNode) -> &'static str {
    match node.kind {
        FlowNodeKind::Start => "start",
        FlowNodeKind::End => "end",
        FlowNodeKind::Variable => "variable",
        FlowNodeKind::Call => "call",
        FlowNodeKind::Statement => "statement",
        FlowNodeKind::Condition => "condition",
        FlowNodeKind::Loop => "loop",
        FlowNodeKind::Return => "return",
        FlowNodeKind::Throw => "throw",
        FlowNodeKind::Break => "break",
        FlowNodeKind::Continue => "continue",
        FlowNodeKind::Definition => "stub",
        FlowNodeKind::Opaque => "opaque",
    }
}

/// Whether the PoC's label for this node is one Unfold deliberately computes differently.
fn label_differs_by_design(node: &FlowNode, poc: &Value) -> bool {
    node.kind == FlowNodeKind::Start
        || (node.kind == FlowNodeKind::Variable
            && poc["described"] != json!(true)
            && node.label != poc["label"])
}

fn node_id(id: u32) -> Value {
    json!(format!("n{id}"))
}

/// Our lane tree in the PoC's shape.
fn tree(items: &[FlowItem]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| match item {
                FlowItem::Node { node, ends, lanes } => json!({"type": "node", "id": node_id(*node), "ends": ends, "lanes": lanes}),
                FlowItem::Branch {
                    node,
                    yes,
                    no,
                    yes_ends,
                    no_ends,
                    ends,
                    lanes,
                    yes_lane,
                    else_if,
                } => json!({"type": "branch", "id": node_id(*node), "yes": tree(yes), "no": tree(no), "yesEnds": yes_ends,
                    "noEnds": no_ends, "ends": ends, "lanes": lanes, "yesLane": yes_lane, "elseIf": else_if}),
                FlowItem::Loop {
                    node,
                    body,
                    test_after,
                    ends,
                    lanes,
                } => json!({"type": "loop", "id": node_id(*node), "body": tree(body), "testAfter": test_after, "ends": ends, "lanes": lanes}),
                FlowItem::Try {
                    key,
                    catch_key,
                    body,
                    handler,
                    body_ends,
                    handler_ends,
                    ends,
                    lanes,
                    catch_lane,
                    ..
                } => json!({"type": "try", "key": key.as_str(), "body": tree(body), "handler": tree(handler),
                    "catchKey": catch_key.as_str(), "bodyEnds": body_ends, "handlerEnds": handler_ends, "ends": ends,
                    "lanes": lanes, "catchLane": catch_lane}),
            })
            .collect(),
    )
}

/// The PoC tree without the catch binding text.
fn poc_tree(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(poc_tree).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "binding")
                .map(|(key, value)| (key.clone(), poc_tree(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn compare_flow(key: &str, flow: &FlowModel, expected: &Value, differences: &mut Differences) {
    let nodes = expected["nodes"].as_array().expect("nodes");
    differences.compare(
        &format!("{key} node count"),
        &json!(nodes.len()),
        &json!(flow.nodes.len()),
    );
    for (node, poc) in flow.nodes.iter().zip(nodes) {
        let what = format!("{key} n{}", node.id);
        differences.compare(&format!("{what} kind"), &poc["kind"], &json!(kind(node)));
        differences.compare(
            &format!("{what} key"),
            &poc["key"],
            &json!(node.key.as_ref().map(|key| key.as_str())),
        );
        differences.compare(&format!("{what} lane"), &poc["lane"], &json!(node.lane));
        if !label_differs_by_design(node, poc) {
            differences.compare(&format!("{what} label"), &poc["label"], &json!(node.label));
        }
        differences.compare(
            &format!("{what} checks"),
            &poc.get("checks").cloned().unwrap_or(Value::Null),
            &json!(node.checks),
        );
        differences.compare(
            &format!("{what} outcome"),
            &poc.get("outcome").cloned().unwrap_or(Value::Null),
            &serde_json::to_value(node.outcome).expect("serialize the outcome"),
        );
        let poc_chips: Vec<Value> = poc["chips"]
            .as_array()
            .expect("chips")
            .iter()
            .map(|chip| json!([chip["source"], chip["kind"]]))
            .collect();
        let chips: Vec<Value> = node
            .chips
            .iter()
            .map(|chip| json!([chip.source.as_str(), chip.kind.as_str()]))
            .collect();
        differences.compare(&format!("{what} chips"), &json!(poc_chips), &json!(chips));
        let poc_call = poc
            .get("call")
            .map(|call| json!([call["source"], call["kind"], call["name"]]));
        let call = node.call.map(|index| {
            let chip = &node.chips[index as usize];
            json!([chip.source.as_str(), chip.kind.as_str(), chip.name])
        });
        differences.compare(&format!("{what} call"), &json!(poc_call), &json!(call));
        let poc_callbacks: Vec<Value> = poc
            .get("callbacks")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| json!([item["key"], item["lines"], item["exits"] == json!("throws")]))
                    .collect()
            })
            .unwrap_or_default();
        let callbacks: Vec<Value> = node
            .callbacks
            .iter()
            .map(|item| json!([item.key.as_str(), item.lines, item.throws]))
            .collect();
        differences.compare(
            &format!("{what} callbacks"),
            &json!(poc_callbacks),
            &json!(callbacks),
        );
    }
    let routes: Vec<Value> = flow
        .routes
        .iter()
        .map(|route| {
            let mut edge = vec![node_id(route.from), node_id(route.to), json!(route.kind.label())];
            if route.kind.dashed() {
                edge.push(json!(true));
            }
            Value::Array(edge)
        })
        .collect();
    differences.compare(&format!("{key} routes"), &expected["edges"], &json!(routes));
    differences.compare(
        &format!("{key} tree"),
        &poc_tree(&expected["tree"]),
        &tree(&flow.tree),
    );
    differences.compare(&format!("{key} start"), &expected["start"], &node_id(flow.start));
    differences.compare(
        &format!("{key} end"),
        &expected["end"],
        &json!(flow.end.map(node_id)),
    );
    differences.compare(&format!("{key} lanes"), &expected["lanes"], &json!(flow.lanes));
    differences.compare(
        &format!("{key} warnings"),
        &json!(expected["warnings"].as_array().map_or(0, Vec::len)),
        &json!(flow.warnings.len()),
    );
}

fn check(playground: &str, corpus: &str) {
    let loaded = load(corpus);
    let graph = BoardGraph::from_graph(&loaded.graph, &playground_root(playground));
    let names = Names(&graph);
    let expected = reference(playground, "flows");
    let mut differences = Differences::default();
    let flows = expected.as_object().expect("flows by key");
    assert!(flows.len() > 50, "the reference covers every definition");
    for (key, poc) in flows {
        let Some(flow) = graph.flow(key, &names) else {
            differences.0.push(format!("{key}: no flow"));
            continue;
        };
        assert_eq!(
            flow.source,
            unfold_reader::source::SourceState::Current,
            "{key} verifies"
        );
        compare_flow(key, &flow, poc, &mut differences);
    }
    differences.assert_none(playground);
}

#[test]
fn typescript_flows_match_the_poc_reader() {
    check("ts-orders", "playground-ts-native");
}

#[test]
fn python_flows_match_the_poc_reader() {
    check("py-orders", "python-playground");
}
