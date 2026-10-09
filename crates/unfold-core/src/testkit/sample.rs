//! A small, complete, valid graph for tests: three TypeScript files, one Python file, two
//! configuration inputs, and three entrances, exercising every record type.

use super::{GraphBuilder, Outcome};
use crate::vocab::{
    BlockKind as K, DetectorId, EdgeKind, EntranceKind, HttpMethod, InputRole, Language, Reason, Role,
    SlotName as S,
};

/// `src/orders.ts`.
pub const ORDERS_TS: &str = r#"import { helper } from "./helper";

export function createOrder(req: unknown) {
  if (!req) {
    throw new Error("missing");
  }
  return save(req, helper);
}

function save(value: unknown, callback: unknown) {
  return value;
}
"#;

/// `src/helper.ts`.
pub const HELPER_TS: &str = "export function helper() {\n  return 1;\n}\n";

/// `src/server.ts`.
pub const SERVER_TS: &str = "app.post(\"/orders\", createOrder);\n";

/// `jobs/tasks.py`.
pub const TASKS_PY: &str = "def run(handler):\n    handler()\n";

/// `.unfold/config.toml`.
pub const CONFIG_TOML: &str =
    "[[entrance]]\nkind = \"worker\"\ntrigger = \"orders\"\nhandler = \"jobs/tasks.py#run\"\n";

/// `package.json`.
pub const PACKAGE_JSON: &str = "{ \"scripts\": { \"start\": \"node src/server.ts\" } }\n";

/// Builds the sample. Validating `builder.build()` against `builder.snapshots()` reports
/// nothing.
pub fn sample() -> GraphBuilder {
    let mut g = GraphBuilder::new();

    let helper_file = g.file("src/helper.ts", Language::TypeScript, HELPER_TS);
    let helper = g.block(
        helper_file,
        K::Function,
        Some(Role::Body),
        Some("helper"),
        g.find(helper_file, "function helper() {\n  return 1;\n}"),
    );
    g.slot(helper, S::Name, None, g.find(helper_file, "helper"));
    g.slot(helper, S::Body, None, g.find(helper_file, "{\n  return 1;\n}"));
    let helper_return = g.block(
        helper,
        K::Return,
        Some(Role::Body),
        None,
        g.find(helper_file, "return 1;"),
    );
    g.slot(helper_return, S::Expression, None, g.find(helper_file, "1"));

    let orders = g.file("src/orders.ts", Language::TypeScript, ORDERS_TS);
    let import = g.block(
        orders,
        K::Import,
        Some(Role::Body),
        None,
        g.find(orders, "import { helper } from \"./helper\";"),
    );
    g.slot(import, S::Specifier, None, g.find(orders, "\"./helper\""));
    g.edge(
        import,
        EdgeKind::Imports,
        Outcome::Resolved(g.key(helper_file)),
        "./helper",
        None,
    );

    let create = g.block(
        orders,
        K::Function,
        Some(Role::Body),
        Some("createOrder"),
        g.find(orders, "function createOrder(req: unknown) {\n  if (!req) {\n    throw new Error(\"missing\");\n  }\n  return save(req, helper);\n}"),
    );
    g.slot(create, S::Name, None, g.find(orders, "createOrder"));
    g.slot(create, S::Parameter, Some(0), g.find(orders, "req: unknown"));
    g.slot(
        create,
        S::Body,
        None,
        g.find(
            orders,
            "{\n  if (!req) {\n    throw new Error(\"missing\");\n  }\n  return save(req, helper);\n}",
        ),
    );
    let branch = g.block(
        create,
        K::If,
        Some(Role::Body),
        None,
        g.find(orders, "if (!req) {\n    throw new Error(\"missing\");\n  }"),
    );
    g.slot(branch, S::Condition, None, g.find(orders, "!req"));
    g.slot(
        branch,
        S::Then,
        None,
        g.find(orders, "{\n    throw new Error(\"missing\");\n  }"),
    );
    let throw = g.block(
        branch,
        K::Throw,
        Some(Role::Then),
        None,
        g.find(orders, "throw new Error(\"missing\");"),
    );
    g.slot(
        throw,
        S::Expression,
        None,
        g.find(orders, "new Error(\"missing\")"),
    );
    let construction = g.block(
        throw,
        K::New,
        Some(Role::Expression),
        None,
        g.find(orders, "new Error(\"missing\")"),
    );
    g.slot(construction, S::Callee, None, g.find(orders, "Error"));
    g.slot(construction, S::Argument, Some(0), g.find(orders, "\"missing\""));
    g.edge(
        construction,
        EdgeKind::Constructs,
        Outcome::external("global"),
        "Error",
        None,
    );
    let ret = g.block(
        create,
        K::Return,
        Some(Role::Body),
        None,
        g.find(orders, "return save(req, helper);"),
    );
    g.slot(ret, S::Expression, None, g.find(orders, "save(req, helper)"));
    let call = g.block(
        ret,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(orders, "save(req, helper)"),
    );
    let call_start = g.find(orders, "save(req, helper)").start;
    g.slot(call, S::Callee, None, g.find_from(orders, "save", call_start));
    g.slot(call, S::Argument, Some(0), g.find_from(orders, "req", call_start));
    g.slot(
        call,
        S::Argument,
        Some(1),
        g.find_from(orders, "helper", call_start),
    );

    let save = g.block(
        orders,
        K::Function,
        Some(Role::Body),
        Some("save"),
        g.find(
            orders,
            "function save(value: unknown, callback: unknown) {\n  return value;\n}",
        ),
    );
    let save_start = g.find(orders, "function save").start;
    g.slot(save, S::Name, None, g.find_from(orders, "save", save_start));
    g.slot(save, S::Parameter, Some(0), g.find(orders, "value: unknown"));
    g.slot(save, S::Parameter, Some(1), g.find(orders, "callback: unknown"));
    g.slot(save, S::Body, None, g.find(orders, "{\n  return value;\n}"));
    let save_return = g.block(
        save,
        K::Return,
        Some(Role::Body),
        None,
        g.find(orders, "return value;"),
    );
    g.slot(
        save_return,
        S::Expression,
        None,
        g.find_from(orders, "value", g.find(orders, "return value;").start),
    );
    g.edge(
        call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(save)),
        "save",
        None,
    );
    g.edge(
        call,
        EdgeKind::References,
        Outcome::Resolved(g.key(helper)),
        "helper",
        Some(1),
    );

    let server = g.file("src/server.ts", Language::TypeScript, SERVER_TS);
    let statement = g.block(
        server,
        K::ExpressionStatement,
        Some(Role::Body),
        None,
        g.find(server, "app.post(\"/orders\", createOrder);"),
    );
    let route = g.block(
        statement,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(server, "app.post(\"/orders\", createOrder)"),
    );
    g.slot(
        statement,
        S::Expression,
        None,
        g.find(server, "app.post(\"/orders\", createOrder)"),
    );
    g.slot(route, S::Callee, None, g.find(server, "app.post"));
    g.slot(route, S::Argument, Some(0), g.find(server, "\"/orders\""));
    g.slot(route, S::Argument, Some(1), g.find(server, "createOrder"));
    g.edge(
        route,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::UnknownReceiver),
        "app.post",
        None,
    );
    g.edge(
        route,
        EdgeKind::References,
        Outcome::Resolved(g.key(create)),
        "createOrder",
        Some(1),
    );

    let tasks = g.file("jobs/tasks.py", Language::Python, TASKS_PY);
    let run = g.block(
        tasks,
        K::Function,
        Some(Role::Body),
        Some("run"),
        g.find(tasks, "def run(handler):\n    handler()"),
    );
    g.slot(run, S::Name, None, g.find(tasks, "run"));
    g.slot(run, S::Parameter, Some(0), g.find(tasks, "handler"));
    g.slot(run, S::Body, None, g.find(tasks, ":\n    handler()"));
    let body_start = g.find(tasks, "    handler()").start + 4;
    let invoke_range = g.find_from(tasks, "handler()", body_start);
    let invoke = g.block(run, K::ExpressionStatement, Some(Role::Body), None, invoke_range);
    g.slot(invoke, S::Expression, None, invoke_range);
    let parameter_call = g.block(invoke, K::Call, Some(Role::Expression), None, invoke_range);
    g.slot(
        parameter_call,
        S::Callee,
        None,
        g.find_from(tasks, "handler", body_start),
    );
    g.edge(
        parameter_call,
        EdgeKind::Calls,
        Outcome::Unresolved(Reason::Parameter),
        "handler",
        Some(0),
    );

    g.input(".unfold/config.toml", InputRole::UnfoldConfig, CONFIG_TOML);
    g.input("package.json", InputRole::Package, PACKAGE_JSON);

    let evidence = g.source_evidence(route);
    let http = g.entrance(
        DetectorId::ExpressRoute,
        EntranceKind::Http,
        Some(HttpMethod::Post),
        Some("/orders"),
        evidence,
    );
    g.target(http, Outcome::Resolved(g.key(create)), "createOrder");

    let evidence = g.config_evidence(".unfold/config.toml", CONFIG_TOML.trim_end(), "/entrance/0");
    let worker = g.entrance(
        DetectorId::Manual,
        EntranceKind::Worker,
        None,
        Some("orders"),
        evidence,
    );
    g.target(worker, Outcome::Resolved(g.key(run)), "jobs/tasks.py#run");

    let evidence = g.config_evidence(
        "package.json",
        "\"start\": \"node src/server.ts\"",
        "/scripts/start",
    );
    let start = g.entrance(
        DetectorId::PackageScript,
        EntranceKind::Process,
        None,
        Some("start"),
        evidence,
    );
    g.target(start, Outcome::Resolved(g.key(server)), "src/server.ts");

    g
}
