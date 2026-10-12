//! Claim tests: what resource cards, connections, and frame descriptions are signed over, their
//! health and repairs, and the grounding a save checks for agent-authored claims, on a small
//! hand-built orders system (types, a queue, a route, a worker, a repository, and an evidence
//! file) whose sources are written to a temporary folder.

use std::path::Path;

use unfold_core::testkit::{BlockRef, GraphBuilder, Outcome};
use unfold_core::vocab::{BlockKind, DetectorId, EdgeKind, EntranceKind, Heritage, HttpMethod, InputRole};
use unfold_core::vocab::{Language, MemberKind, Reason, Role};
use unfold_core::{Graph, Key, Member, RepoPath};

use super::*;

const TYPES: &str = "export interface Draft {\n  customerId: string;\n}\nexport interface Order extends Draft {\n  id: string;\n}\nexport interface Job { order: Draft }\nexport function helper() {}\n";
const QUEUES: &str = "export const ordersQueue: Queue<Job> = new Queue(\"orders\");\n";
const ROUTES: &str = "export function createOrder(draft: Draft) {\n  ordersQueue.add(\"place\", draft);\n  repo.insert(draft);\n  charge(draft);\n}\nexport function charge(draft: Draft) {}\n";
const REPOSITORY: &str = "export class Repository {\n  private orders = new Map();\n}\n";
const WORKER: &str = "export const worker = new Worker(\"orders\", createOrder);\n";
const SERVER: &str = "app.post(\"/orders\", createOrder);\n";
const SCHEMA: &str = "create table orders (\n  id text primary key,\n  customer_id text\n);\n";

const DRAFT: &str = "types.ts#interface:Draft";
const ORDER: &str = "types.ts#interface:Order";
const JOB: &str = "types.ts#interface:Job";
const HELPER: &str = "types.ts#function:helper";
const QUEUE: &str = "queues.ts#variable:ordersQueue";
const CREATE: &str = "routes.ts#function:createOrder";
const ADD: &str = "routes.ts#function:createOrder/expression_statement[0]/call[0]";
const INSERT: &str = "routes.ts#function:createOrder/expression_statement[1]/call[0]";
const CHARGE_CALL: &str = "routes.ts#function:createOrder/expression_statement[2]/call[0]";
const CHARGE: &str = "routes.ts#function:charge";
const REPOSITORY_CLASS: &str = "repository.ts#class:Repository";
const SCHEMA_FILE: &str = "db/schema.sql";

/// A written property of a type declaration.
fn member(builder: &mut GraphBuilder, block: BlockRef, index: u32, name: &str) {
    builder.types(block).members.push(Member {
        block: block.local_id(),
        index,
        kind: MemberKind::Property,
        name: Some(name.into()),
        optional: false,
        readonly: false,
        default: false,
        ty: None,
        value: None,
    });
}

/// A top-level block whose range is `text`'s first occurrence in its file.
fn declare(
    builder: &mut GraphBuilder,
    parent: BlockRef,
    kind: BlockKind,
    name: Option<&str>,
    text: &str,
) -> BlockRef {
    let range = builder.find(parent, text);
    let role = if kind == BlockKind::New {
        Role::Initializer
    } else {
        Role::Body
    };
    builder.block(parent, kind, Some(role), name, range)
}

/// A statement `call;` of `function` whose call's occurrence edge has `outcome`.
fn call(builder: &mut GraphBuilder, function: BlockRef, call: &str, outcome: Outcome) {
    let statement = declare(
        builder,
        function,
        BlockKind::ExpressionStatement,
        None,
        &format!("{call};"),
    );
    let range = builder.find(statement, call);
    let block = builder.block(statement, BlockKind::Call, Some(Role::Expression), None, range);
    let callee = &call[..call.find('(').unwrap()];
    builder.edge(block, EdgeKind::Calls, outcome, callee, None);
}

/// The orders system's graph, with its `schema.sql` (an evidence file) as `schema`.
fn orders_graph(schema: &str) -> Graph {
    let mut g = GraphBuilder::new();
    let types = g.file("types.ts", Language::TypeScript, TYPES);
    let draft = declare(
        &mut g,
        types,
        BlockKind::Interface,
        Some("Draft"),
        "interface Draft {\n  customerId: string;\n}",
    );
    member(&mut g, draft, 0, "customerId");
    let order = declare(
        &mut g,
        types,
        BlockKind::Interface,
        Some("Order"),
        "interface Order extends Draft {\n  id: string;\n}",
    );
    member(&mut g, order, 0, "id");
    let from = g.range(order).start;
    g.heritage_link(
        order,
        "Draft",
        from,
        Outcome::Resolved(Key::from_text(DRAFT)),
        Heritage::Extends,
    );
    let job = declare(
        &mut g,
        types,
        BlockKind::Interface,
        Some("Job"),
        "interface Job { order: Draft }",
    );
    member(&mut g, job, 0, "order");
    declare(
        &mut g,
        types,
        BlockKind::Function,
        Some("helper"),
        "function helper() {}",
    );

    let queues = g.file("queues.ts", Language::TypeScript, QUEUES);
    let queue = declare(
        &mut g,
        queues,
        BlockKind::Variable,
        Some("ordersQueue"),
        "ordersQueue: Queue<Job> = new Queue(\"orders\")",
    );
    g.type_link(queue, "Job", 0, Outcome::Resolved(Key::from_text(JOB)));

    let routes = g.file("routes.ts", Language::TypeScript, ROUTES);
    let create = declare(
        &mut g,
        routes,
        BlockKind::Function,
        Some("createOrder"),
        &ROUTES[7..ROUTES.find("}\n").unwrap() + 1],
    );
    g.type_link(create, "Draft", 0, Outcome::Resolved(Key::from_text(DRAFT)));
    call(
        &mut g,
        create,
        "ordersQueue.add(\"place\", draft)",
        Outcome::external("bullmq"),
    );
    call(
        &mut g,
        create,
        "repo.insert(draft)",
        Outcome::Unresolved(Reason::UnknownReceiver),
    );
    call(
        &mut g,
        create,
        "charge(draft)",
        Outcome::Resolved(Key::from_text(CHARGE)),
    );
    declare(
        &mut g,
        routes,
        BlockKind::Function,
        Some("charge"),
        "function charge(draft: Draft) {}",
    );

    let repository = g.file("repository.ts", Language::TypeScript, REPOSITORY);
    declare(
        &mut g,
        repository,
        BlockKind::Class,
        Some("Repository"),
        &REPOSITORY[7..REPOSITORY.len() - 1],
    );

    let worker = g.file("worker.ts", Language::TypeScript, WORKER);
    let variable = declare(
        &mut g,
        worker,
        BlockKind::Variable,
        Some("worker"),
        "worker = new Worker(\"orders\", createOrder)",
    );
    let construction = declare(
        &mut g,
        variable,
        BlockKind::New,
        None,
        "new Worker(\"orders\", createOrder)",
    );
    let evidence = g.source_evidence(construction);
    let entrance = g.entrance(
        DetectorId::BullmqWorker,
        EntranceKind::Worker,
        None,
        Some("orders"),
        evidence,
    );
    g.target(entrance, Outcome::Resolved(Key::from_text(CREATE)), "createOrder");

    let server = g.file("server.ts", Language::TypeScript, SERVER);
    let statement = declare(
        &mut g,
        server,
        BlockKind::ExpressionStatement,
        None,
        SERVER.trim_end(),
    );
    let range = g.find(statement, "app.post(\"/orders\", createOrder)");
    let post = g.block(statement, BlockKind::Call, Some(Role::Expression), None, range);
    let evidence = g.source_evidence(post);
    let route = g.entrance(
        DetectorId::ExpressRoute,
        EntranceKind::Http,
        Some(HttpMethod::Post),
        Some("/orders"),
        evidence,
    );
    g.target(route, Outcome::Resolved(Key::from_text(CREATE)), "createOrder");

    g.input(SCHEMA_FILE, InputRole::Evidence, schema);
    g.build()
}

/// The entrance key of `kind` in `graph`.
fn entrance_key(graph: &Graph, kind: EntranceKind) -> String {
    let entrance = graph
        .entrances
        .iter()
        .find(|entrance| entrance.kind == kind)
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
        project: None,
    }
}

fn lines(first: u32, last: u32) -> Anchor {
    Anchor::File {
        file: RepoPath::new(SCHEMA_FILE).unwrap(),
        lines: [first, last],
        project: None,
    }
}

fn card(id: &str, key: &str) -> Card {
    let reference = if key.starts_with("entrance:") {
        CardRef::entrance(Key::from_text(key), None)
    } else {
        CardRef::block(Key::from_text(key), None)
    };
    Card {
        id: id.to_owned(),
        reference,
        at: [0.0, 0.0],
        view: None,
        folded: false,
        description: None,
        expanded: Vec::new(),
    }
}

fn resource(id: &str, kind: ResourceKind, name: &str, anchors: Vec<Anchor>) -> Resource {
    Resource {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        project: None,
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

fn connection(id: &str, kind: LinkKind, from: &str, to: &str) -> Link {
    Link {
        kind: Some(kind),
        author: Some(agent()),
        ..Link::new(id, from, to)
    }
}

/// A board whose one page shows `createOrder`, `charge`, the worker, and the route.
fn board(graph: &Graph) -> Board {
    let mut board = Board::new("Orders");
    let page = &mut board.pages[0];
    page.cards.push(card("create", CREATE));
    page.cards.push(card("charge", CHARGE));
    page.cards
        .push(card("worker", &entrance_key(graph, EntranceKind::Worker)));
    page.cards
        .push(card("route", &entrance_key(graph, EntranceKind::Http)));
    board
}

fn save(
    board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    prepare_save(board, previous, &PageGraphs::own_only(graph))
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
    let mut enqueue = Link {
        step: Some(Key::from_text(ADD)),
        anchors: vec![key(QUEUE)],
        description: Some(text("Queues the order")),
        label: Some(text("place job")),
        ..connection("enqueue", LinkKind::Queue, "create", "worker")
    };
    enqueue.channel = Some("typed by hand".to_owned());
    page.links.push(enqueue);
    page.links.push(Link {
        label: Some(text("calls")),
        ..Link::new("drawn", "create", "charge")
    });
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
    let queue = bytes_of(QUEUES, "ordersQueue: Queue<Job> = new Queue(\"orders\")");
    let signature = Signature::of(format!("{step}\n{queue}\n").as_bytes());
    let enqueue = page.link("enqueue").unwrap();
    assert_eq!(enqueue.signature, Some(signature));
    assert_eq!(enqueue.description.as_ref().unwrap().signature, Some(signature));
    assert_eq!(enqueue.label.as_ref().unwrap().signature, Some(signature));
    assert_eq!(enqueue.channel.as_deref(), Some("orders"));

    // A drawn link's label is still signed over its ends' guards.
    let ends = |id: &str| {
        page.guard(None, page.card(id).unwrap().reference.key().as_str())
            .unwrap()
            .signature
            .0
    };
    let drawn = Signature::of(format!("{}\n{}", ends("create"), ends("charge")).as_bytes());
    assert_eq!(
        page.link("drawn").unwrap().label.as_ref().unwrap().signature,
        Some(drawn)
    );

    // A frame's description: its member cards' signatures, by card id, joined with line breaks.
    let members = [CREATE, &entrance_key(&graph, EntranceKind::Http)]
        .map(|key| loaded.signature(key).unwrap().0.to_string())
        .join("\n");
    assert_eq!(
        page.frames[0].description.as_ref().unwrap().signature,
        Some(Signature::of(members.as_bytes()))
    );

    // Every cited key is guarded, so it reconnects like a card's.
    for cited in [ORDER, REPOSITORY_CLASS, ADD, QUEUE] {
        assert!(page.guard(None, cited).is_some(), "{cited} is guarded");
    }
    let references = saved.references(None);
    assert_eq!(references.tables, [ORDER]);
    assert_eq!(references.files, [&RepoPath::new(SCHEMA_FILE).unwrap()]);

    // A drawn link given a kind and a step cites code: its label, signed over its ends before,
    // is signed over that code, not reported outdated.
    let mut cited = saved.clone();
    let link = cited.pages[0]
        .links
        .iter_mut()
        .find(|link| link.id == "drawn")
        .unwrap();
    link.kind = Some(LinkKind::Calls);
    link.author = Some(person());
    link.step = Some(Key::from_text(INSERT));
    let (resaved, health) = save(cited, Some((&saved, &health)), &loaded).unwrap();
    assert!(health.is_healthy(), "{:#?}", health.items);
    let link = resaved.pages[0].link("drawn").unwrap();
    assert_eq!(link.label.as_ref().unwrap().signature, link.signature);
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
    let health = board_health(&saved, &PageGraphs::own_only(&stale));
    let outdated = &health.items[0];
    assert_eq!(
        (outdated.kind, &outdated.target, outdated.message.as_str()),
        (
            HealthKind::Outdated,
            &Target::Resource("orders".to_owned()),
            "Code changed since this resource card was drawn"
        )
    );
    assert_eq!(outdated.repairs, [RepairKind::Resign, RepairKind::RemoveResource]);
    assert_eq!(health.label().as_deref(), Some("1 outdated"));

    // After a compile records the new bytes, re-signing accepts them.
    let recompiled = BoardGraph::from_graph(&orders_graph(&changed), dir.path());
    let mut page = saved.pages[0].clone();
    apply_repair(
        &mut page,
        &PageGraphs::own_only(&recompiled),
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
            project: None,
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
    health(page, &PageGraphs::own_only(graph))
}

#[test]
fn claims_go_missing_broken_or_held_when_their_code_changes() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.resources.push(table(
        "types.ts#interface:Gone",
        &[],
        vec![key("queues.ts#variable:gone")],
    ));
    page.links.push(Link {
        step: Some(Key::from_text("routes.ts#function:createOrder/return[0]")),
        carries: Some(Key::from_text(HELPER)),
        anchors: vec![key(QUEUE), key("queues.ts#variable:gone")],
        ..connection("enqueue", LinkKind::Queue, "create", "worker")
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
    let link = Target::Link("enqueue".to_owned());
    let missing = [RepairKind::Reconnect, RepairKind::RemoveResource];
    let broken = [RepairKind::Reconnect, RepairKind::RemoveLink];
    assert_eq!(
        items,
        [
            (
                HealthKind::Missing,
                &table,
                Some("types.ts#interface:Gone"),
                None,
                &missing[..]
            ),
            (
                HealthKind::Missing,
                &table,
                Some("queues.ts#variable:gone"),
                Some(0),
                &missing[..]
            ),
            (
                HealthKind::BrokenLink,
                &link,
                Some("routes.ts#function:createOrder/return[0]"),
                None,
                &broken[..]
            ),
            (HealthKind::BrokenLink, &link, Some(HELPER), None, &broken[..]),
            (
                HealthKind::BrokenLink,
                &link,
                Some("queues.ts#variable:gone"),
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
    assert!(refusal(board.clone(), &loaded).starts_with("Type types.ts#interface:Gone is not in the graph"),);

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
            &[
                RepairKind::Accept,
                RepairKind::Reconnect,
                RepairKind::RemoveResource
            ][..]
        )
    );
}

#[test]
fn reconnecting_a_card_carries_its_connections_steps_along() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.cards[0] = card("create", "routes.ts#function:placeOrder");
    page.links.push(Link {
        step: Some(Key::from_text(
            "routes.ts#function:placeOrder/expression_statement[0]/call[0]",
        )),
        anchors: vec![key("queues.ts#variable:queue")],
        ..connection("enqueue", LinkKind::Queue, "create", "worker")
    });
    page.resources.push(resource(
        "queue",
        ResourceKind::Queue,
        "orders",
        vec![key("queues.ts#variable:queue")],
    ));
    let graphs = PageGraphs::own_only(&loaded);
    let mut page = board.pages[0].clone();
    let reconnect = |from: &str, to: &str| Repair::Reconnect {
        project: None,
        from: from.to_owned(),
        to: to.to_owned(),
    };
    apply_repair(
        &mut page,
        &graphs,
        &reconnect("routes.ts#function:placeOrder", CREATE),
    )
    .unwrap();
    // The step moves to the same statement under the new key, and gets a fresh guard.
    let link = page.link("enqueue").unwrap();
    assert_eq!(link.step.as_ref().map(Key::as_str), Some(ADD));
    assert!(page.guard(None, ADD).is_some());
    // Reconnecting an anchor's key points every claim citing it at the new key.
    apply_repair(&mut page, &graphs, &reconnect("queues.ts#variable:queue", QUEUE)).unwrap();
    assert_eq!(page.link("enqueue").unwrap().anchors, [key(QUEUE)]);
    assert_eq!(page.resource("queue").unwrap().anchors, [key(QUEUE)]);
    assert!(health_of(&page, &loaded).is_healthy());
    // A key that isn't in the graph is refused, naming the claim that cites it.
    assert_eq!(
        apply_repair(
            &mut page,
            &graphs,
            &reconnect(QUEUE, "queues.ts#variable:nowhere")
        )
        .unwrap_err(),
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
    let graphs = PageGraphs::own_only(&loaded);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.resources.push(resource(
        "schema",
        ResourceKind::Table,
        "orders",
        vec![lines(1, 1)],
    ));
    page.links.push(Link {
        anchors: vec![key(QUEUE)],
        description: Some(text("Queues the order")),
        label: Some(text("job")),
        ..connection("enqueue", LinkKind::Queue, "create", "schema")
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
    apply_repair(&mut page, &graphs, &anchor(lines(1, 2))).unwrap();
    assert_eq!(page.resource("schema").unwrap().anchors, [lines(1, 2)]);
    assert!(matches!(
        apply_repair(&mut page, &graphs, &anchor(lines(9, 9))),
        Err(BoardError::Problem { .. })
    ));
    // Re-signing signs the claim and its texts over its code now.
    page.links[0].signature = Some(Signature::of(b"old"));
    apply_repair(
        &mut page,
        &graphs,
        &Repair::Resign(Target::Link("enqueue".to_owned())),
    )
    .unwrap();
    let signature = Signature::of(
        format!(
            "{}\n",
            bytes_of(QUEUES, "ordersQueue: Queue<Job> = new Queue(\"orders\")")
        )
        .as_bytes(),
    );
    let link = page.link("enqueue").unwrap();
    assert_eq!(
        [link.signature, link.label.as_ref().unwrap().signature],
        [Some(signature); 2]
    );
    // A connection's description repair removes its description and label.
    apply_repair(
        &mut page,
        &graphs,
        &Repair::RemoveDescription(Target::Link("enqueue".to_owned())),
    )
    .unwrap();
    let link = page.link("enqueue").unwrap();
    assert!(link.description.is_none() && link.label.is_none());
    // Removing a resource card removes the links to it; its note stays where it was.
    apply_repair(&mut page, &graphs, &Repair::RemoveResource("schema".to_owned())).unwrap();
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
        "Can't add the queue \"orders\": it has no anchors. Anchor it to the code that defines or uses it (a block or entrance key), or to lines of an evidence file"
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

    let link = |link: Link| {
        let mut board = board(&graph);
        board.pages[0].links.push(link);
        board
    };
    assert_eq!(
        refusal(
            link(connection("enqueue", LinkKind::Queue, "create", "worker")),
            &loaded
        ),
        "Can't connect createOrder to worker orders (queue): it has no anchors. A queue connection cites its receiving side: anchor it to the code or entrance that receives it"
    );
    let anchored = Link {
        anchors: vec![key(&entrance_key(&graph, EntranceKind::Worker))],
        ..connection("enqueue", LinkKind::Queue, "create", "worker")
    };
    save(link(anchored), None, &loaded).unwrap();
    // `writes`, `reads`, `relates`, and `other` connections need no anchors.
    save(
        link(connection("writes", LinkKind::Writes, "create", "charge")),
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
        "Can't add the queue \"payments\": \"payments\" doesn't appear in its anchors (ordersQueue in queues.ts). Use the name the code writes, or anchor it where the name is written"
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
        board.pages[0].links.push(Link {
            step: Some(Key::from_text(ADD)),
            anchors: vec![key(QUEUE)],
            channel: Some(channel.to_owned()),
            ..connection("enqueue", LinkKind::Queue, "create", to)
        });
        board
    };
    save(link("orders", "queue"), None, &loaded).unwrap();
    assert_eq!(
        refusal(link("emails", "queue"), &loaded),
        "Can't connect createOrder to the queue \"orders\" (queue): the channel \"emails\" doesn't appear in its step or anchors (routes.ts:2 and ordersQueue in queues.ts). Anchor it where the channel is named, or leave the channel out"
    );
    // An entrance at the `to` end gives the channel, whatever was typed.
    let (saved, _) = save(link("emails", "worker"), None, &loaded).unwrap();
    assert_eq!(
        saved.pages[0].link("enqueue").unwrap().channel.as_deref(),
        Some("orders")
    );
    let mut route = board(&graph);
    route.pages[0]
        .resources
        .push(resource("customer", ResourceKind::Actor, "Customer", Vec::new()));
    route.pages[0].links.push(Link {
        anchors: vec![key(&entrance_key(&graph, EntranceKind::Http))],
        ..connection("posts", LinkKind::Http, "customer", "route")
    });
    let (saved, _) = save(route, None, &loaded).unwrap();
    assert_eq!(
        saved.pages[0].link("posts").unwrap().channel.as_deref(),
        Some("POST /orders")
    );
}

#[test]
fn tables_backed_by_a_type_mark_its_fields() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let with = |orders: Resource, link: Option<Link>| {
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
    let relates = |column: &str| Link {
        from_column: Some(column.to_owned()),
        to_column: Some("id".to_owned()),
        cardinality: Some(Cardinality::ManyToOne),
        ..connection("customer", LinkKind::Relates, "orders", "customers")
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
        board.pages[0].links.push(Link {
            step: step.map(Key::from_text),
            anchors: vec![key(QUEUE)],
            carries: Some(Key::from_text(carries)),
            ..connection("enqueue", LinkKind::Queue, "create", "worker")
        });
        board
    };
    // `Job` is written in the anchor; `Draft` in the step's function.
    save(carrying(JOB, None), None, &loaded).unwrap();
    save(carrying(DRAFT, Some(ADD)), None, &loaded).unwrap();
    assert_eq!(
        refusal(carrying(DRAFT, None), &loaded),
        "Can't connect createOrder to worker orders (queue): Draft (types.ts) isn't used in its step's function or in its anchors. Carry a type the sending code or an anchor writes, or leave carries out"
    );
    assert_eq!(
        refusal(carrying(ORDER, Some(ADD)), &loaded),
        "Can't connect createOrder to worker orders (queue): Order (types.ts) isn't used in createOrder or in its anchors. Carry a type the sending code or an anchor writes, or leave carries out"
    );
}

#[test]
fn runtime_calls_leave_from_calls_unfold_cant_follow() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let calling = |step: Option<&str>| {
        let mut board = board(&graph);
        board.pages[0].links.push(Link {
            step: step.map(Key::from_text),
            anchors: vec![key(CHARGE)],
            ..connection("runtime", LinkKind::Calls, "create", "charge")
        });
        board
    };
    // Unresolved and external calls are what a runtime call explains.
    save(calling(Some(INSERT)), None, &loaded).unwrap();
    save(calling(Some(ADD)), None, &loaded).unwrap();
    assert_eq!(
        refusal(calling(Some(CHARGE_CALL)), &loaded),
        "Can't connect createOrder to charge (calls): the call charge (line 4) resolves to charge, so Unfold already draws it. A calls connection is for a call Unfold can't follow; open the call's arrow instead"
    );
    assert_eq!(
        refusal(
            calling(Some("routes.ts#function:createOrder/expression_statement[1]")),
            &loaded
        ),
        "Can't connect createOrder to charge (calls): its step (line 3) isn't a call. Give the call itself as the step, such as routes.ts#function:createOrder/expression_statement[1]/call[0]"
    );
    assert_eq!(
        refusal(calling(None), &loaded),
        "Can't connect createOrder to charge (calls): it has no step. A calls connection leaves from the call Unfold can't follow; give that call as its step"
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

#[test]
fn claims_citing_a_project_that_is_not_connected_are_kept_unchecked() {
    let (_dir, graph, loaded) = orders(SCHEMA);
    let web = ProjectName::parse("web").unwrap();
    let mut board = board(&graph);
    let mut elsewhere = resource(
        "queue",
        ResourceKind::Queue,
        "orders",
        vec![Anchor::Key {
            key: Key::from_text("api.ts#variable:queue"),
            project: Some(web.clone()),
        }],
    );
    elsewhere.author = person();
    board.pages[0].resources.push(elsewhere);
    board.refresh_projects();
    let (saved, health) = save(board.clone(), None, &loaded).unwrap();
    assert!(health.is_healthy());
    assert_eq!(saved.projects, std::slice::from_ref(&web));
    assert_eq!(
        saved.pages[0].resources[0].signature, None,
        "signed once connected"
    );
    // An agent's can't be grounded, so it is refused.
    board.pages[0].resources[0].author = agent();
    assert_eq!(
        refusal(board, &loaded),
        "Can't check the queue \"orders\": it cites web, which is not connected. Connect that project, or cite code of a connected one"
    );
    // Nor can a type it says a connection carries.
    let mut board = self::board(&graph);
    board.pages[0].links.push(Link {
        anchors: vec![key(QUEUE)],
        carries: Some(Key::from_text("api.ts#interface:Job")),
        carries_project: Some(web),
        ..connection("enqueue", LinkKind::Queue, "create", "worker")
    });
    board.refresh_projects();
    assert_eq!(
        refusal(board.clone(), &loaded),
        "Can't check the connection createOrder to worker orders (queue): it cites web, which is not connected. Connect that project, or cite code of a connected one"
    );
    board.pages[0].links[0].author = Some(person());
    save(board, None, &loaded).unwrap();
}

#[test]
fn mermaid_marks_outdated_claims() {
    let (dir, graph, loaded) = orders(SCHEMA);
    let mut board = board(&graph);
    let page = &mut board.pages[0];
    page.resources.push(resource(
        "schema",
        ResourceKind::Table,
        "orders",
        vec![lines(1, 1)],
    ));
    page.links.push(Link {
        anchors: vec![lines(1, 1)],
        ..connection("enqueue", LinkKind::Queue, "create", "schema")
    });
    let (saved, _) = save(board, None, &loaded).unwrap();
    std::fs::write(dir.path().join(SCHEMA_FILE), "drop table orders;\n").unwrap();
    let stale = BoardGraph::from_graph(&graph, dir.path());
    let output = crate::mermaid::page(&saved, "page-1", &PageGraphs::own_only(&stale)).unwrap();
    assert!(output.contains("r0[(\"table · orders (outdated)\")]"), "{output}");
    assert!(output.contains("|\"queue (outdated)\"|"), "{output}");
}
