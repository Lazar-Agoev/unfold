//! Flow tests: every structure kind on the playgrounds' real facts (TypeScript and Python), and
//! the cases the playgrounds lack on small hand-built graphs.

use std::collections::BTreeMap;

use unfold_core::testkit::{GraphBuilder, Outcome, testdata_dir};
use unfold_core::vocab::{BlockKind, EdgeKind, Language, Role, SlotName};
use unfold_core::{FileGraph, Graph, canonical};

use super::*;
use crate::board::BoardGraph;

struct Labels(BTreeMap<String, String>);

impl FlowContext for Labels {
    fn label(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

struct Playground {
    graph: BoardGraph,
}

/// Names targets in other files, as the workspace does from the catalog.
struct Names<'a>(&'a BoardGraph);

impl FlowContext for Names<'_> {
    fn target_name(&self, key: &str) -> Option<String> {
        self.0.qualified_name(key)
    }
}

impl Playground {
    fn load(playground: &str) -> Self {
        let text =
            std::fs::read_to_string(testdata_dir().join(format!("goldens/{playground}.jsonl"))).unwrap();
        let graph: Graph = canonical::read(&text).unwrap();
        Self {
            graph: BoardGraph::from_graph(&graph, &testdata_dir().join("playgrounds").join(playground)),
        }
    }

    fn typescript() -> Self {
        Self::load("ts-orders")
    }

    fn python() -> Self {
        Self::load("py-orders")
    }

    fn flow(&self, key: &str) -> FlowModel {
        let flow = self.graph.flow(key, &Names(&self.graph)).unwrap();
        assert_eq!(flow.source, SourceState::Current, "{key}");
        flow
    }
}

fn node<'a>(flow: &'a FlowModel, key_suffix: &str) -> &'a FlowNode {
    flow.nodes
        .iter()
        .find(|node| {
            node.key
                .as_ref()
                .is_some_and(|key| key.as_str().ends_with(key_suffix))
        })
        .unwrap_or_else(|| panic!("no node ending in {key_suffix}"))
}

fn route(flow: &FlowModel, from: u32, to: u32) -> Option<RouteKind> {
    flow.routes
        .iter()
        .find(|route| route.from == from && route.to == to)
        .map(|route| route.kind)
}

fn leaves(flow: &FlowModel, from: u32) -> bool {
    flow.routes.iter().any(|route| route.from == from)
}

fn codes(flow: &FlowModel) -> Vec<WarningCode> {
    flow.warnings.iter().map(|warning| warning.code).collect()
}

#[test]
fn an_early_return_takes_the_yes_lane_and_the_rest_continues_below() {
    let flow = Playground::typescript().flow("src/routes/orders.ts#function:createOrder");
    let gate = node(&flow, "createOrder/if[0]");
    assert_eq!(
        (gate.kind, gate.label.as_str(), gate.fallback),
        (FlowNodeKind::Condition, "req.header?", true)
    );
    let queued = node(&flow, "createOrder/if[0]/variable:job");
    assert_eq!(queued.kind, FlowNodeKind::Call);
    assert_eq!(queued.label, "ordersQueue.add");
    assert_eq!(route(&flow, gate.id, queued.id), Some(RouteKind::Yes));
    let returned = node(&flow, "createOrder/if[0]/return[0]");
    assert_eq!((returned.kind, returned.lane), (FlowNodeKind::Return, 1));
    let placed = node(&flow, "createOrder/variable:order");
    assert_eq!((placed.label.as_str(), placed.lane), ("OrderService.place", 0));
    assert_eq!(route(&flow, gate.id, placed.id), Some(RouteKind::No));
    let end = flow.end.unwrap();
    assert_eq!(route(&flow, returned.id, end), Some(RouteKind::Return));
    assert!(!leaves(&flow, end));
    assert_eq!(flow.nodes[flow.start as usize].label, "createOrder");
    assert!(!flow.nodes[flow.start as usize].fallback);
    assert_eq!(flow.lanes, 2);
    let call = &placed.chips[placed.call.unwrap() as usize];
    assert_eq!(
        (call.kind, call.name.as_str()),
        (EdgeKind::Calls, "OrderService.place")
    );
    // The model is deterministic.
    assert_eq!(
        Playground::typescript().flow("src/routes/orders.ts#function:createOrder"),
        flow
    );
}

#[test]
fn a_chain_of_three_checks_folds_into_one_condition() {
    let flow = Playground::typescript().flow("src/lib/validate.ts#function:validateOrder");
    let head = node(&flow, "validateOrder/if[1]");
    assert_eq!(head.checks, Some(3));
    assert_eq!(head.outcome, Some(ChainOutcome::Throw));
    assert_eq!(head.label, "customerId, email, Array.isArray, items?");
    let next = node(&flow, "validateOrder/if[1]/if[0]");
    assert_eq!(route(&flow, head.id, next.id), Some(RouteKind::ElseIf));
    assert_eq!(next.checks, None, "the links of a chain never fold");
    let FlowItem::Branch {
        else_if, yes_ends, ..
    } = flow
        .tree
        .iter()
        .find(|item| matches!(item, FlowItem::Branch { node, .. } if *node == head.id))
        .unwrap()
    else {
        unreachable!()
    };
    assert!(*else_if && *yes_ends);
    // A short chain does not fold.
    assert_eq!(node(&flow, "validateOrder/if[0]").checks, None);
    // A nested callback is listed on its statement and opens as its own card.
    let items = node(&flow, "validateOrder/variable:validatedItems");
    assert_eq!(items.callbacks.len(), 1);
    assert_eq!(items.callbacks[0].label, "callback to items.map");
    assert!(items.callbacks[0].throws);
    assert!(
        flow.nodes
            .iter()
            .flat_map(|node| &node.chips)
            .all(|chip| !chip.source.as_str().contains("function[0]")),
        "the callback's calls belong to the callback"
    );
}

#[test]
fn loops_route_each_next_and_done_and_do_while_tests_after_its_body() {
    let flow = Playground::typescript().flow("src/jobs/cleanup.ts#function:cleanupExpiredOrders");
    let counted = node(&flow, "cleanupExpiredOrders/for[0]");
    assert_eq!(counted.kind, FlowNodeKind::Loop);
    assert_eq!(counted.label, "index, orders.length");
    let first = node(&flow, "cleanupExpiredOrders/for[0]/variable:order");
    assert_eq!(route(&flow, counted.id, first.id), Some(RouteKind::Each));
    assert!(
        flow.routes
            .iter()
            .any(|route| route.to == counted.id && route.kind == RouteKind::Next)
    );
    let after = node(&flow, "cleanupExpiredOrders/variable:removed");
    assert_eq!(route(&flow, counted.id, after.id), Some(RouteKind::Done));

    let repeat = node(&flow, "do_while[0]");
    let body = node(&flow, "do_while[0]/variable:id");
    assert_eq!(route(&flow, repeat.id, body.id), Some(RouteKind::Again));
    assert!(
        flow.routes
            .iter()
            .any(|route| route.to == repeat.id && route.kind == RouteKind::Test)
    );
    let gate = node(&flow, "cleanupExpiredOrders/if[0]");
    assert_eq!(
        route(&flow, gate.id, body.id),
        Some(RouteKind::Yes),
        "a do-while is entered at its body"
    );
    assert!(
        flow.tree
            .iter()
            .any(|item| matches!(item, FlowItem::Branch { yes, .. }
        if yes.iter().any(|item| matches!(item, FlowItem::Loop { test_after: true, .. }))))
    );
}

#[test]
fn try_and_catch_are_regions_with_an_error_route() {
    let flow = Playground::typescript().flow("src/jobs/import-orders.ts#function:importOrders");
    let attempt = node(&flow, "for_of[0]/try[0]/variable:draft");
    let handler = node(&flow, "for_of[0]/try[0]/catch[0]/expression_statement[0]");
    assert_eq!(route(&flow, attempt.id, handler.id), Some(RouteKind::Error));
    assert!(RouteKind::Error.dashed());
    assert_eq!((attempt.lane, handler.lane), (0, 1));
    let region = flow
        .tree
        .iter()
        .find_map(|item| match item {
            FlowItem::Loop { body, .. } => body.first(),
            _ => None,
        })
        .unwrap();
    let FlowItem::Try {
        binding, catch_lane, ..
    } = region
    else {
        panic!("expected a try region, found {region:?}");
    };
    assert_eq!((binding.as_deref(), *catch_lane), (Some("error"), 1));
    assert!(flow.warnings.is_empty());
}

#[test]
fn typescript_switch_and_try_finally_are_opaque_without_outgoing_routes() {
    let playground = Playground::typescript();
    let retry = playground.flow("src/lib/retry.ts#function:withRetry");
    let region = node(&retry, "while[0]/try[0]");
    assert_eq!(
        (region.kind, region.label.as_str(), region.warning),
        (
            FlowNodeKind::Opaque,
            "try … finally",
            Some(WarningCode::TryFinally)
        )
    );
    assert!(!leaves(&retry, region.id));
    assert_eq!(codes(&retry), [WarningCode::TryFinally]);

    let cli = playground.flow("src/cli.ts#function:main");
    let switch = node(&cli, "main/switch[0]");
    assert_eq!(
        (switch.kind, switch.label.as_str()),
        (FlowNodeKind::Opaque, "switch command")
    );
    assert!(!leaves(&cli, switch.id));
    assert_eq!(cli.end, None, "no route reaches End past an opaque region");
    assert_eq!(codes(&cli), [WarningCode::Switch]);
}

#[test]
fn a_class_lists_its_members_as_definitions() {
    let flow = Playground::typescript().flow("src/services/order-service.ts#class:OrderService");
    assert!(flow.routes.is_empty());
    assert!(
        flow.nodes[1..]
            .iter()
            .all(|node| node.kind == FlowNodeKind::Definition)
    );
    assert_eq!(node(&flow, "method:place").label, "OrderService.place");
    assert_eq!(codes(&flow), [WarningCode::ClassMembers]);
}

#[test]
fn python_structures() {
    let playground = Playground::python();
    let validate = playground.flow("src/orders_playground/lib/validate.py#function:validate_order");
    let head = node(&validate, "validate_order/if[0]");
    assert_eq!((head.checks, head.outcome), (Some(3), Some(ChainOutcome::Throw)));
    assert_eq!(head.label, "isinstance, value, dict, str?");
    let each = node(&validate, "validate_order/for_of[0]");
    assert_eq!(
        (each.kind, each.label.as_str()),
        (FlowNodeKind::Loop, "item, items")
    );
    assert!(
        validate
            .routes
            .iter()
            .any(|route| route.from == each.id && route.kind == RouteKind::Done)
    );

    let scan = playground.flow("src/orders_playground/features/control.py#function:scan");
    for (suffix, code) in [
        ("scan/for_of[0]", WarningCode::PythonLoopElse),
        ("scan/try[0]", WarningCode::PythonTry),
        ("scan/with[0]", WarningCode::PythonWith),
    ] {
        let region = node(&scan, suffix);
        assert_eq!(
            (region.kind, region.warning),
            (FlowNodeKind::Opaque, Some(code)),
            "{suffix}"
        );
        assert!(!leaves(&scan, region.id), "{suffix}");
    }
    let returned = node(&scan, "scan/return[0]");
    assert!(
        !scan.routes.iter().any(|route| route.to == returned.id),
        "statements after an opaque region stay visible without an incoming route"
    );

    let cli = playground.flow("src/orders_playground/cli.py#function:main");
    assert!(codes(&cli).contains(&WarningCode::PythonMatch));

    let counter = playground.flow("src/orders_playground/features/control.py#function:counter");
    let nested = node(&counter, "counter/function:increment");
    assert_eq!(
        (nested.kind, nested.label.as_str()),
        (FlowNodeKind::Definition, "counter.increment")
    );
}

#[test]
fn authored_labels_replace_fallbacks_but_never_the_type() {
    let playground = Playground::typescript();
    let key = "src/routes/orders.ts#function:createOrder";
    let order = format!("{key}/variable:order");
    let labels = Labels(BTreeMap::from([
        (order, "  Place the order  ".to_owned()),
        (key.to_owned(), "Never shown on Start".to_owned()),
        (format!("{key}/if[0]"), "   ".to_owned()),
    ]));
    let flow = playground.graph.flow(key, &labels).unwrap();
    let placed = node(&flow, "createOrder/variable:order");
    assert_eq!(
        (placed.label.as_str(), placed.fallback, placed.kind),
        ("Place the order", false, FlowNodeKind::Call)
    );
    assert_eq!(flow.nodes[flow.start as usize].label, "createOrder");
    let gate = node(&flow, "createOrder/if[0]");
    assert!(gate.fallback, "a blank label is no label");
    assert_eq!(
        flow.unlabeled().count(),
        flow.nodes.len() - 3,
        "all but Start, End, and the labeled node"
    );
}

/// A TypeScript function `f` whose body is built by `body`, with the builder and the function.
fn typescript(text: &str, body: impl FnOnce(&mut GraphBuilder, unfold_core::testkit::BlockRef)) -> FileGraph {
    let mut builder = GraphBuilder::new();
    let root = builder.file("a.ts", Language::TypeScript, text);
    let range = builder.find(root, text.trim_end());
    let function = builder.block(root, BlockKind::Function, Some(Role::Body), Some("f"), range);
    body(&mut builder, function);
    builder.build().files.remove(0)
}

fn flow_of(file: &FileGraph, text: &str) -> FlowModel {
    build_flow(file, 1, Some(text), SourceState::Current, &NoContext).unwrap()
}

#[test]
fn unlabeled_break_and_continue_transfer_to_their_loop() {
    let text = "function f() { while (more) { if (a) break; if (b) continue; step(); } done(); }\n";
    let file = typescript(text, |b, function| {
        let root = function;
        let range = b.find(root, "while (more) { if (a) break; if (b) continue; step(); }");
        let loop_block = b.block(function, BlockKind::While, Some(Role::Body), None, range);
        let condition = b.find(root, "more");
        b.slot(loop_block, SlotName::Condition, None, condition);
        for (needle, condition, kind, word) in [
            ("if (a) break;", "a", BlockKind::Break, "break;"),
            ("if (b) continue;", "b", BlockKind::Continue, "continue;"),
        ] {
            let range = b.find(root, needle);
            let gate = b.block(loop_block, BlockKind::If, Some(Role::Body), None, range);
            let condition = b.find_from(root, condition, range.start);
            b.slot(gate, SlotName::Condition, None, condition);
            let transfer = b.find_from(root, word, range.start);
            b.block(gate, kind, Some(Role::Then), None, transfer);
        }
        for needle in ["step();", "done();"] {
            let range = b.find(root, needle);
            let statement = b.block(
                if needle == "step();" { loop_block } else { function },
                BlockKind::ExpressionStatement,
                Some(Role::Body),
                None,
                range,
            );
            let call_range = b.find(root, &needle[..needle.len() - 1]);
            let call = b.block(
                statement,
                BlockKind::Call,
                Some(Role::Expression),
                None,
                call_range,
            );
            b.slot(statement, SlotName::Expression, None, call_range);
            b.edge(
                call,
                EdgeKind::Calls,
                Outcome::external("lib"),
                &needle[..needle.len() - 3],
                None,
            );
        }
    });
    let flow = flow_of(&file, text);
    let gate = flow
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Loop)
        .unwrap();
    let broken = flow
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Break)
        .unwrap();
    let continued = flow
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Continue)
        .unwrap();
    let done = flow.nodes.iter().find(|node| node.label == "done").unwrap();
    assert_eq!(route(&flow, continued.id, gate.id), Some(RouteKind::Continue));
    assert_eq!(route(&flow, broken.id, done.id), Some(RouteKind::Break));
    assert_eq!(route(&flow, gate.id, done.id), Some(RouteKind::Done));
    assert_eq!(done.kind, FlowNodeKind::Call, "a main call sets the node's type");
    assert!(flow.warnings.is_empty());

    // A labeled transfer gets no guessed destination.
    let labeled = text.replace("break;", "break outer;");
    let file = typescript(&labeled, |b, function| {
        let root = function;
        let range = b.find(
            root,
            "while (more) { if (a) break outer; if (b) continue; step(); }",
        );
        let loop_block = b.block(function, BlockKind::While, Some(Role::Body), None, range);
        let range = b.find(root, "break outer;");
        b.block(loop_block, BlockKind::Break, Some(Role::Body), None, range);
    });
    let flow = flow_of(&file, &labeled);
    let opaque = flow
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Opaque)
        .unwrap();
    assert_eq!(opaque.label, "Labeled break");
    assert_eq!(codes(&flow), [WarningCode::LabeledBreak]);

    // A transfer outside any loop has no route.
    let orphan = "function f() { break; }\n";
    let file = typescript(orphan, |b, function| {
        let range = b.find(function, "break;");
        b.block(function, BlockKind::Break, Some(Role::Body), None, range);
    });
    let flow = flow_of(&file, orphan);
    assert_eq!(codes(&flow), [WarningCode::OrphanTransfer]);
    assert_eq!(flow.end, None);
}

#[test]
fn deep_nesting_is_drawn_up_to_a_limit() {
    let depth = 300;
    let mut text = String::from("function f() {");
    for _ in 0..depth {
        text.push_str(" if (x) {");
    }
    for _ in 0..depth {
        text.push_str(" }");
    }
    text.push_str(" }\n");
    let file = typescript(&text, |b, function| {
        let mut parent = function;
        let mut from = 0;
        for level in 0..depth {
            let range = b.find_from(function, "if (x)", from);
            let end = u32::try_from(text.len() - 3 - 2 * level).unwrap();
            let block = b.block(
                parent,
                BlockKind::If,
                Some(if level == 0 { Role::Body } else { Role::Then }),
                None,
                unfold_core::ByteRange {
                    start: range.start,
                    end,
                },
            );
            parent = block;
            from = range.end;
        }
    });
    let flow = flow_of(&file, &text);
    assert!(codes(&flow).contains(&WarningCode::TooDeep));
    assert!(flow.nodes.len() < depth);
}

#[test]
fn changed_source_shows_only_start_and_why() {
    let text = "function f() { go(); }\n";
    let file = typescript(text, |_, _| {});
    let flow = build_flow(&file, 1, None, SourceState::Changed, &NoContext).unwrap();
    assert_eq!(flow.source, SourceState::Changed);
    assert_eq!(flow.nodes.len(), 2);
    assert!(flow.routes.is_empty());
    assert_eq!(codes(&flow), [WarningCode::SourceChanged]);
    assert!(build_flow(&file, 99, Some(text), SourceState::Current, &NoContext).is_err());
}

/// The text `focus` covers in `text`, counted independently: lines split at `\n`, columns in
/// UTF-16 code units.
fn focused(text: &str, focus: SourceFocus) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut starts = vec![0];
    starts.extend(
        units
            .iter()
            .enumerate()
            .filter(|(_, unit)| **unit == u16::from(b'\n'))
            .map(|(index, _)| index + 1),
    );
    let at = |line: u32, column: u32| starts[line as usize - 1] + column as usize;
    String::from_utf16(&units[at(focus.start_line, focus.start_column)..at(focus.end_line, focus.end_column)])
        .unwrap()
}

/// The source of a playground file.
fn playground_text(folder: &str, path: &str) -> String {
    std::fs::read_to_string(testdata_dir().join("playgrounds").join(folder).join(path)).unwrap()
}

#[test]
fn focus_is_the_condition_the_main_call_or_the_statement() {
    let playground = Playground::typescript();
    let flow = playground.flow("src/routes/orders.ts#function:createOrder");
    let text = playground_text("ts-orders", "src/routes/orders.ts");
    let gate = node(&flow, "createOrder/if[0]").focus.unwrap();
    assert_eq!(
        gate,
        SourceFocus {
            start_line: 27,
            start_column: 6,
            end_line: 27,
            end_column: 46
        }
    );
    assert_eq!(
        focused(&text, gate),
        r#"req.header("prefer") === "respond-async""#
    );
    for (suffix, expected) in [
        ("createOrder/variable:order", "orderService.place(draft)"),
        (
            "createOrder/if[0]/variable:job",
            r#"ordersQueue.add("place-order", { order: draft })"#,
        ),
        (
            "createOrder/expression_statement[1]",
            "res.status(201).json(order)",
        ),
        ("createOrder/if[0]/return[0]", "return;"),
        ("createOrder/variable:draft", "draft: OrderDraft = req.body"),
    ] {
        let found = node(&flow, suffix).focus.unwrap();
        assert_eq!(focused(&text, found), expected, "{suffix}");
    }
    let start = &flow.nodes[flow.start as usize];
    let end = &flow.nodes[flow.end.unwrap() as usize];
    assert_eq!((start.focus, end.focus), (None, None));
    assert!(
        flow.nodes
            .iter()
            .filter(|node| !matches!(node.kind, FlowNodeKind::Start | FlowNodeKind::End))
            .all(|node| node.focus.is_some())
    );
    // The focus crosses IPC as camelCase lines and columns.
    assert_eq!(
        serde_json::to_value(node(&flow, "createOrder/if[0]")).unwrap()["focus"],
        serde_json::json!({"startLine": 27, "startColumn": 6, "endLine": 27, "endColumn": 46})
    );

    // A loop's focus is its condition, or else its header; a multi-line statement spans lines.
    let cleanup = playground.flow("src/jobs/cleanup.ts#function:cleanupExpiredOrders");
    let text = playground_text("ts-orders", "src/jobs/cleanup.ts");
    for (suffix, expected) in [
        ("cleanupExpiredOrders/for[0]", "index < orders.length"),
        ("do_while[0]", "expired.length > 0"),
    ] {
        assert_eq!(
            focused(&text, node(&cleanup, suffix).focus.unwrap()),
            expected,
            "{suffix}"
        );
    }
    let retry = playground.flow("src/lib/retry.ts#function:withRetry");
    let text = playground_text("ts-orders", "src/lib/retry.ts");
    let region = node(&retry, "while[0]/try[0]").focus.unwrap();
    assert_eq!(
        (region.start_line, region.end_line),
        (7, 14),
        "an opaque region's whole block"
    );
    assert!(focused(&text, region).starts_with("try {") && focused(&text, region).ends_with('}'));
    let imports = playground.flow("src/jobs/import-orders.ts#function:importOrders");
    let text = playground_text("ts-orders", "src/jobs/import-orders.ts");
    assert_eq!(
        focused(&text, node(&imports, "importOrders/for_of[0]").focus.unwrap()),
        "for (const [index, value] of input.entries())"
    );
}

#[test]
fn python_focus_uses_the_same_ranges() {
    let playground = Playground::python();
    let flow = playground.flow("src/orders_playground/lib/validate.py#function:validate_order");
    let text = playground_text("py-orders", "src/orders_playground/lib/validate.py");
    for (suffix, expected) in [
        ("validate_order/if[0]", "not isinstance(value, dict)"),
        (
            "validate_order/if[0]/if[0]",
            r#"not isinstance(value.get("customer_id"), str) or not value["customer_id"]"#,
        ),
        ("validate_order/variable:items", r#"value.get("items")"#),
        // A Python suite's recorded range starts at its colon.
        ("validate_order/for_of[0]", "for item in items"),
        ("validate_order/return[0]", "return cast(OrderDraft, value)"),
    ] {
        assert_eq!(
            focused(&text, node(&flow, suffix).focus.unwrap()),
            expected,
            "{suffix}"
        );
    }
    let gate = node(&flow, "validate_order/if[0]").focus.unwrap();
    assert_eq!((gate.start_line, gate.start_column), (8, 7));
}

#[test]
fn focus_columns_count_utf16_code_units() {
    // `é` is two UTF-8 bytes and one UTF-16 unit; `😀` is four bytes and two units.
    let text = "// café 😀\nfunction f() {\n  const s = \"é😀\"; if (s === \"😀é\") { go(\"ü\"); }\n}\n";
    let mut b = GraphBuilder::new();
    let root = b.file("a.ts", Language::TypeScript, text);
    let range = b.find(root, "function f() {");
    let function_range = unfold_core::ByteRange {
        start: range.start,
        end: u32::try_from(text.trim_end().len()).unwrap(),
    };
    let function = b.block(
        root,
        BlockKind::Function,
        Some(Role::Body),
        Some("f"),
        function_range,
    );
    let variable = b.find(root, "const s = \"é😀\";");
    let variable = b.block(
        function,
        BlockKind::Variable,
        Some(Role::Body),
        Some("s"),
        variable,
    );
    let initializer = b.find(root, "\"é😀\"");
    b.slot(variable, SlotName::Initializer, None, initializer);
    let gate_range = b.find(root, "if (s === \"😀é\") { go(\"ü\"); }");
    let gate = b.block(function, BlockKind::If, Some(Role::Body), None, gate_range);
    let condition = b.find(root, "s === \"😀é\"");
    b.slot(gate, SlotName::Condition, None, condition);
    let statement = b.find(root, "go(\"ü\");");
    let statement = b.block(
        gate,
        BlockKind::ExpressionStatement,
        Some(Role::Then),
        None,
        statement,
    );
    let call_range = b.find(root, "go(\"ü\")");
    b.slot(statement, SlotName::Expression, None, call_range);
    let call = b.block(
        statement,
        BlockKind::Call,
        Some(Role::Expression),
        None,
        call_range,
    );
    b.edge(call, EdgeKind::Calls, Outcome::external("lib"), "go", None);
    let file = b.build().files.remove(0);
    let flow = build_flow(&file, 1, Some(text), SourceState::Current, &NoContext).unwrap();
    let focus_of = |kind: FlowNodeKind| {
        flow.nodes
            .iter()
            .find(|node| node.kind == kind)
            .and_then(|node| node.focus)
            .unwrap()
    };
    let condition = focus_of(FlowNodeKind::Condition);
    assert_eq!(
        condition,
        SourceFocus {
            start_line: 3,
            start_column: 23,
            end_line: 3,
            end_column: 34
        }
    );
    assert_eq!(focused(text, condition), "s === \"😀é\"");
    let called = focus_of(FlowNodeKind::Call);
    assert_eq!((called.start_column, called.end_column), (38, 45));
    assert_eq!(focused(text, called), "go(\"ü\")");
    assert_eq!(
        focused(text, focus_of(FlowNodeKind::Variable)),
        "const s = \"é😀\";"
    );

    // Source that isn't current has no focus, even when its old text is at hand.
    for state in [SourceState::Changed, SourceState::Missing] {
        for given in [None, Some(text)] {
            let flow = build_flow(&file, 1, given, state, &NoContext).unwrap();
            assert!(flow.nodes.iter().all(|node| node.focus.is_none()), "{state:?}");
        }
    }
}
