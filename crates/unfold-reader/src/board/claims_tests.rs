//! Claim tests: what resource cards, connections, and frame descriptions are signed over, their
//! health and repairs, and the grounding a save checks (for everyone, and for agents' claims),
//! on a small hand-built orders system (types, a queue, a route, a worker, a repository, and an evidence
//! file) whose sources are written to a temporary folder.

use std::path::Path;

use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, NodeRef, library, project, target, unknown};
use unfold_core::vocab::{Form, InputRole, Language, LinkKind as GraphLink, MemberKind, Reason, TargetState};
use unfold_core::{Graph, Key, Member, RepoPath};

use super::*;

const TYPES: &str = "export interface Draft {\n  customerId: string;\n}\nexport interface Order extends Draft {\n  id: string;\n}\nexport interface Job { order: Draft }\nexport function helper() {}\n";
const QUEUES: &str = "export const ordersQueue: Queue<Job> = new Queue(\"orders\");\n";
const ROUTES: &str = "export function createOrder(draft: Draft) {\n  ordersQueue.add(\"place\", draft);\n  repo.insert(draft);\n  charge(draft);\n}\nexport function charge(draft: Draft) {}\n";
const REPOSITORY: &str = "export class Repository {\n  private orders = new Map();\n}\n";
const WORKER: &str = "export const worker = new Worker(\"orders\", createOrder);\n";
const SERVER: &str = "app.post(\"/orders\", createOrder);\n";
const SCHEMA: &str = "create table orders (\n  id text primary key,\n  customer_id text\n);\n";

const DRAFT: &str = "types.ts#Draft";
const ORDER: &str = "types.ts#Order";
const JOB: &str = "types.ts#Job";
const HELPER: &str = "types.ts#helper";
const QUEUE: &str = "queues.ts#/call:Queue";
const CREATE: &str = "routes.ts#createOrder";
const ADD: &str = "routes.ts#createOrder/call:ordersQueue.add";
const INSERT: &str = "routes.ts#createOrder/call:repo.insert";
const CHARGE_CALL: &str = "routes.ts#createOrder/call:charge";
const CHARGE: &str = "routes.ts#charge";
const REPOSITORY_CLASS: &str = "repository.ts#Repository";
const SCHEMA_FILE: &str = "db/schema.sql";

/// A written property of a type or class, ranged from its name.
fn member(builder: &mut GraphBuilder, node: NodeRef, index: u32, name: &str) {
    let range = builder.find_in(node, name);
    builder.types(node).members.push(Member {
        node: node.local_id(),
        index,
        range,
        kind: MemberKind::Property,
        name: Some(name.into()),
        optional: false,
        readonly: false,
        default: false,
        ty: None,
        value: None,
    });
}

/// A `types` link from `from` naming the type written as `name` (at or after `from`'s start).
fn type_link(builder: &mut GraphBuilder, from: NodeRef, name: &str, target: &str) {
    let range = builder.find_in(from, name);
    builder.link(
        from,
        GraphLink::Types,
        range,
        name,
        project(&Key::from_text(target)),
    );
}

/// A call `call` of `parent` (its callee is the text before `(`) whose `calls` link reaches
/// `to`.
fn call(
    builder: &mut GraphBuilder,
    parent: NodeRef,
    call: &str,
    form: Form,
    to: unfold_core::LinkTarget,
) -> NodeRef {
    let range = builder.find_in(parent, call);
    let written = call.strip_prefix("new ").unwrap_or(call);
    let written = &written[..written.find('(').unwrap()];
    let mut parts = written.split('.');
    let mut callee = Callee::name(parts.next().unwrap());
    for part in parts {
        callee = callee.member(part);
    }
    let node = match form {
        Form::New => builder.construction(parent, &callee, range),
        _ => builder.call(parent, &callee, range),
    };
    let head = builder.find_in(node, written);
    builder.link(node, GraphLink::Calls, head, written, to);
    node
}

/// The orders system's graph, with its `schema.sql` (an evidence file) as `schema`.
fn orders_graph(schema: &str) -> Graph {
    let mut g = GraphBuilder::new();
    let types = g.file("types.ts", Language::TypeScript, TYPES);
    let draft = g.def(
        types,
        Form::Interface,
        "Draft",
        g.find(types, "interface Draft {\n  customerId: string;\n}"),
    );
    member(&mut g, draft, 0, "customerId");
    let order = g.def(
        types,
        Form::Interface,
        "Order",
        g.find(types, "interface Order extends Draft {\n  id: string;\n}"),
    );
    member(&mut g, order, 0, "id");
    let base = g.find_in(order, "Draft");
    g.link(
        order,
        GraphLink::Extends,
        base,
        "Draft",
        project(&Key::from_text(DRAFT)),
    );
    let job = g.def(
        types,
        Form::Interface,
        "Job",
        g.find(types, "interface Job { order: Draft }"),
    );
    member(&mut g, job, 0, "order");
    g.def(
        types,
        Form::Function,
        "helper",
        g.find(types, "function helper() {}"),
    );

    let queues = g.file("queues.ts", Language::TypeScript, QUEUES);
    type_link(&mut g, queues, "Job", JOB);
    call(
        &mut g,
        queues,
        "new Queue(\"orders\")",
        Form::New,
        library("bullmq"),
    );

    let routes = g.file("routes.ts", Language::TypeScript, ROUTES);
    let create = g.def(
        routes,
        Form::Function,
        "createOrder",
        g.find(routes, &ROUTES[7..ROUTES.find("}\n").unwrap() + 1]),
    );
    type_link(&mut g, create, "Draft", DRAFT);
    call(
        &mut g,
        create,
        "ordersQueue.add(\"place\", draft)",
        Form::Call,
        library("bullmq"),
    );
    call(
        &mut g,
        create,
        "repo.insert(draft)",
        Form::Call,
        unknown(Reason::UnknownReceiver),
    );
    call(
        &mut g,
        create,
        "charge(draft)",
        Form::Call,
        project(&Key::from_text(CHARGE)),
    );
    let charge = g.def(
        routes,
        Form::Function,
        "charge",
        g.find(routes, "function charge(draft: Draft) {}"),
    );
    type_link(&mut g, charge, "Draft", DRAFT);

    let repository = g.file("repository.ts", Language::TypeScript, REPOSITORY);
    let class = g.def(
        repository,
        Form::Class,
        "Repository",
        g.find(repository, &REPOSITORY[7..REPOSITORY.len() - 1]),
    );
    call(&mut g, class, "new Map()", Form::New, library("global"));

    let worker = g.file("worker.ts", Language::TypeScript, WORKER);
    let construction = call(
        &mut g,
        worker,
        "new Worker(\"orders\", createOrder)",
        Form::New,
        library("bullmq"),
    );
    let passed = g.find_in(construction, "createOrder");
    g.link(
        construction,
        GraphLink::Passes,
        passed,
        "createOrder",
        project(&Key::from_text(CREATE)),
    );

    let server = g.file("server.ts", Language::TypeScript, SERVER);
    let post = call(
        &mut g,
        server,
        "app.post(\"/orders\", createOrder)",
        Form::Call,
        unknown(Reason::UnknownName),
    );
    let passed = g.find_in(post, "createOrder");
    g.link(
        post,
        GraphLink::Passes,
        passed,
        "createOrder",
        project(&Key::from_text(CREATE)),
    );

    let app = g.app("Orders", Some("npm start"), None);
    g.entrance(
        app,
        "Worker",
        Some("orders"),
        vec![target(CREATE, TargetState::Found)],
    );
    g.entrance(
        app,
        "Route",
        Some("POST /orders"),
        vec![target(CREATE, TargetState::Found)],
    );

    g.input(SCHEMA_FILE, InputRole::Evidence, schema);
    g.build()
}

/// The key of the entrance named `name` in `graph`.
fn entrance_key(graph: &Graph, name: &str) -> String {
    let entrance = graph
        .apps
        .iter()
        .flat_map(|app| &app.entrances)
        .find(|entrance| &*entrance.name == name)
        .unwrap();
    entrance.key.to_string()
}

/// A folder holding the orders system's sources (with `schema` as the evidence file), and the
/// board graph of it.
fn orders(schema: &str) -> (tempfile::TempDir, Graph, BoardGraph) {
    let dir = tempfile::tempdir().unwrap();
    let graph = orders_graph(schema);
    write_sources(dir.path(), schema);
    let board_graph = BoardGraph::from_graph(&graph, dir.path());
    (dir, graph, board_graph)
}

fn write_sources(dir: &Path, schema: &str) {
    for (path, text) in [
        ("types.ts", TYPES),
        ("queues.ts", QUEUES),
        ("routes.ts", ROUTES),
        ("repository.ts", REPOSITORY),
        ("worker.ts", WORKER),
        ("server.ts", SERVER),
        (SCHEMA_FILE, schema),
    ] {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
}

fn agent() -> Author {
    Author::agent("claude").unwrap()
}

fn person() -> Author {
    Author::person("tester").unwrap()
}

fn text(words: &str) -> Description {
    Description {
        text: words.to_owned(),
        author: agent(),
        signature: None,
    }
}

fn key(key: &str) -> Anchor {
    Anchor::Key {
        key: Key::from_text(key),
    }
}

fn lines(first: u32, last: u32) -> Anchor {
    Anchor::File {
        file: RepoPath::new(SCHEMA_FILE).unwrap(),
        lines: [first, last],
    }
}

fn card(id: &str, reference: CardRef) -> Card {
    Card {
        id: id.to_owned(),
        reference,
        at: [0.0, 0.0],
        view: None,
        description: None,
        open: Vec::new(),
    }
}

fn block_card(id: &str, key: &str) -> Card {
    card(id, CardRef::block(Key::from_text(key)))
}

fn start_card(id: &str, key: &str) -> Card {
    card(id, CardRef::entrance(Key::from_text(key)))
}

fn resource(id: &str, kind: ResourceKind, name: &str, anchors: Vec<Anchor>) -> Resource {
    Resource {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        type_key: None,
        columns: Vec::new(),
        anchors,
        at: [400.0, 0.0],
        description: None,
        author: agent(),
        signature: None,
    }
}

fn table(type_key: &str, marks: &[&str], anchors: Vec<Anchor>) -> Resource {
    Resource {
        type_key: Some(Key::from_text(type_key)),
        columns: marks.iter().map(|name| column(name)).collect(),
        ..resource("orders", ResourceKind::Table, "orders", anchors)
    }
}

fn column(name: &str) -> Column {
    Column {
        name: name.to_owned(),
        written_type: None,
        key: None,
        references: None,
        optional: false,
    }
}

fn connection(id: &str, kind: ConnectionKind, from: &str, to: &str) -> Connection {
    Connection::new(id, kind, from, to, agent())
}

/// A board whose one page shows `createOrder`, `charge`, the worker, and the route.
fn board(graph: &Graph) -> Board {
    let mut board = Board::new("Orders");
    let page = &mut board.pages[0];
    page.cards.push(block_card("create", CREATE));
    page.cards.push(block_card("charge", CHARGE));
    page.cards
        .push(start_card("worker", &entrance_key(graph, "Worker")));
    page.cards
        .push(start_card("route", &entrance_key(graph, "Route")));
    board
}

fn save(
    board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    prepare_save(board, previous, graph)
}

/// The message a save refuses `board` with.
fn refusal(board: Board, graph: &BoardGraph) -> String {
    match save(board, None, graph).unwrap_err() {
        BoardError::Problem { message, .. } => message,
        other => panic!("not a board problem: {other}"),
    }
}

fn bytes_of(source: &str, text: &str) -> String {
    let start = source.find(text).unwrap();
    source[start..start + text.len()].to_owned()
}

#[test]
fn claims_are_signed_over_the_code_they_cite() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    let mut orders = table(ORDER, &["id"], vec![key(REPOSITORY_CLASS), lines(1, 2)]);
    orders.description = Some(text("Orders by id"));
    page.resources.push(orders);
    let mut enqueue = Connection {
        step: Some(Key::from_text(ADD)),
        anchors: vec![key(QUEUE)],
        description: Some(text("Queues the order")),
        label: Some(text("place job")),
        ..connection("enqueue", ConnectionKind::Queue, "create", "worker")
    };
    enqueue.channel = Some("typed by hand".to_owned());
    page.links.push(enqueue);
    page.frames.push(Frame {
        id: "api".to_owned(),
        title: "API".to_owned(),
        description: Some(text("Takes orders")),
        members: vec!["route".to_owned(), "create".to_owned()],
        look: FrameLook::Box,
        icon: None,
        direction: None,
        collapsed: false,
        author: agent(),
    });
    let (saved, health) = save(board, None, &loaded).unwrap();
    assert!(health.is_healthy(), "{:#?}", health.items);
    let page = &saved.pages[0];

    // A table: its type's range, then each anchor's bytes (an evidence file's lines with their
    // line breaks), each followed by a line break. Its description is signed the same.
    let class = &REPOSITORY[7..REPOSITORY.len() - 1];
    let order = "interface Order extends Draft {\n  id: string;\n}";
    let resource = Signature::of(
        format!("{order}\n{class}\ncreate table orders (\n  id text primary key,\n\n").as_bytes(),
    );
    assert_eq!(page.resources[0].signature, Some(resource));
    assert_eq!(
        page.resources[0].description.as_ref().unwrap().signature,
        Some(resource)
    );

    // A connection: its step's range, then its anchors'. Its texts are signed the same, and an
    // entrance at its `to` end gives its channel.
    let step = bytes_of(ROUTES, "ordersQueue.add(\"place\", draft)");
    let queue = bytes_of(QUEUES, "new Queue(\"orders\")");
    let signature = Signature::of(format!("{step}\n{queue}\n").as_bytes());
    let enqueue = page.connection("enqueue").unwrap();
    assert_eq!(enqueue.signature, Some(signature));
    assert_eq!(enqueue.description.as_ref().unwrap().signature, Some(signature));
    assert_eq!(enqueue.label.as_ref().unwrap().signature, Some(signature));
    assert_eq!(enqueue.channel.as_deref(), Some("orders"));

    // A frame's description: its member cards' signatures, by card id, joined with line breaks.
    let members = [CREATE, &entrance_key(&graph, "Route")]
        .map(|key| loaded.signature(key).unwrap().0.to_string())
        .join("\n");
    assert_eq!(
        page.frames[0].description.as_ref().unwrap().signature,
        Some(Signature::of(members.as_bytes()))
    );

    // Every cited key is guarded, so it reconnects like a card's.
    for cited in [ORDER, REPOSITORY_CLASS, ADD, QUEUE] {
        assert!(page.guard(cited).is_some(), "{cited} is guarded");
    }
    let references = saved.references();
    assert_eq!(references.tables, [ORDER]);
    assert_eq!(references.files, [&RepoPath::new(SCHEMA_FILE).unwrap()]);
}

#[test]
fn evidence_anchors_go_outdated_when_their_file_changes() {
    let (dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let mut orders = resource("orders", ResourceKind::Table, "orders", vec![lines(1, 3)]);
    orders.columns = vec![column("id"), column("customer_id")];
    board.pages[0].resources.push(orders);
    let (saved, health) = save(board, None, &loaded).unwrap();
    assert!(health.is_healthy());
    assert!(saved.pages[0].resources[0].signature.is_some());

    // The file changes on disk: its bytes no longer verify, so the table is outdated.
    let changed = SCHEMA.replace("customer_id text", "customer_id uuid");
    std::fs::write(dir.path().join(SCHEMA_FILE), &changed).unwrap();
    let stale = BoardGraph::from_graph(&graph, dir.path());
    let health = board_health(&saved, &stale);
    let outdated = &health.items[0];
    assert_eq!(
        (outdated.kind, &outdated.target, outdated.message.as_str()),
        (
            HealthKind::Outdated,
            &Target::Resource("orders".to_owned()),
            "Code changed since this resource card was drawn"
        )
    );
    assert_eq!(outdated.repairs, [RepairKind::Resign, RepairKind::Remove]);
    assert_eq!(health.label().as_deref(), Some("1 outdated"));

    // After a compile records the new bytes, re-signing accepts them.
    let recompiled = BoardGraph::from_graph(&orders_graph(&changed), dir.path());
    let mut page = saved.pages[0].clone();
    apply_repair(
        &mut page,
        &recompiled,
        &Repair::Resign(Target::Resource("orders".to_owned())),
    )
    .unwrap();
    assert!(health_of(&page, &recompiled).is_healthy());

    // Lines the file doesn't have, and a file the graph doesn't record, are missing.
    page.resources[0].anchors = vec![
        lines(4, 9),
        Anchor::File {
            file: RepoPath::new("db/other.sql").unwrap(),
            lines: [1, 1],
        },
    ];
    let health = health_of(&page, &recompiled);
    let messages: Vec<(&str, Option<u32>)> = health
        .items
        .iter()
        .map(|item| (item.message.as_str(), item.anchor))
        .collect();
    assert_eq!(
        messages,
        [
            (
                "Anchor db/schema.sql has no lines 4–9: the file got shorter. Reconnect the anchor to the lines it means now",
                Some(0)
            ),
            (
                "Anchor db/other.sql is not an evidence file of the project: list it under `[evidence] files` in unfold.toml and compile, or reconnect the anchor",
                Some(1)
            ),
        ]
    );
    assert!(health.items.iter().all(|item| item.kind == HealthKind::Missing));
}

/// A health item's kind, target, key, anchor, and repairs.
type Row<'h> = (
    HealthKind,
    &'h Target,
    Option<&'h str>,
    Option<u32>,
    &'h [RepairKind],
);

fn health_of(page: &Page, graph: &BoardGraph) -> Health {
    health(page, graph)
}

#[test]
fn claims_go_missing_broken_or_held_when_their_code_changes() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.resources
        .push(table("types.ts#Gone", &[], vec![key("queues.ts#gone")]));
    page.links.push(Connection {
        step: Some(Key::from_text("routes.ts#createOrder/call:gone")),
        carries: Some(Key::from_text(HELPER)),
        anchors: vec![key(QUEUE), key("queues.ts#gone")],
        ..connection("enqueue", ConnectionKind::Queue, "create", "worker")
    });
    let page = &board.pages[0];
    let health = health_of(page, &loaded);
    let items: Vec<Row<'_>> = health
        .items
        .iter()
        .map(|item| {
            (
                item.kind,
                &item.target,
                item.key.as_deref(),
                item.anchor,
                item.repairs.as_slice(),
            )
        })
        .collect();
    let table = Target::Resource("orders".to_owned());
    let link = Target::Connection("enqueue".to_owned());
    let missing = [RepairKind::Reconnect, RepairKind::Remove];
    let broken = missing;
    assert_eq!(
        items,
        [
            (
                HealthKind::Missing,
                &table,
                Some("types.ts#Gone"),
                None,
                &missing[..]
            ),
            (
                HealthKind::Missing,
                &table,
                Some("queues.ts#gone"),
                Some(0),
                &missing[..]
            ),
            (
                HealthKind::BrokenLink,
                &link,
                Some("routes.ts#createOrder/call:gone"),
                None,
                &broken[..]
            ),
            (HealthKind::BrokenLink, &link, Some(HELPER), None, &broken[..]),
            (
                HealthKind::BrokenLink,
                &link,
                Some("queues.ts#gone"),
                Some(1),
                &broken[..]
            ),
        ]
    );
    assert_eq!(
        health.items[3].message,
        format!(
            "{HELPER} is not a type declaration or class: the carried type is the interface, type, enum, or class the code writes"
        )
    );
    assert_eq!(health.label().as_deref(), Some("2 missing · 3 broken links"));
    // A save refuses them as new problems, saying what to do.
    assert!(refusal(board.clone(), &loaded).starts_with("Type types.ts#Gone is not in the graph"));

    // A cited key whose kind changed is held for review, like a card's.
    let mut held = board.clone();
    let page = &mut held.pages[0];
    page.resources.clear();
    page.links.clear();
    page.resources
        .push(resource("queue", ResourceKind::Queue, "orders", vec![key(QUEUE)]));
    let (saved, _) = save(held, None, &loaded).unwrap();
    let mut page = saved.pages[0].clone();
    page.guards
        .iter_mut()
        .filter(|guard| guard.key.as_str() == QUEUE)
        .for_each(|guard| guard.kind = "function".to_owned());
    let health = health_of(&page, &loaded);
    assert_eq!(
        (
            health.items[0].kind,
            health.items[0].anchor,
            health.items[0].repairs.as_slice()
        ),
        (
            HealthKind::Review,
            Some(0),
            &[RepairKind::Accept, RepairKind::Reconnect, RepairKind::Remove][..]
        )
    );
}

#[test]
fn reconnecting_moves_what_is_keyed_inside_and_each_other_key_on_its_own() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.cards[0] = block_card("create", "routes.ts#placeOrder");
    page.links.push(Connection {
        step: Some(Key::from_text("routes.ts#placeOrder/call:ordersQueue.add")),
        anchors: vec![key("queues.ts#queue")],
        ..connection("enqueue", ConnectionKind::Queue, "create", "worker")
    });
    page.resources.push(resource(
        "queue",
        ResourceKind::Queue,
        "orders",
        vec![key("queues.ts#queue")],
    ));
    let mut page = board.pages[0].clone();
    let reconnect = |from: &str, to: &str| Repair::Reconnect {
        from: from.to_owned(),
        to: to.to_owned(),
    };
    apply_repair(&mut page, &loaded, &reconnect("routes.ts#placeOrder", CREATE)).unwrap();
    // The card moves, and the step inside the old key with it.
    assert_eq!(page.cards[0].reference, CardRef::block(Key::from_text(CREATE)));
    assert_eq!(
        page.connection("enqueue").unwrap().step.as_ref().map(Key::as_str),
        Some(ADD)
    );
    let health = health_of(&page, &loaded);
    assert!(health.items.iter().all(|item| !item.message.starts_with("Step")));
    // Reconnecting an anchor's key points every claim citing it at the new key.
    apply_repair(&mut page, &loaded, &reconnect("queues.ts#queue", QUEUE)).unwrap();
    assert_eq!(page.connection("enqueue").unwrap().anchors, [key(QUEUE)]);
    assert_eq!(page.resource("queue").unwrap().anchors, [key(QUEUE)]);
    assert!(health_of(&page, &loaded).is_healthy());
    // A key that isn't in the graph is refused, naming the claim that cites it.
    assert_eq!(
        apply_repair(&mut page, &loaded, &reconnect(QUEUE, "queues.ts#nowhere")).unwrap_err(),
        BoardError::Problem {
            page: "page-1".to_owned(),
            target: Target::Resource("queue".to_owned()),
            message: "the new reference is not in the graph or is a different kind of reference".to_owned()
        }
    );
}

#[test]
fn claims_are_repaired_by_anchor_resigned_or_removed() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.resources.push(resource(
        "schema",
        ResourceKind::Table,
        "orders",
        vec![lines(1, 1)],
    ));
    page.links.push(Connection {
        anchors: vec![key(QUEUE)],
        description: Some(text("Queues the order")),
        label: Some(text("job")),
        ..connection("enqueue", ConnectionKind::Queue, "create", "schema")
    });
    page.notes.push(Note {
        id: "note".to_owned(),
        text: "Read me".to_owned(),
        at: [0.0, 10.0],
        attach: Some("schema".to_owned()),
        author: person(),
    });
    let (saved, _) = save(board, None, &loaded).unwrap();
    let mut page = saved.pages[0].clone();

    // An evidence anchor is reconnected by its position.
    let anchor = |to: Anchor| Repair::ReconnectAnchor {
        item: "schema".to_owned(),
        index: 0,
        to,
    };
    apply_repair(&mut page, &loaded, &anchor(lines(1, 2))).unwrap();
    assert_eq!(page.resource("schema").unwrap().anchors, [lines(1, 2)]);
    assert!(matches!(
        apply_repair(&mut page, &loaded, &anchor(lines(9, 9))),
        Err(BoardError::Problem { .. })
    ));
    // Re-signing signs the claim and its texts over its code now.
    page.links[0].signature = Some(Signature::of(b"old"));
    apply_repair(
        &mut page,
        &loaded,
        &Repair::Resign(Target::Connection("enqueue".to_owned())),
    )
    .unwrap();
    let signature = Signature::of(format!("{}\n", bytes_of(QUEUES, "new Queue(\"orders\")")).as_bytes());
    let link = page.connection("enqueue").unwrap();
    assert_eq!(
        [link.signature, link.label.as_ref().unwrap().signature],
        [Some(signature); 2]
    );
    // A connection's description repair removes its description; its label is the line's.
    apply_repair(
        &mut page,
        &loaded,
        &Repair::RemoveDescription(Target::Connection("enqueue".to_owned())),
    )
    .unwrap();
    let link = page.connection("enqueue").unwrap();
    assert!(link.description.is_none() && link.label.is_some());
    // Removing a resource card removes the connections to it; its note stays where it was.
    apply_repair(
        &mut page,
        &loaded,
        &Repair::Remove(Target::Resource("schema".to_owned())),
    )
    .unwrap();
    assert!(page.resources.is_empty() && page.links.is_empty());
    let note = page.note("note").unwrap();
    assert_eq!((note.attach.as_deref(), note.at), (None, [400.0, 10.0]));
}

#[test]
fn agents_anchor_resource_cards_and_requests() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let with = |resource: Resource| {
        let mut board = board(&graph);
        board.pages[0].resources.push(resource);
        board
    };
    assert_eq!(
        refusal(
            with(resource("queue", ResourceKind::Queue, "orders", Vec::new())),
            &loaded
        ),
        "Can't add the queue \"orders\": it has no anchors. Anchor it to the code that defines or uses it (a block, app, or entrance key), or to lines of an evidence file"
    );
    // An actor needs none, and neither does a person's resource card.
    save(
        with(resource("customer", ResourceKind::Actor, "Customer", Vec::new())),
        None,
        &loaded,
    )
    .unwrap();
    let mut theirs = resource("queue", ResourceKind::Queue, "anything", Vec::new());
    theirs.author = person();
    save(with(theirs), None, &loaded).unwrap();

    let link = |link: Connection| {
        let mut board = board(&graph);
        board.pages[0].links.push(link);
        board
    };
    assert_eq!(
        refusal(
            link(connection("enqueue", ConnectionKind::Queue, "create", "worker")),
            &loaded
        ),
        "Can't connect createOrder to Worker (queue): it has no anchors. A queue connection cites its receiving side: the entrance, the handler's registration, or where the queue is created"
    );
    assert_eq!(
        refusal(
            link(Connection {
                step: Some(Key::from_text(INSERT)),
                ..connection("runtime", ConnectionKind::Calls, "create", "charge")
            }),
            &loaded
        ),
        "Can't connect createOrder to charge (calls): it has no anchors. A calls connection cites why the call goes where it does, such as the factory or the injection"
    );
    let anchored = Connection {
        anchors: vec![key(&entrance_key(&graph, "Worker"))],
        ..connection("enqueue", ConnectionKind::Queue, "create", "worker")
    };
    save(link(anchored), None, &loaded).unwrap();
    // A `writes` or `reads` connection cites its step: the call that writes or reads.
    assert_eq!(
        refusal(
            link(connection("writes", ConnectionKind::Writes, "create", "charge")),
            &loaded
        ),
        "Can't connect createOrder to charge (writes): it cites nothing. A writes connection leaves from the call that writes as its step; give that call, or an anchor"
    );
    save(
        link(Connection {
            step: Some(Key::from_text(INSERT)),
            ..connection("writes", ConnectionKind::Reads, "create", "charge")
        }),
        None,
        &loaded,
    )
    .unwrap();
    // An `other` connection needs no citation, and people's connections need none.
    save(
        link(Connection {
            label: Some(text("posts to Slack")),
            ..connection("other", ConnectionKind::Other, "create", "charge")
        }),
        None,
        &loaded,
    )
    .unwrap();
    save(
        link(Connection {
            author: person(),
            ..connection("writes", ConnectionKind::Writes, "create", "charge")
        }),
        None,
        &loaded,
    )
    .unwrap();
}

#[test]
fn names_columns_and_channels_appear_in_the_cited_code() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let with = |resource: Resource| {
        let mut board = board(&graph);
        board.pages[0].resources.push(resource);
        board
    };
    save(
        with(resource("queue", ResourceKind::Queue, "orders", vec![key(QUEUE)])),
        None,
        &loaded,
    )
    .unwrap();
    assert_eq!(
        refusal(
            with(resource(
                "queue",
                ResourceKind::Queue,
                "payments",
                vec![key(QUEUE)]
            )),
            &loaded
        ),
        "Can't add the queue \"payments\": \"payments\" doesn't appear in its anchors (Queue in queues.ts). Use the name the code writes, or anchor it where the name is written"
    );
    let mut listed = resource("orders", ResourceKind::Table, "orders", vec![lines(1, 4)]);
    listed.columns = vec![column("id"), column("customer_id")];
    save(with(listed.clone()), None, &loaded).unwrap();
    listed.columns.push(column("email"));
    listed.anchors.push(key(REPOSITORY_CLASS));
    assert_eq!(
        refusal(with(listed), &loaded),
        "Can't add the table \"orders\": the column \"email\" doesn't appear in its anchors (db/schema.sql lines 1–4 and Repository in repository.ts). Cite where the column is defined, or leave it out"
    );

    let link = |channel: &str, to: &str| {
        let mut board = board(&graph);
        board.pages[0]
            .resources
            .push(resource("queue", ResourceKind::Queue, "orders", vec![key(QUEUE)]));
        board.pages[0].links.push(Connection {
            step: Some(Key::from_text(ADD)),
            anchors: vec![key(QUEUE)],
            channel: Some(channel.to_owned()),
            ..connection("enqueue", ConnectionKind::Queue, "create", to)
        });
        board
    };
    save(link("orders", "queue"), None, &loaded).unwrap();
    assert_eq!(
        refusal(link("emails", "queue"), &loaded),
        "Can't connect createOrder to the queue \"orders\" (queue): the channel \"emails\" doesn't appear in its step or anchors (ordersQueue.add in routes.ts and Queue in queues.ts). Anchor it where the channel is named, or leave the channel out"
    );
    // An entrance at the `to` end gives the channel, whatever was typed.
    let (saved, _) = save(link("emails", "worker"), None, &loaded).unwrap();
    assert_eq!(
        saved.pages[0].connection("enqueue").unwrap().channel.as_deref(),
        Some("orders")
    );
    let mut route = board(&graph);
    route.pages[0]
        .resources
        .push(resource("customer", ResourceKind::Actor, "Customer", Vec::new()));
    route.pages[0].links.push(Connection {
        anchors: vec![key(&entrance_key(&graph, "Route"))],
        ..connection("posts", ConnectionKind::Http, "customer", "route")
    });
    let (saved, _) = save(route, None, &loaded).unwrap();
    assert_eq!(
        saved.pages[0].connection("posts").unwrap().channel.as_deref(),
        Some("POST /orders")
    );
}

#[test]
fn tables_backed_by_a_type_mark_its_fields() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let with = |orders: Resource, link: Option<Connection>| {
        let mut board = board(&graph);
        let page = &mut board.pages[0];
        page.resources.push(orders);
        let mut customers = resource("customers", ResourceKind::Table, "customers", vec![lines(1, 4)]);
        customers.columns = vec![column("id")];
        customers.author = person();
        page.resources.push(customers);
        page.links.extend(link);
        board
    };
    // Its own fields and the ones it inherits.
    let orders = table(ORDER, &["id", "customerId"], vec![key(REPOSITORY_CLASS)]);
    save(with(orders.clone(), None), None, &loaded).unwrap();
    let mut wrong = table(ORDER, &["customer_id"], vec![key(REPOSITORY_CLASS)]);
    assert_eq!(
        refusal(with(wrong.clone(), None), &loaded),
        "Can't add the table \"orders\": \"customer_id\" isn't a field of Order (types.ts). A table backed by a type marks its fields; mark one of: id, customerId"
    );
    wrong.author = person();
    assert!(
        refusal(with(wrong, None), &loaded).contains("isn't a field of Order"),
        "a person's marks name fields too"
    );
    // A relationship joins columns its tables have.
    let relates = |column: &str| Connection {
        from_column: Some(column.to_owned()),
        to_column: Some("id".to_owned()),
        cardinality: Some(Cardinality::ManyToOne),
        ..connection("customer", ConnectionKind::Relates, "orders", "customers")
    };
    save(with(orders.clone(), Some(relates("customerId"))), None, &loaded).unwrap();
    assert_eq!(
        refusal(with(orders, Some(relates("customer"))), &loaded),
        "Can't connect the table \"orders\" to the table \"customers\" (relates): \"customer\" isn't a column of the table \"orders\", whose columns are the fields of Order (types.ts). Use one of: id, customerId"
    );
}

#[test]
fn carried_types_are_used_where_the_connection_leaves_or_in_its_anchors() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let carrying = |carries: &str, step: Option<&str>| {
        let mut board = board(&graph);
        board.pages[0].links.push(Connection {
            step: step.map(Key::from_text),
            anchors: vec![key("queues.ts#")],
            carries: Some(Key::from_text(carries)),
            ..connection("enqueue", ConnectionKind::Queue, "create", "worker")
        });
        board
    };
    // `Job` is written in the anchor (the queues module); `Draft` in the step's function.
    save(carrying(JOB, None), None, &loaded).unwrap();
    save(carrying(DRAFT, Some(ADD)), None, &loaded).unwrap();
    assert_eq!(
        refusal(carrying(DRAFT, None), &loaded),
        "Can't connect createOrder to Worker (queue): Draft (types.ts) isn't used in its step's function or in its anchors. Carry a type the sending code or an anchor writes, or leave carries out"
    );
    assert_eq!(
        refusal(carrying(ORDER, Some(ADD)), &loaded),
        "Can't connect createOrder to Worker (queue): Order (types.ts) isn't used in createOrder or in its anchors. Carry a type the sending code or an anchor writes, or leave carries out"
    );
}

#[test]
fn steps_are_calls_and_runtime_calls_start_at_unknown_calls() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let calling = |step: Option<&str>, author: Author| {
        let mut board = board(&graph);
        board.pages[0].links.push(Connection {
            step: step.map(Key::from_text),
            anchors: vec![key(CHARGE)],
            author,
            ..connection("runtime", ConnectionKind::Calls, "create", "charge")
        });
        board
    };
    // An unknown call is what a runtime call explains.
    save(calling(Some(INSERT), agent()), None, &loaded).unwrap();
    // A resolved or library call isn't, whoever draws it.
    for author in [agent(), person()] {
        assert_eq!(
            refusal(calling(Some(CHARGE_CALL), author.clone()), &loaded),
            "Can't connect createOrder to charge (calls): the call charge (line 4) resolves to charge, so Unfold already draws it. A calls connection starts at a call Unfold can't follow"
        );
        assert_eq!(
            refusal(calling(Some(ADD), author.clone()), &loaded),
            "Can't connect createOrder to charge (calls): the call ordersQueue.add (line 2) is a library call, not an unknown one. A calls connection starts at a call Unfold can't follow"
        );
        assert_eq!(
            refusal(calling(None, author), &loaded),
            "Can't connect createOrder to charge (calls): it has no step. A calls connection leaves from the unknown call itself; give that call as its step"
        );
    }
    // Any connection's step is a call.
    let mut writes = board(&graph);
    writes.pages[0].links.push(Connection {
        step: Some(Key::from_text(CREATE)),
        author: person(),
        ..connection("writes", ConnectionKind::Writes, "create", "charge")
    });
    assert_eq!(
        refusal(writes, &loaded),
        "Can't connect createOrder to charge (writes): its step (line 1) is a function, not a call. A connection leaves from a call: give the call's key as the step"
    );
}

#[test]
fn only_new_or_changed_claims_are_grounded() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let mut ungrounded = resource("queue", ResourceKind::Queue, "payments", vec![key(QUEUE)]);
    ungrounded.author = person();
    board.pages[0].resources.push(ungrounded);
    let (saved, health) = save(board, None, &loaded).unwrap();
    // An agent takes the card over (it becomes theirs): a change, grounded.
    let mut agents = saved;
    agents.pages[0].resources[0].author = agent();
    assert!(refusal(agents.clone(), &loaded).contains("\"payments\" doesn't appear in its anchors"));
    // Moved or described, an agent's claim saved before isn't grounded again.
    let mut moved = agents.clone();
    moved.pages[0].resources[0].at = [0.0, 0.0];
    moved.pages[0].resources[0].description = Some(text("Payments"));
    save(moved, Some((&agents, &health)), &loaded).unwrap();
}
