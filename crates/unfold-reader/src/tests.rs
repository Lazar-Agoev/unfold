//! The reader's views on the playgrounds' golden graphs: owners, the catalog, callers, outgoing
//! links, closures, Reached from, and verified source.

use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, NodeRef};
use unfold_core::vocab::{Inline, Language, LinkKind, NodeKind, Outcome};
use unfold_core::{FileGraph, Key};

use crate::catalog;
use crate::relations::{callers, outgoing};
use crate::source::{SourceState, verify_file};
use crate::testing::Playground;
use crate::tree::{Blocks, FileTree};

const PLAYGROUNDS: [&str; 2] = ["ts-orders", "py-orders"];

/// The nearest definition above node `id`, found by walking parents.
fn nearest_definition(file: &FileGraph, id: u32) -> Option<u32> {
    let mut parent = file.nodes[id as usize].parent;
    while let Some(ancestor) = parent {
        if file.nodes[ancestor as usize].kind().is_definition() {
            return Some(ancestor);
        }
        parent = file.nodes[ancestor as usize].parent;
    }
    None
}

#[test]
fn every_node_belongs_to_the_nearest_definition_above_it() {
    for name in PLAYGROUNDS {
        let playground = Playground::load(name);
        let index = playground.index();
        let mut checked = 0;
        for file in &playground.graph.files {
            let tree = FileTree::new(file.clone());
            for (id, node) in file.nodes.iter().enumerate() {
                let id = u32::try_from(id).unwrap();
                let expected = nearest_definition(file, id);
                assert_eq!(tree.owner(id), expected, "{}", node.key);
                let owner = expected.map(|owner| &file.nodes[owner as usize].key);
                assert_eq!(index.owner(node.key.as_str()), owner, "{}", node.key);
                checked += 1;
            }
        }
        assert!(checked > 400, "{name}: {checked} nodes");
    }
}

#[test]
fn a_callback_owns_its_calls_and_a_method_belongs_to_its_class() {
    let playground = Playground::load("ts-orders");
    let index = playground.index();
    let callback = "src/routes/health.ts#/call:healthRouter.get(\"/live\")/callback[0]";
    assert_eq!(
        index.owner(callback).map(Key::as_str),
        Some("src/routes/health.ts#")
    );
    let (tree, id) = playground.block(callback);
    assert!(tree.owned(id).count() > 0);
    assert!(tree.owned(id).all(|owned| tree.owner(owned) == Some(id)));
    assert_eq!(
        index
            .owner("src/services/order-service.ts#OrderService.place")
            .map(Key::as_str),
        Some("src/services/order-service.ts#OrderService")
    );
    assert_eq!(index.owner("src/cli.ts#"), None);
    assert!(!index.contains("src/cli.ts#absent"));
}

#[test]
fn a_callback_written_as_an_object_property_is_labeled_by_its_property() {
    let text = "useMutation({ onSuccess: () => items.map(() => done()), retry: wrap(() => again()) });\n";
    let mut builder = GraphBuilder::new();
    let module = builder.file("a.ts", Language::TypeScript, text);
    let mutation = builder.call(
        module,
        &Callee::name("useMutation"),
        builder.find(module, "useMutation({"),
    );
    let success = builder.inline(
        mutation,
        Inline::Callback,
        builder.find(module, "() => items.map(() => done())"),
    );
    builder.node_mut(success).name = Some("onSuccess".into());
    let map = builder.call(
        success,
        &Callee::name("items").member("map"),
        builder.find(module, "items.map("),
    );
    let each = builder.inline(map, Inline::Callback, builder.find(module, "() => done()"));
    let wrap = builder.call(mutation, &Callee::name("wrap"), builder.find(module, "wrap("));
    let again = builder.inline(wrap, Inline::Callback, builder.find(module, "() => again()"));
    let graph = builder.build();
    let file = &graph.files[0];
    let label = |node: NodeRef| {
        let id = node.local_id();
        (
            catalog::label(&file.nodes, &file.file.path, id),
            catalog::qualified_name(&file.nodes, &file.file.path, id),
        )
    };
    assert_eq!(label(success), ("onSuccess".to_owned(), "onSuccess".to_owned()));
    assert_eq!(
        label(each),
        (
            "callback to items.map".to_owned(),
            "onSuccess.callback to items.map".to_owned()
        )
    );
    assert_eq!(
        label(again),
        ("callback to wrap".to_owned(), "callback to wrap".to_owned())
    );
}

#[test]
fn the_catalog_lists_every_definition_with_its_label() {
    let ts = Playground::load("ts-orders");
    let entries: Vec<_> = ts.graph.files.iter().flat_map(catalog::definitions).collect();
    let definitions = ts
        .graph
        .files
        .iter()
        .flat_map(|file| &file.nodes)
        .filter(|node| node.kind().is_definition())
        .count();
    assert_eq!(entries.len(), definitions);
    let entry = |key: &str| {
        entries
            .iter()
            .find(|entry| entry.key.as_str() == key)
            .unwrap_or_else(|| panic!("{key} is in the catalog"))
    };
    let label = |key: &str| (&*entry(key).label, &*entry(key).qualified_name);
    assert_eq!(
        label("src/routes/health.ts#/call:healthRouter.get(\"/live\")/callback[0]"),
        ("callback to healthRouter.get", "callback to healthRouter.get")
    );
    assert_eq!(
        label("src/cli.ts#/call:main().catch/callback[0]"),
        ("callback to main().catch", "callback to main().catch")
    );
    assert_eq!(
        label("src/jobs/report.ts#printReport/call:orderService.list().map/callback[0]"),
        (
            "callback to orderService.list().map",
            "printReport.callback to orderService.list().map"
        )
    );
    assert_eq!(
        label("src/services/order-service.ts#OrderService.place"),
        ("place", "OrderService.place")
    );
    assert_eq!(
        label("src/lib/logger.ts#logger.info"),
        ("logger.info", "logger.info")
    );
    assert_eq!(label("src/types.ts#OrderStatus"), ("OrderStatus", "OrderStatus"));
    assert_eq!(label("src/cli.ts#"), ("src/cli.ts", "src/cli.ts"));
    assert_eq!(entry("src/types.ts#OrderStatus").kind, NodeKind::Type);

    // The store keeps the catalog the reader computes.
    let reader = ts.reader();
    let stored = reader
        .definition("src/routes/health.ts#/call:healthRouter.get(\"/live\")/callback[0]")
        .unwrap()
        .unwrap();
    assert_eq!(&*stored.label, "callback to healthRouter.get");

    let py = Playground::load("py-orders");
    let tree = py.tree("src/orders_playground/queues/email_worker.py");
    let callback = tree
        .id("src/orders_playground/queues/email_worker.py#email_task/call:with_retry/callback[0]")
        .unwrap();
    assert_eq!(tree.label(callback), "callback to with_retry");
    assert_eq!(tree.qualified_name(callback), "email_task.callback to with_retry");
}

#[test]
fn callers_come_a_page_at_a_time_from_the_reverse_index() {
    let playground = Playground::load("ts-orders");
    let reader = playground.reader();
    let index = playground.index();
    let key = "src/lib/validate.ts#validateOrder";
    let first = callers(&reader, &index, key, None, 2).unwrap();
    assert_eq!(first.total, 4);
    assert_eq!(first.callers.len(), 2);
    let second = callers(&reader, &index, key, first.next.as_ref(), 2).unwrap();
    assert!(second.next.is_none());
    let all: Vec<(&str, &str, u32)> = first
        .callers
        .iter()
        .chain(&second.callers)
        .map(|caller| (caller.owner.as_str(), caller.path.as_str(), caller.line))
        .collect();
    assert_eq!(
        all,
        [
            (
                "src/jobs/import-orders.ts#importOrders",
                "src/jobs/import-orders.ts",
                13
            ),
            (
                "src/jobs/process-order.ts#processOrder",
                "src/jobs/process-order.ts",
                8
            ),
            (
                "src/routes/middleware.ts#validateBody",
                "src/routes/middleware.ts",
                14
            ),
            (
                "src/services/order-service.ts#OrderService.place",
                "src/services/order-service.ts",
                14
            ),
        ]
    );
    assert!(first.callers.iter().all(|caller| caller.kind == LinkKind::Calls));

    // A definition passed as a value is called by the call that passes it.
    let passed = callers(&reader, &index, "src/routes/orders.ts#createOrder", None, 10).unwrap();
    let [caller] = passed.callers.as_slice() else {
        panic!("one caller: {passed:?}");
    };
    assert_eq!(caller.kind, LinkKind::Passes);
    assert_eq!(caller.owner.as_str(), "src/routes/orders.ts#");
    assert_eq!(
        caller.call.as_str(),
        "src/routes/orders.ts#/call:ordersRouter.post"
    );
}

#[test]
fn outgoing_links_are_the_calls_a_definition_owns() {
    let playground = Playground::load("ts-orders");
    let (tree, id) = playground.block("src/routes/orders.ts#createOrder");
    let links = outgoing(&tree, id);
    assert!(links.iter().all(|link| link.kind == LinkKind::Calls));
    assert_eq!(
        links.iter().map(|link| &*link.text).collect::<Vec<_>>(),
        [
            "req.header",
            "ordersQueue.add",
            "res.status().json",
            "res.status",
            "orderService.place",
            "slack.post",
            "res.status().json",
            "res.status"
        ]
    );
    let place = outgoing(&tree, id)
        .into_iter()
        .find(|link| &*link.text == "orderService.place")
        .unwrap();
    assert_eq!(place.to.outcome, Outcome::Project);
    assert_eq!(
        place.to.key.as_ref().map(Key::as_str),
        Some("src/services/order-service.ts#OrderService.place")
    );
    let add = outgoing(&tree, id)
        .into_iter()
        .find(|link| &*link.text == "ordersQueue.add")
        .unwrap();
    assert_eq!(add.to.note.as_deref(), Some("bullmq"));
    // The module's own calls aren't the function's.
    let module = tree.id("src/routes/orders.ts#").unwrap();
    assert!(
        outgoing(&tree, module)
            .iter()
            .all(|link| link.call.as_str() != "src/routes/orders.ts#createOrder/call:slack.post")
    );
    assert!(
        outgoing(&tree, module)
            .iter()
            .any(|link| link.kind == LinkKind::Passes)
    );
}

#[test]
fn entrances_reach_what_their_code_runs() {
    let playground = Playground::load("ts-orders");
    let index = playground.index();
    let reached = |key: &str| -> Vec<String> {
        index
            .reached_from(key)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    let orders = [
        "entrance:server/place-an-order",
        "entrance:worker/process-an-order",
        "entrance:orders-cli/import-orders",
    ];
    assert_eq!(reached("src/lib/validate.ts#validateOrder"), orders);
    // A block stands for the definition it belongs to.
    assert_eq!(
        reached("src/services/order-service.ts#OrderService.place/call:validateOrder"),
        orders
    );
    assert_eq!(
        reached("src/routes/health.ts#/call:healthRouter.get(\"/live\")/callback[0]"),
        ["entrance:server/liveness"]
    );
    assert!(index.reached_from("src/absent.ts#").is_none());

    let py = Playground::load("py-orders");
    let index = py.index();
    let reached: Vec<String> = index
        .reached_from("src/orders_playground/lib/validate.py#validate_order")
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(reached, orders);
}

#[test]
fn source_is_shown_only_while_it_matches_the_facts() {
    let playground = Playground::load("py-orders");
    let path = "src/orders_playground/lib/validate.py";
    let file = playground.graph.file(path).unwrap();
    let verified = verify_file(&playground.root, &file.file.path, file.file.hash);
    let source = verified.source().expect("the playground matches its golden");
    let tree = playground.trees();
    let function = tree
        .block("src/orders_playground/lib/validate.py#validate_order")
        .unwrap();
    assert!(
        source
            .slice(function.node().range)
            .unwrap()
            .starts_with("def validate_order(value: object) -> OrderDraft:")
    );

    // A copy of the file, then an edit: the same facts no longer show it.
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join(path);
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, source.text()).unwrap();
    assert_eq!(
        verify_file(dir.path(), &file.file.path, file.file.hash).state(),
        SourceState::Current
    );
    std::fs::write(&copy, source.text().replace("nonempty", "non-empty")).unwrap();
    let changed = verify_file(dir.path(), &file.file.path, file.file.hash);
    assert_eq!(changed.state(), SourceState::Changed);
    assert!(changed.source().is_none());
}
