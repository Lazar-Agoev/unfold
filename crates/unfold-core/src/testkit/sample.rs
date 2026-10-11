//! A small, complete, valid graph for tests: three TypeScript files, one Python file, two
//! configuration inputs, and three entrances, exercising every record type except type links
//! and members (its code writes no type names and declares no types).

use super::{BlockRef, GraphBuilder, Outcome};
use crate::typed::{Argument, CallResult, Member, Pairing, Parameter, Signature, WrittenType};
use crate::vocab::{
    BlockKind as K, DetectorId, EdgeKind, EntranceKind, HttpMethod, InputRole, Language, MemberKind,
    ParameterKind, Reason, ResultBinding, Role, SlotName as S,
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

    add_typed(
        &mut g,
        &Typed {
            helper,
            create,
            construction,
            call,
            save,
            route,
            run,
            parameter_call,
        },
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

/// The sample's blocks that carry typed records.
struct Typed {
    helper: BlockRef,
    create: BlockRef,
    construction: BlockRef,
    call: BlockRef,
    save: BlockRef,
    route: BlockRef,
    run: BlockRef,
    parameter_call: BlockRef,
}

fn parameter(g: &GraphBuilder, block: BlockRef, index: u32, name: &str, ty: Option<&str>) -> Parameter {
    Parameter {
        block: block.local_id(),
        index,
        kind: ParameterKind::Positional,
        name: Some(name.into()),
        optional: false,
        default: false,
        ty: ty.map(|ty| {
            let from = g
                .find_from(block, &format!("{name}: {ty}"), g.range(block).start)
                .start;
            g.written_from(block, ty, from)
        }),
    }
}

fn argument(call: BlockRef, index: u32, pairing: Option<Pairing>) -> Argument {
    Argument {
        call: call.local_id(),
        index,
        expand: None,
        keyword: None,
        pairing,
        bound: None,
    }
}

fn result(call: BlockRef, binding: ResultBinding) -> CallResult {
    CallResult {
        call: call.local_id(),
        binding,
        awaited: false,
        target: None,
        position: None,
        bound: None,
    }
}

fn signature(block: BlockRef, returns: Option<WrittenType>) -> Signature {
    Signature {
        block: block.local_id(),
        returns,
    }
}

/// Every function's signature and parameters, every argument (paired where its call
/// resolved), and every call's result.
fn add_typed(g: &mut GraphBuilder, b: &Typed) {
    g.types(b.helper).signatures.push(signature(b.helper, None));

    let parameters = vec![
        parameter(g, b.create, 0, "req", Some("unknown")),
        parameter(g, b.save, 0, "value", Some("unknown")),
        parameter(g, b.save, 1, "callback", Some("unknown")),
    ];
    let types = g.types(b.create);
    types.signatures.push(signature(b.create, None));
    types.signatures.push(signature(b.save, None));
    types.parameters.extend(parameters);
    types.arguments.extend([
        argument(b.construction, 0, None),
        argument(b.call, 0, Some(Pairing::Parameter(0))),
        // `helper` is passed as a callable: a plain function binds nothing.
        Argument {
            bound: Some(0),
            ..argument(b.call, 1, Some(Pairing::Parameter(1)))
        },
    ]);
    types.results.push(result(b.construction, ResultBinding::Other));
    types.results.push(CallResult {
        bound: Some(0),
        ..result(b.call, ResultBinding::Return)
    });

    let types = g.types(b.route);
    types
        .arguments
        .extend([argument(b.route, 0, None), argument(b.route, 1, None)]);
    types.results.push(result(b.route, ResultBinding::Discard));

    let run_parameter = parameter(g, b.run, 0, "handler", None);
    let types = g.types(b.run);
    types.signatures.push(signature(b.run, None));
    types.parameters.push(run_parameter);
    types
        .results
        .push(result(b.parameter_call, ResultBinding::Discard));
}

/// `src/cart.ts` of [`typed`].
pub const CART_TS: &str = r#"export interface Cart {
  id: CartId;
  coupon?: string;
}

export type CartId = string;

export enum Status { Open = "open" }
"#;

/// `src/charge.ts` of [`typed`].
pub const CHARGE_TS: &str = r#"import { Cart } from "./cart";

export function charge(cart: Cart, ...items: Item[]): Promise<Receipt> {
  return pay(cart);
}

function pay(cart: Cart) {
  throw new Error("declined");
}
"#;

/// `jobs/settle.py` of [`typed`].
pub const SETTLE_PY: &str = r#"def settle(amount: int, *, currency="usd"):
    return amount

settle(5, currency="eur")
"#;

/// A small, valid graph exercising every typed record: an interface with members and a type
/// link to a type alias, an enum, signatures with written and missing types, resolved,
/// external, and unresolved type links, paired and unpaired arguments (a TypeScript rest
/// parameter, a Python keyword-only parameter), and every kind of call result but `variable`.
/// Validating `builder.build()` against `builder.snapshots()` reports nothing.
pub fn typed() -> GraphBuilder {
    let mut g = GraphBuilder::new();

    let cart = g.file("src/cart.ts", Language::TypeScript, CART_TS);
    let interface_text = "export interface Cart {\n  id: CartId;\n  coupon?: string;\n}";
    let interface = g.block(
        cart,
        K::Interface,
        Some(Role::Body),
        Some("Cart"),
        g.find(cart, interface_text),
    );
    g.slot(interface, S::Name, None, g.find(cart, "Cart"));
    g.slot(
        interface,
        S::Body,
        None,
        g.find(cart, "{\n  id: CartId;\n  coupon?: string;\n}"),
    );
    let alias = g.block(
        cart,
        K::TypeAlias,
        Some(Role::Body),
        Some("CartId"),
        g.find(cart, "export type CartId = string;"),
    );
    let alias_start = g.find(cart, "export type").start;
    g.slot(alias, S::Name, None, g.find_from(cart, "CartId", alias_start));
    g.slot(alias, S::Type, None, g.find_from(cart, "string", alias_start));
    let status = g.block(
        cart,
        K::Enum,
        Some(Role::Body),
        Some("Status"),
        g.find(cart, "export enum Status { Open = \"open\" }"),
    );
    g.slot(status, S::Name, None, g.find(cart, "Status"));
    g.slot(status, S::Body, None, g.find(cart, "{ Open = \"open\" }"));
    let id_type = g.written_from(cart, "CartId", g.find(cart, "id:").start);
    let coupon_type = g.written_from(cart, "string", g.find(cart, "coupon?").start);
    g.type_link(
        interface,
        "CartId",
        g.find(cart, "id:").start,
        Outcome::Resolved(g.key(alias)),
    );
    let types = g.types(cart);
    types.members.extend([
        Member {
            block: interface.local_id(),
            index: 0,
            kind: MemberKind::Property,
            name: Some("id".into()),
            optional: false,
            readonly: false,
            default: false,
            ty: Some(id_type),
            value: None,
        },
        Member {
            block: interface.local_id(),
            index: 1,
            kind: MemberKind::Property,
            name: Some("coupon".into()),
            optional: true,
            readonly: false,
            default: false,
            ty: Some(coupon_type),
            value: None,
        },
        Member {
            block: status.local_id(),
            index: 0,
            kind: MemberKind::Value,
            name: Some("Open".into()),
            optional: false,
            readonly: false,
            default: false,
            ty: None,
            value: Some("\"open\"".into()),
        },
    ]);

    let file = g.file("src/charge.ts", Language::TypeScript, CHARGE_TS);
    let import = g.block(
        file,
        K::Import,
        Some(Role::Body),
        None,
        g.find(file, "import { Cart } from \"./cart\";"),
    );
    g.slot(import, S::Specifier, None, g.find(file, "\"./cart\""));
    g.edge(
        import,
        EdgeKind::Imports,
        Outcome::Resolved(g.key(cart)),
        "./cart",
        None,
    );
    let charge_text =
        "export function charge(cart: Cart, ...items: Item[]): Promise<Receipt> {\n  return pay(cart);\n}";
    let charge = g.block(
        file,
        K::Function,
        Some(Role::Body),
        Some("charge"),
        g.find(file, charge_text),
    );
    let charge_start = g.find(file, charge_text).start;
    g.slot(
        charge,
        S::Name,
        None,
        g.find_from(file, "charge", charge_start + 16),
    );
    g.slot(
        charge,
        S::Parameter,
        Some(0),
        g.find_from(file, "cart: Cart", charge_start),
    );
    g.slot(charge, S::Parameter, Some(1), g.find(file, "...items: Item[]"));
    g.slot(charge, S::ReturnType, None, g.find(file, "Promise<Receipt>"));
    g.slot(charge, S::Body, None, g.find(file, "{\n  return pay(cart);\n}"));
    let ret = g.block(
        charge,
        K::Return,
        Some(Role::Body),
        None,
        g.find(file, "return pay(cart);"),
    );
    g.slot(ret, S::Expression, None, g.find(file, "pay(cart)"));
    let call = g.block(
        ret,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(file, "pay(cart)"),
    );
    let call_start = g.find(file, "pay(cart)").start;
    g.slot(call, S::Callee, None, g.find_from(file, "pay", call_start));
    g.slot(call, S::Argument, Some(0), g.find_from(file, "cart", call_start));
    let pay_text = "function pay(cart: Cart) {\n  throw new Error(\"declined\");\n}";
    let pay = g.block(
        file,
        K::Function,
        Some(Role::Body),
        Some("pay"),
        g.find(file, pay_text),
    );
    let pay_start = g.find(file, pay_text).start;
    g.slot(pay, S::Name, None, g.find_from(file, "pay", pay_start));
    g.slot(
        pay,
        S::Parameter,
        Some(0),
        g.find_from(file, "cart: Cart", pay_start),
    );
    g.slot(
        pay,
        S::Body,
        None,
        g.find(file, "{\n  throw new Error(\"declined\");\n}"),
    );
    let throw = g.block(
        pay,
        K::Throw,
        Some(Role::Body),
        None,
        g.find(file, "throw new Error(\"declined\");"),
    );
    g.slot(
        throw,
        S::Expression,
        None,
        g.find(file, "new Error(\"declined\")"),
    );
    let error = g.block(
        throw,
        K::New,
        Some(Role::Expression),
        None,
        g.find(file, "new Error(\"declined\")"),
    );
    g.slot(error, S::Callee, None, g.find(file, "Error"));
    g.slot(error, S::Argument, Some(0), g.find(file, "\"declined\""));
    g.edge(call, EdgeKind::Calls, Outcome::Resolved(g.key(pay)), "pay", None);
    g.edge(
        error,
        EdgeKind::Constructs,
        Outcome::external("global"),
        "Error",
        None,
    );
    let cart_param = g.find_from(file, "Cart", charge_start).start;
    g.type_link(charge, "Cart", cart_param, Outcome::Resolved(g.key(interface)));
    g.type_link(
        charge,
        "Item",
        charge_start,
        Outcome::Unresolved(Reason::UnknownName),
    );
    g.type_link(charge, "Promise", charge_start, Outcome::external("global"));
    g.type_link(
        charge,
        "Receipt",
        charge_start,
        Outcome::Unresolved(Reason::UnknownName),
    );
    g.type_link(pay, "Cart", pay_start, Outcome::Resolved(g.key(interface)));
    let charge_parameters = [
        parameter(&g, charge, 0, "cart", Some("Cart")),
        Parameter {
            kind: ParameterKind::Rest,
            ..parameter(&g, charge, 1, "items", Some("Item[]"))
        },
        parameter(&g, pay, 0, "cart", Some("Cart")),
    ];
    let returns = g.written_from(file, "Promise<Receipt>", charge_start);
    let types = g.types(file);
    types
        .signatures
        .extend([signature(charge, Some(returns)), signature(pay, None)]);
    types.parameters.extend(charge_parameters);
    types.arguments.extend([
        argument(call, 0, Some(Pairing::Parameter(0))),
        argument(error, 0, None),
    ]);
    types.results.extend([
        result(call, ResultBinding::Return),
        result(error, ResultBinding::Other),
    ]);

    let settle_file = g.file("jobs/settle.py", Language::Python, SETTLE_PY);
    let settle_text = "def settle(amount: int, *, currency=\"usd\"):\n    return amount";
    let settle = g.block(
        settle_file,
        K::Function,
        Some(Role::Body),
        Some("settle"),
        g.find(settle_file, settle_text),
    );
    g.slot(settle, S::Name, None, g.find_from(settle_file, "settle", 4));
    g.slot(settle, S::Parameter, Some(0), g.find(settle_file, "amount: int"));
    g.slot(
        settle,
        S::Parameter,
        Some(1),
        g.find(settle_file, "currency=\"usd\""),
    );
    g.slot(settle, S::Body, None, g.find(settle_file, ":\n    return amount"));
    let settle_return = g.block(
        settle,
        K::Return,
        Some(Role::Body),
        None,
        g.find(settle_file, "return amount"),
    );
    let return_start = g.find(settle_file, "return amount").start;
    g.slot(
        settle_return,
        S::Expression,
        None,
        g.find_from(settle_file, "amount", return_start),
    );
    let call_text = "settle(5, currency=\"eur\")";
    let statement = g.block(
        settle_file,
        K::ExpressionStatement,
        Some(Role::Body),
        None,
        g.find(settle_file, call_text),
    );
    g.slot(statement, S::Expression, None, g.find(settle_file, call_text));
    let settle_call = g.block(
        statement,
        K::Call,
        Some(Role::Expression),
        None,
        g.find(settle_file, call_text),
    );
    let settle_call_start = g.find(settle_file, call_text).start;
    g.slot(
        settle_call,
        S::Callee,
        None,
        g.find_from(settle_file, "settle", settle_call_start),
    );
    g.slot(
        settle_call,
        S::Argument,
        Some(0),
        g.find_from(settle_file, "5", settle_call_start),
    );
    g.slot(
        settle_call,
        S::Argument,
        Some(1),
        g.find(settle_file, "currency=\"eur\""),
    );
    g.edge(
        settle_call,
        EdgeKind::Calls,
        Outcome::Resolved(g.key(settle)),
        "settle",
        None,
    );
    g.type_link(settle, "int", 0, Outcome::external("python:builtins"));
    let settle_parameters = [
        parameter(&g, settle, 0, "amount", Some("int")),
        Parameter {
            kind: ParameterKind::KeywordOnly,
            default: true,
            ..parameter(&g, settle, 1, "currency", None)
        },
    ];
    let types = g.types(settle_file);
    types.signatures.push(signature(settle, None));
    types.parameters.extend(settle_parameters);
    types.arguments.extend([
        argument(settle_call, 0, Some(Pairing::Parameter(0))),
        Argument {
            keyword: Some("currency".into()),
            ..argument(settle_call, 1, Some(Pairing::Parameter(1)))
        },
    ]);
    types.results.push(CallResult {
        bound: Some(0),
        ..result(settle_call, ResultBinding::Discard)
    });
    g
}
