//! Derived views on small hand-written graphs.

use unfold_core::testkit::{BlockRef, GraphBuilder, Outcome};
use unfold_core::vocab::{
    BlockKind as K, DetectorId, EdgeKind, EntranceKind, HttpMethod, InputRole, Language, Reason, Role,
};
use unfold_core::{ByteRange, Graph, Key};
use unfold_store::{DiskStamp, EntranceChanges, FileChange, Membership, Store, StoreConfig, Update};

use crate::catalog::label;
use crate::relations::outgoing;
use crate::{GraphIndex, ReaderCache, ReaderIndex, project};

const APP_TS: &str = r#"import { route } from "./routes";
app.get("/orders", route);
class Service {
  run() {
    helper();
  }
}
function helper() {}
"#;

const ROUTES_TS: &str = r#"export function route(req) {
  const svc = new Service();
  svc.run();
  items.forEach((item) => {
    log(item);
  });
  if (req) {
    audit();
  }
  return fetch(req);
}
function audit() {}
function orphan() {}
"#;

const CLI_PY: &str = "@command\ndef main():\n    run()\n\ndef run():\n    pass\n";

/// Keys of the blocks tests refer to.
struct Sample {
    graph: Graph,
    app: Key,
    routes: Key,
    service: Key,
    run: Key,
    helper: Key,
    helper_call: Key,
    route: Key,
    callback: Key,
    branch: Key,
    audit: Key,
    orphan: Key,
    cli: Key,
    main: Key,
    run_py: Key,
    decorator_call: Key,
}

fn add(
    g: &mut GraphBuilder,
    parent: BlockRef,
    kind: K,
    role: Role,
    name: Option<&str>,
    needle: &str,
) -> BlockRef {
    let range = g.find(parent, needle);
    g.block(parent, kind, Some(role), name, range)
}

fn sample() -> Sample {
    let mut g = GraphBuilder::new();
    g.input(
        "package.json",
        InputRole::Package,
        "{\"scripts\":{\"start\":\"node app.ts\"}}",
    );
    g.input(
        "pyproject.toml",
        InputRole::Pyproject,
        "[project.scripts]\ncli = \"cli:main\"\n",
    );
    g.input(
        ".unfold/config.toml",
        InputRole::UnfoldConfig,
        "[[entrance]]\nkind = \"command\"\n[[entrance]]\nkind = \"worker\"\n",
    );

    let app = g.file("src/app.ts", Language::TypeScript, APP_TS);
    let routes = g.file("src/routes.ts", Language::TypeScript, ROUTES_TS);

    let import = add(
        &mut g,
        app,
        K::Import,
        Role::Body,
        None,
        "import { route } from \"./routes\";",
    );
    g.edge(
        import,
        EdgeKind::Imports,
        Outcome::Resolved(g.key(routes)),
        "./routes",
        None,
    );
    let register = add(
        &mut g,
        app,
        K::ExpressionStatement,
        Role::Body,
        None,
        "app.get(\"/orders\", route);",
    );
    let register_call = add(
        &mut g,
        register,
        K::Call,
        Role::Expression,
        None,
        "app.get(\"/orders\", route)",
    );
    let service = add(
        &mut g,
        app,
        K::Class,
        Role::Body,
        Some("Service"),
        "class Service {\n  run() {\n    helper();\n  }\n}",
    );
    let run = add(
        &mut g,
        service,
        K::Method,
        Role::Body,
        Some("run"),
        "run() {\n    helper();\n  }",
    );
    let helper_statement = add(&mut g, run, K::ExpressionStatement, Role::Body, None, "helper();");
    let helper_call = add(
        &mut g,
        helper_statement,
        K::Call,
        Role::Expression,
        None,
        "helper()",
    );
    let helper = add(
        &mut g,
        app,
        K::Function,
        Role::Body,
        Some("helper"),
        "function helper() {}",
    );
    g.edge(
        helper_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(helper)),
        "helper",
        None,
    );

    let route_range = ByteRange::new(
        g.find(routes, "export function route").start,
        g.find(routes, "return fetch(req);\n}").end,
    )
    .unwrap();
    let route = g.block(routes, K::Function, Some(Role::Body), Some("route"), route_range);
    let svc = add(
        &mut g,
        route,
        K::Variable,
        Role::Body,
        Some("svc"),
        "const svc = new Service();",
    );
    let construct = add(&mut g, svc, K::New, Role::Initializer, None, "new Service()");
    g.edge(
        construct,
        EdgeKind::Constructs,
        Outcome::Resolved(g.key(service)),
        "Service",
        None,
    );
    let run_statement = add(
        &mut g,
        route,
        K::ExpressionStatement,
        Role::Body,
        None,
        "svc.run();",
    );
    let run_call = add(
        &mut g,
        run_statement,
        K::Call,
        Role::Expression,
        None,
        "svc.run()",
    );
    g.edge(
        run_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(run)),
        "svc.run",
        None,
    );
    let each_statement = add(
        &mut g,
        route,
        K::ExpressionStatement,
        Role::Body,
        None,
        "items.forEach((item) => {\n    log(item);\n  });",
    );
    let each_call = add(
        &mut g,
        each_statement,
        K::Call,
        Role::Expression,
        None,
        "items.forEach((item) => {\n    log(item);\n  })",
    );
    g.edge(
        each_call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownReceiver),
        "items.forEach",
        None,
    );
    let callback = add(
        &mut g,
        each_call,
        K::Function,
        Role::Argument,
        None,
        "(item) => {\n    log(item);\n  }",
    );
    let log_statement = add(
        &mut g,
        callback,
        K::ExpressionStatement,
        Role::Body,
        None,
        "log(item);",
    );
    let log_call = add(
        &mut g,
        log_statement,
        K::Call,
        Role::Expression,
        None,
        "log(item)",
    );
    g.edge(
        log_call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "log",
        None,
    );
    let branch = add(
        &mut g,
        route,
        K::If,
        Role::Body,
        None,
        "if (req) {\n    audit();\n  }",
    );
    let audit_statement = add(
        &mut g,
        branch,
        K::ExpressionStatement,
        Role::Then,
        None,
        "audit();",
    );
    let audit_call = add(
        &mut g,
        audit_statement,
        K::Call,
        Role::Expression,
        None,
        "audit()",
    );
    let fetch_return = add(&mut g, route, K::Return, Role::Body, None, "return fetch(req);");
    let fetch_call = add(
        &mut g,
        fetch_return,
        K::Call,
        Role::Expression,
        None,
        "fetch(req)",
    );
    g.edge(
        fetch_call,
        EdgeKind::Calls,
        Outcome::external("global"),
        "fetch",
        None,
    );
    let audit = add(
        &mut g,
        routes,
        K::Function,
        Role::Body,
        Some("audit"),
        "function audit() {}",
    );
    g.edge(
        audit_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(audit)),
        "audit",
        None,
    );
    let orphan = add(
        &mut g,
        routes,
        K::Function,
        Role::Body,
        Some("orphan"),
        "function orphan() {}",
    );
    // The registration passes `route` (a reference) after its external occurrence.
    g.edge(
        register_call,
        EdgeKind::Calls,
        Outcome::external("express"),
        "app.get",
        None,
    );
    g.edge(
        register_call,
        EdgeKind::References,
        Outcome::Resolved(g.key(route)),
        "route",
        Some(1),
    );

    let cli = g.file("cli.py", Language::Python, CLI_PY);
    let main = add(
        &mut g,
        cli,
        K::Function,
        Role::Body,
        Some("main"),
        "@command\ndef main():\n    run()",
    );
    let decorator_call = add(&mut g, main, K::Call, Role::Decorator, None, "command");
    g.edge(
        decorator_call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "command",
        None,
    );
    let run_py_statement = add(&mut g, main, K::ExpressionStatement, Role::Body, None, "run()");
    let run_py_call = add(&mut g, run_py_statement, K::Call, Role::Expression, None, "run()");
    let run_py = add(
        &mut g,
        cli,
        K::Function,
        Role::Body,
        Some("run"),
        "def run():\n    pass",
    );
    g.edge(
        run_py_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(run_py)),
        "run",
        None,
    );

    let start = g.entrance(
        DetectorId::PackageScript,
        EntranceKind::Process,
        None,
        Some("start"),
        g.config_evidence("package.json", "\"start\":\"node app.ts\"", "/scripts/start"),
    );
    g.target(start, Outcome::Resolved(g.key(app)), "app.ts");
    let console = g.entrance(
        DetectorId::PyprojectScript,
        EntranceKind::Process,
        None,
        Some("cli"),
        g.config_evidence("pyproject.toml", "cli = \"cli:main\"", "/project/scripts/cli"),
    );
    g.target(console, Outcome::Resolved(g.key(main)), "cli:main");
    let http = g.entrance(
        DetectorId::ExpressRoute,
        EntranceKind::Http,
        Some(HttpMethod::Get),
        Some("/orders"),
        g.source_evidence(register_call),
    );
    g.target(http, Outcome::Resolved(g.key(route)), "route");
    let command = g.entrance(
        DetectorId::Manual,
        EntranceKind::Command,
        None,
        Some("audit"),
        g.config_evidence(".unfold/config.toml", "kind = \"command\"", "/entrance/0"),
    );
    g.target(command, Outcome::Resolved(g.key(branch)), "src/routes.ts:7");
    let worker = g.entrance(
        DetectorId::Manual,
        EntranceKind::Worker,
        None,
        Some("orphans"),
        g.config_evidence(".unfold/config.toml", "kind = \"worker\"", "/entrance/1"),
    );
    g.target(worker, Outcome::Resolved(g.key(orphan)), "src/routes.ts#orphan");

    Sample {
        app: g.key(app),
        routes: g.key(routes),
        service: g.key(service),
        run: g.key(run),
        helper: g.key(helper),
        helper_call: g.key(helper_call),
        route: g.key(route),
        callback: g.key(callback),
        branch: g.key(branch),
        audit: g.key(audit),
        orphan: g.key(orphan),
        cli: g.key(cli),
        main: g.key(main),
        run_py: g.key(run_py),
        decorator_call: g.key(decorator_call),
        graph: sorted_edges(g.build()),
    }
}

/// The graph with each file's edges in model order (by source, then sequence); the builder keeps
/// insertion order, and the sample adds some edges after later sources.
fn sorted_edges(mut graph: Graph) -> Graph {
    for file in &mut graph.files {
        file.edges.sort_by_key(|edge| (edge.source, edge.seq));
    }
    graph
}

fn entrance_key(graph: &Graph, trigger: &str) -> Key {
    graph
        .entrances
        .iter()
        .find(|entrance| entrance.trigger.as_deref() == Some(trigger))
        .map(|entrance| entrance.key.clone())
        .expect("the sample declares the entrance")
}

fn strings(keys: &[Key]) -> Vec<&str> {
    keys.iter().map(Key::as_str).collect()
}

#[test]
fn owners_follow_activation_scopes() {
    let s = sample();
    let index = ReaderIndex::from_graph(&s.graph);
    let owner = |key: &Key| index.owner(key.as_str()).cloned();
    // A nested callback owns its own calls; the call it is passed to belongs to the function.
    let log_call = format!("{}/expression_statement[0]/call[0]", s.callback);
    assert_eq!(index.owner(&log_call), Some(&s.callback));
    assert_eq!(owner(&s.callback), Some(s.route.clone()));
    // A method's calls belong to the method, and the method to its class.
    assert_eq!(owner(&s.helper_call), Some(s.run.clone()));
    assert_eq!(owner(&s.run), Some(s.service.clone()));
    // A decorator belongs to the enclosing scope, not the decorated function.
    assert_eq!(owner(&s.decorator_call), Some(s.cli.clone()));
    assert_eq!(index.owner("src/absent.ts#"), None);
}

#[test]
fn the_catalog_names_and_qualifies_definitions() {
    let s = sample();
    let projections = project(&s.graph);
    let all: Vec<(&str, &str, &str)> = projections
        .definitions
        .iter()
        .flatten()
        .map(|definition| {
            (
                definition.key.as_str(),
                &*definition.qualified_name,
                &*definition.label,
            )
        })
        .collect();
    assert!(all.contains(&(s.run.as_str(), "Service.run", "run")));
    assert!(all.contains(&(s.service.as_str(), "Service", "Service")));
    // The unnamed callback is neither named nor targeted, so it is not a definition.
    assert!(!all.iter().any(|(key, _, _)| *key == s.callback.as_str()));
    let routes = s.graph.file("src/routes.ts").expect("the sample has routes.ts");
    let callback_local = routes
        .blocks
        .iter()
        .position(|block| block.key == s.callback)
        .expect("the callback is in routes.ts");
    assert_eq!(
        label(routes, u32::try_from(callback_local).unwrap()).as_deref(),
        Some("callback to items.forEach")
    );
    // Definitions are per file, in preorder, aligned with the graph's files.
    let app_index = s
        .graph
        .files
        .iter()
        .position(|file| file.file.path.as_str() == "src/app.ts")
        .unwrap();
    let app_keys: Vec<&str> = projections
        .file_definitions(app_index)
        .iter()
        .map(|d| d.key.as_str())
        .collect();
    assert_eq!(app_keys, [s.service.as_str(), s.run.as_str(), s.helper.as_str()]);
}

#[test]
fn targeted_unnamed_callables_join_the_catalog_with_their_labels() {
    let mut g = GraphBuilder::new();
    let file = g.file(
        "src/a.ts",
        Language::TypeScript,
        "const handler = function () {};\nrun(handler);\n",
    );
    let variable = add(
        &mut g,
        file,
        K::Variable,
        Role::Body,
        Some("handler"),
        "const handler = function () {};",
    );
    let callable = add(
        &mut g,
        variable,
        K::Function,
        Role::Initializer,
        None,
        "function () {}",
    );
    let statement = add(
        &mut g,
        file,
        K::ExpressionStatement,
        Role::Body,
        None,
        "run(handler);",
    );
    let call = add(&mut g, statement, K::Call, Role::Expression, None, "run(handler)");
    g.edge(
        call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownName),
        "run",
        None,
    );
    g.edge(
        call,
        EdgeKind::References,
        Outcome::Resolved(g.key(callable)),
        "handler",
        Some(0),
    );
    let projections = project(&g.build());
    let definitions = &projections.definitions[0];
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].key, g.key(callable));
    assert_eq!(&*definitions[0].label, "anonymous function");
    assert_eq!(&*definitions[0].qualified_name, "handler.anonymous function");
}

#[test]
fn closures_follow_resolved_edges_and_owned_classes() {
    let s = sample();
    let index = ReaderIndex::from_graph(&s.graph);
    let reached = index.closure(&[s.app.as_str()]);
    let mut expected = vec![
        s.app.as_str(),
        s.service.as_str(),
        s.run.as_str(),
        s.helper.as_str(),
        s.routes.as_str(),
        s.route.as_str(),
        s.audit.as_str(),
    ];
    expected.sort_unstable();
    let mut actual = strings(&reached);
    actual.sort_unstable();
    // The callback is never targeted, so its calls are not followed; external and unresolved
    // edges stop the walk; `orphan` is never reached.
    assert_eq!(actual, expected);
    // A branch seed considers only the occurrences inside the branch.
    assert_eq!(
        strings(&index.closure(&[s.branch.as_str()])),
        [s.branch.as_str(), s.audit.as_str()]
    );
    // Unknown seeds are ignored.
    assert!(index.closure(&["src/absent.ts#"]).is_empty());
}

#[test]
fn applications_and_memberships() {
    let s = sample();
    let projections = project(&s.graph);
    let applications: Vec<(&str, &str)> = projections
        .applications
        .iter()
        .map(|application| (application.startup.as_str(), &*application.name))
        .collect();
    assert_eq!(
        applications,
        [(s.main.as_str(), "cli#main"), (s.app.as_str(), "app")]
    );
    let route = entrance_key(&s.graph, "/orders");
    let command = entrance_key(&s.graph, "audit");
    // The route's evidence is a top-level registration (owner: the file); the command's branch
    // target anchors to its owner, `route`. The orphan worker belongs to no application.
    let mut expected = vec![
        Membership {
            entrance: route,
            application: s.app.clone(),
        },
        Membership {
            entrance: command,
            application: s.app.clone(),
        },
    ];
    expected.sort();
    assert_eq!(projections.memberships, expected);
    // The console script seeds its callable and the module root.
    let index = ReaderIndex::from_graph(&s.graph);
    let closure = index
        .entrance_closure(entrance_key(&s.graph, "cli").as_str())
        .unwrap();
    assert_eq!(
        strings(&closure),
        [s.cli.as_str(), s.main.as_str(), s.run_py.as_str()]
    );
}

#[test]
fn relations_callers_outgoing_and_reached_from() {
    let s = sample();
    let index = ReaderIndex::from_graph(&s.graph);
    let callers = index.callers(s.helper.as_str()).unwrap();
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].owner, s.run);
    assert_eq!(callers[0].source, s.helper_call);
    assert_eq!(callers[0].kind, EdgeKind::Calls);
    assert!(index.callers("src/absent.ts#").is_none());

    let routes = s.graph.file("src/routes.ts").unwrap();
    let route_local = routes
        .blocks
        .iter()
        .position(|block| block.key == s.route)
        .unwrap();
    let grouped = outgoing(routes, u32::try_from(route_local).unwrap(), |key| {
        key == s.run.as_str()
    })
    .unwrap();
    let texts = |calls: &[crate::relations::OutgoingCall]| {
        calls.iter().map(|call| call.text.to_string()).collect::<Vec<_>>()
    };
    assert_eq!(texts(&grouped.on_board), ["svc.run"]);
    assert_eq!(texts(&grouped.openable), ["Service", "audit"]);
    assert_eq!(texts(&grouped.external_or_unresolved), ["items.forEach", "fetch"]);
    let notes: Vec<Option<&str>> = grouped
        .external_or_unresolved
        .iter()
        .map(|call| call.note.as_deref())
        .collect();
    assert_eq!(notes, [Some("receiver type unknown"), Some("global")]);
    assert_eq!(grouped.openable[1].line, 8);

    let reached = index.reached_from(s.audit.as_str()).unwrap();
    let mut expected = vec![entrance_key(&s.graph, "/orders"), entrance_key(&s.graph, "audit")];
    expected.sort();
    let mut actual = reached;
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        index.reached_from(s.orphan.as_str()).unwrap(),
        [entrance_key(&s.graph, "orphans")]
    );
    assert_eq!(index.non_process_entrances().len(), 3);
}

fn commit(store: &mut Store, graph: &Graph) {
    let projections = project(graph);
    let files = graph
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| FileChange {
            definitions: Some(projections.file_definitions(index)),
            ..FileChange::full(file, DiskStamp::default())
        })
        .collect();
    store
        .writer
        .commit(&Update {
            files,
            inputs: Some(&graph.inputs),
            entrances: EntranceChanges::all(&graph.entrances),
            applications: Some(&projections.applications),
            memberships: Some(&projections.memberships),
            ..Update::default()
        })
        .unwrap();
}

#[test]
fn the_index_loaded_from_the_store_matches_the_in_memory_one() {
    let s = sample();
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("graph.sqlite"), &StoreConfig::new("test")).unwrap();
    commit(&mut store, &s.graph);
    let loaded = ReaderIndex::load(&store.readers.get().unwrap()).unwrap();
    assert_eq!(loaded.index(), &GraphIndex::from_graph(&s.graph));
    let reader = store.readers.get().unwrap();
    assert_eq!(reader.applications().unwrap(), project(&s.graph).applications);

    // The cache loads once and reloads after invalidation.
    let cache = ReaderCache::new();
    let first = cache.get(&store.readers).unwrap();
    let again = cache.get(&store.readers).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    cache.invalidate();
    let reloaded = cache.get(&store.readers).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &reloaded));
    let installed = cache.install(ReaderIndex::from_graph(&s.graph));
    assert!(std::sync::Arc::ptr_eq(
        &installed,
        &cache.get(&store.readers).unwrap()
    ));

    // Requests that miss together share one load.
    cache.invalidate();
    let barrier = std::sync::Barrier::new(8);
    let loaded: Vec<_> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    cache.get(&store.readers).unwrap()
                })
            })
            .collect();
        workers.into_iter().map(|worker| worker.join().unwrap()).collect()
    });
    assert!(
        loaded
            .iter()
            .all(|index| std::sync::Arc::ptr_eq(index, &loaded[0]))
    );
}

#[test]
fn projections_are_deterministic_and_tolerate_broken_targets() {
    let s = sample();
    assert_eq!(project(&s.graph), project(&s.graph.clone()));
    // A resolved edge whose target is not indexed stops the walk instead of failing.
    let mut graph = s.graph.clone();
    for file in &mut graph.files {
        for edge in &mut file.edges {
            if edge.target.as_ref() == Some(&s.helper) {
                edge.target = Some(Key::from_text("src/gone.ts#function:gone"));
            }
        }
    }
    let index = ReaderIndex::from_graph(&graph);
    assert!(!index.closure(&[s.run.as_str()]).contains(&s.helper));
    assert!(
        project(&graph)
            .definitions
            .iter()
            .flatten()
            .any(|d| d.key == s.helper)
    );
}
