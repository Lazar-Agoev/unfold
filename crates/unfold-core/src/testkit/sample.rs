//! The `shop` example of the designs as a small, complete graph for tests.
//!
//! - `shop/service.py`: `place_order`, exactly the 21-block tree of diagram-blocks' worked
//!   example, under a module with `imports` links and a `#/` step.
//! - `shop/payments.py`: a class with an `extends` link, a decorator, a construction, and a
//!   lambda.
//! - `shop/store.py`: `place_order`'s callees, a loop whose header call takes a callback, and a
//!   `passes` link.
//! - `src/orders.ts`: an interface with members, a registration call keyed by its literal with
//!   its callback, awaited calls, and `types` links.
//! - `unfold.toml`: one app whose entrances' targets are found, missing, and not a start.
//!
//! Links reach every outcome, and every typed record kind appears.

use super::{GraphBuilder, NodeRef, builtin, library, project, target, unknown};
use crate::key::Callee;
use crate::model::LinkTarget;
use crate::text::ByteRange;
use crate::typed::{Argument, Member, Pairing, Parameter, Signature};
use crate::vocab::{
    CallRole, Flag, Form, Inline, InputRole, Language, LinkKind, MemberKind, PairingReason, ParameterKind,
    Reason, TargetState,
};

/// `shop/service.py`, the source of diagram-blocks' worked example.
pub const SERVICE_PY: &str = r#"import logging

from shop.payments import PaymentDeclined, charge_card
from shop.store import load_order, mark_failed, reserve_stock, save_order

logger = logging.getLogger(__name__)


def place_order(order_id):
    """Charge an order and save it."""
    order = load_order(order_id)
    if order is None:
        raise ValueError("unknown order")
    elif order["paid"]:
        return order

    total = sum(item["price"] for item in order["items"])
    try:
        receipt = charge_card(order["card"], total)
    except PaymentDeclined as error:
        logger.warning("declined: %s", error)
        return mark_failed(order)

    order["receipt"] = receipt
    save_order(order)
    for item in order["items"]:
        reserve_stock(item)
    return order
"#;

/// `shop/payments.py`.
pub const PAYMENTS_PY: &str = r#""""Card payments."""

import logging

logger = logging.getLogger(__name__)


class PaymentDeclined(Exception):
    """The card was declined."""


def retry(times):
    return lambda function: function


@retry(3)
def charge_card(card, amount):
    if amount <= 0:
        raise PaymentDeclined("nothing to charge")
    logger.info("charging %s", card)
    return {"card": card, "amount": amount}
"#;

/// `shop/store.py`.
pub const STORE_PY: &str = r#""""Orders, kept in memory."""

ORDERS = dict()


def load_order(order_id):
    return ORDERS.get(order_id)


def save_order(order):
    ORDERS[order["id"]] = order


def mark_failed(order):
    order["status"] = "failed"
    save_order(order)
    return order


def reserve_stock(item):
    for lot in sorted(item["lots"], key=lambda lot: lot["age"]):
        lot["reserved"] = True


def save_all(orders):
    list(map(save_order, orders))
"#;

/// `src/orders.ts`.
pub const ORDERS_TS: &str = r#"import { Router } from "express";

/** An order as the API sends it. */
export interface Order {
  id: string;
  total: number;
}

export const ordersRouter = Router();

ordersRouter.post("/orders/:id", async (req, res) => {
  const order: Order = await loadOrder(req.params.id);
  res.status(200).json(order);
});

async function loadOrder(id: string): Promise<Order> {
  return { id, total: 0 };
}
"#;

/// `unfold.toml`.
pub const UNFOLD_TOML: &str = r#"[[app]]
name = "Shop"
run = "python -m shop.service"
code = "shop/service.py#"
entrances = [
  { name = "Place an order", on = "POST /orders/{order_id}", code = "shop/service.py#place_order" },
  { name = "Orders API", on = "POST /api/orders/:id", code = 'src/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]' },
  { name = "Refund an order", on = "POST /orders/{order_id}/refund", code = "shop/service.py#refund_order" },
  { name = "Charge a card", code = ["shop/service.py#place_order/try[0]/body/call:charge_card", "shop/payments.py#charge_card"] },
]
"#;

/// The key of `place_order`'s handler, `src/orders.ts`'s registered callback.
pub const ORDERS_HANDLER: &str = "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]";

const PYTHON_BUILTINS: &str = "python:builtins";

/// Builds the sample.
pub fn shop() -> GraphBuilder {
    let mut g = GraphBuilder::new();
    let store = store(&mut g);
    let payments = payments(&mut g);
    service(&mut g, &store, &payments);
    orders(&mut g);

    g.input("unfold.toml", InputRole::UnfoldConfig, UNFOLD_TOML);
    let shop = g.app(
        "Shop",
        Some("python -m shop.service"),
        Some(target("shop/service.py#", TargetState::Found)),
    );
    g.entrance(
        shop,
        "Place an order",
        Some("POST /orders/{order_id}"),
        vec![target("shop/service.py#place_order", TargetState::Found)],
    );
    g.entrance(
        shop,
        "Orders API",
        Some("POST /api/orders/:id"),
        vec![target(ORDERS_HANDLER, TargetState::Found)],
    );
    g.entrance(
        shop,
        "Refund an order",
        Some("POST /orders/{order_id}/refund"),
        vec![target("shop/service.py#refund_order", TargetState::Missing)],
    );
    g.entrance(
        shop,
        "Charge a card",
        None,
        vec![
            target(
                "shop/service.py#place_order/try[0]/body/call:charge_card",
                TargetState::NotAStart,
            ),
            target("shop/payments.py#charge_card", TargetState::Found),
        ],
    );
    g
}

fn name(text: &str) -> Callee {
    Callee::name(text)
}

/// Fills a call's callee range (`head`), the statement it begins, and what it binds.
fn call_fields(g: &mut GraphBuilder, call: NodeRef, callee: &str, stmt: Option<&str>, binds: Option<&str>) {
    let range = g.range(call);
    let head = g.find_in(call, callee);
    let stmt = stmt.map(|stmt| {
        let offset = stmt
            .find(g.source(call, range))
            .expect("a statement contains the call it begins");
        let start = range.start - u32::try_from(offset).expect("test offsets fit in u32");
        let found = g.find_from(call, stmt, start);
        assert_eq!(found.start, start, "the statement surrounds the call");
        found
    });
    let node = g.node_mut(call);
    node.head = Some(head);
    node.stmt = stmt;
    node.binds = binds.map(Into::into);
}

/// Adds a `calls` link over the call's `head`, binding no parameter itself when it reaches a
/// project function.
fn calls(g: &mut GraphBuilder, call: NodeRef, to: LinkTarget) {
    let head = g.node_mut(call).head.expect("the call's head is filled first");
    let text = g.source(call, head).to_owned();
    let bound = matches!(to, LinkTarget::Project(_)).then_some(0);
    g.link(call, LinkKind::Calls, head, &text, to).bound = bound;
}

/// Adds a parameter of `function`, the `index`-th, written as `needle` inside it.
fn parameter(g: &mut GraphBuilder, function: NodeRef, index: u32, needle: &str, name: &str) {
    let range = g.find_word_in(function, needle);
    let ty = needle
        .split_once(": ")
        .map(|(_, ty)| g.written_from(function, ty, range.start));
    g.types(function).parameters.push(Parameter {
        node: function.local_id(),
        index,
        range,
        kind: ParameterKind::Positional,
        name: Some(name.into()),
        optional: false,
        default: false,
        ty,
    });
}

/// Adds the `index`-th argument of `call`, written as `needle` inside it.
fn argument<'g>(
    g: &'g mut GraphBuilder,
    call: NodeRef,
    index: u32,
    needle: &str,
    pairing: Option<Pairing>,
) -> &'g mut Argument {
    let range = g.find_word_in(call, needle);
    let arguments = &mut g.types(call).arguments;
    arguments.push(Argument {
        call: call.local_id(),
        index,
        range,
        expand: None,
        keyword: None,
        pairing,
        bound: None,
    });
    arguments.last_mut().expect("an argument was just pushed")
}

fn signature(g: &mut GraphBuilder, function: NodeRef) {
    g.types(function).signatures.push(Signature {
        node: function.local_id(),
        returns: None,
    });
}

/// The nodes of `shop/store.py` that `shop/service.py` links to.
struct Store {
    module: NodeRef,
    load_order: NodeRef,
    save_order: NodeRef,
    mark_failed: NodeRef,
    reserve_stock: NodeRef,
}

/// The nodes of `shop/payments.py` that `shop/service.py` links to.
struct Payments {
    module: NodeRef,
    declined: NodeRef,
    charge_card: NodeRef,
}

fn store(g: &mut GraphBuilder) -> Store {
    let module = g.file("shop/store.py", Language::Python, STORE_PY);
    g.node_mut(module).doc = Some(g.find(module, "\"\"\"Orders, kept in memory.\"\"\""));

    let orders = g.call(module, &name("dict"), g.find(module, "dict()"));
    call_fields(g, orders, "dict", Some("ORDERS = dict()"), Some("ORDERS"));
    calls(g, orders, builtin(PYTHON_BUILTINS));

    let load_order = function(
        g,
        module,
        "load_order",
        "def load_order(order_id):\n    return ORDERS.get(order_id)",
        "def load_order(order_id)",
    );
    parameter(g, load_order, 0, "order_id", "order_id");
    let ret = exit(
        g,
        load_order,
        Form::Return,
        "return ORDERS.get(order_id)",
        Some("ORDERS.get(order_id)"),
    );
    let get = g.call(
        ret,
        &name("ORDERS").member("get"),
        g.find_in(ret, "ORDERS.get(order_id)"),
    );
    call_fields(g, get, "ORDERS.get", None, None);
    g.node_mut(get).flags.insert(Flag::ExitValue);
    calls(g, get, builtin(PYTHON_BUILTINS));
    argument(g, get, 0, "order_id", None);

    let save_order = function(
        g,
        module,
        "save_order",
        "def save_order(order):\n    ORDERS[order[\"id\"]] = order",
        "def save_order(order)",
    );
    parameter(g, save_order, 0, "order", "order");

    let mark_failed = function(
        g,
        module,
        "mark_failed",
        "def mark_failed(order):\n    order[\"status\"] = \"failed\"\n    save_order(order)\n    return order",
        "def mark_failed(order)",
    );
    parameter(g, mark_failed, 0, "order", "order");
    let save = g.call(
        mark_failed,
        &name("save_order"),
        g.find_in(mark_failed, "save_order(order)"),
    );
    call_fields(g, save, "save_order", Some("save_order(order)"), None);
    calls(g, save, project(&g.key(save_order)));
    argument(g, save, 0, "order", Some(Pairing::Parameter(0)));
    exit(g, mark_failed, Form::Return, "return order", Some("order"));

    let reserve_stock = function(
        g,
        module,
        "reserve_stock",
        "def reserve_stock(item):\n    for lot in sorted(item[\"lots\"], key=lambda lot: lot[\"age\"]):\n        lot[\"reserved\"] = True",
        "def reserve_stock(item)",
    );
    parameter(g, reserve_stock, 0, "item", "item");
    let lp = g.block(
        reserve_stock,
        Form::For,
        g.find_in(
            reserve_stock,
            "for lot in sorted(item[\"lots\"], key=lambda lot: lot[\"age\"]):\n        lot[\"reserved\"] = True",
        ),
    );
    g.node_mut(lp).head = Some(g.find_in(
        lp,
        "for lot in sorted(item[\"lots\"], key=lambda lot: lot[\"age\"])",
    ));
    let sorted = g.registration(
        lp,
        &name("sorted"),
        None,
        g.find_in(lp, "sorted(item[\"lots\"], key=lambda lot: lot[\"age\"])"),
    );
    call_fields(g, sorted, "sorted", None, None);
    calls(g, sorted, builtin(PYTHON_BUILTINS));
    argument(g, sorted, 0, "item[\"lots\"]", None);
    argument(g, sorted, 1, "key=lambda lot: lot[\"age\"]", None).keyword = Some("key".into());
    let key = g.inline(
        sorted,
        Inline::Callback,
        g.find_in(sorted, "lambda lot: lot[\"age\"]"),
    );
    let head = g.find_in(key, "lambda lot");
    let body = g.find_in(key, "lot[\"age\"]");
    let node = g.node_mut(key);
    node.head = Some(head);
    node.body = Some(body);
    signature(g, key);
    parameter(g, key, 0, "lot", "lot");
    let lp_body = g.block(lp, Form::Body, g.find_in(lp, "lot[\"reserved\"] = True"));
    g.node_mut(lp_body).body = Some(g.range(lp_body));

    let save_all = function(
        g,
        module,
        "save_all",
        "def save_all(orders):\n    list(map(save_order, orders))",
        "def save_all(orders)",
    );
    parameter(g, save_all, 0, "orders", "orders");
    let list = g.call(
        save_all,
        &name("list"),
        g.find_in(save_all, "list(map(save_order, orders))"),
    );
    call_fields(g, list, "list", Some("list(map(save_order, orders))"), None);
    calls(g, list, builtin(PYTHON_BUILTINS));
    argument(g, list, 0, "map(save_order, orders)", None);
    let map = g.call(list, &name("map"), g.find_in(list, "map(save_order, orders)"));
    call_fields(g, map, "map", None, None);
    calls(g, map, builtin(PYTHON_BUILTINS));
    let passed = g.find_in(map, "save_order");
    g.link(
        map,
        LinkKind::Passes,
        passed,
        "save_order",
        project(&g.key(save_order)),
    );
    argument(g, map, 0, "save_order", None).bound = Some(0);
    argument(g, map, 1, "orders", None);

    Store {
        module,
        load_order,
        save_order,
        mark_failed,
        reserve_stock,
    }
}

fn payments(g: &mut GraphBuilder) -> Payments {
    let module = g.file("shop/payments.py", Language::Python, PAYMENTS_PY);
    g.node_mut(module).doc = Some(g.find(module, "\"\"\"Card payments.\"\"\""));
    let import = g.find_from(module, "logging", g.find(module, "import logging").start);
    g.link(
        module,
        LinkKind::Imports,
        import,
        "logging",
        library("python:logging"),
    );

    let get_logger = g.call(
        module,
        &name("logging").member("getLogger"),
        g.find(module, "logging.getLogger(__name__)"),
    );
    call_fields(
        g,
        get_logger,
        "logging.getLogger",
        Some("logger = logging.getLogger(__name__)"),
        Some("logger"),
    );
    calls(g, get_logger, library("python:logging"));

    let declined = g.def(
        module,
        Form::Class,
        "PaymentDeclined",
        g.find(
            module,
            "class PaymentDeclined(Exception):\n    \"\"\"The card was declined.\"\"\"",
        ),
    );
    let head = g.find_in(declined, "class PaymentDeclined(Exception)");
    let doc = g.find_in(declined, "\"\"\"The card was declined.\"\"\"");
    let node = g.node_mut(declined);
    node.head = Some(head);
    node.body = Some(doc);
    node.doc = Some(doc);
    let base = g.find_in(declined, "Exception");
    g.link(
        declined,
        LinkKind::Extends,
        base,
        "Exception",
        builtin(PYTHON_BUILTINS),
    );

    let retry = function(
        g,
        module,
        "retry",
        "def retry(times):\n    return lambda function: function",
        "def retry(times)",
    );
    parameter(g, retry, 0, "times", "times");
    let ret = exit(
        g,
        retry,
        Form::Return,
        "return lambda function: function",
        Some("lambda function: function"),
    );
    let lambda = g.inline(ret, Inline::Lambda, g.find_in(ret, "lambda function: function"));
    let head = g.find_in(lambda, "lambda function");
    let body = g.find_from(lambda, "function", head.end + 2);
    let node = g.node_mut(lambda);
    node.head = Some(head);
    node.body = Some(body);
    signature(g, lambda);
    parameter(g, lambda, 0, "function", "function");

    let decorator = g.call(module, &name("retry"), g.find(module, "retry(3)"));
    call_fields(g, decorator, "retry", None, None);
    calls(g, decorator, project(&g.key(retry)));
    argument(g, decorator, 0, "3", Some(Pairing::Parameter(0)));

    let charge_card = function(
        g,
        module,
        "charge_card",
        "def charge_card(card, amount):\n    if amount <= 0:\n        raise PaymentDeclined(\"nothing to charge\")\n    logger.info(\"charging %s\", card)\n    return {\"card\": card, \"amount\": amount}",
        "def charge_card(card, amount)",
    );
    let node = g.node_mut(decorator);
    node.role = Some(CallRole::Decorator);
    node.r#for = Some(charge_card.local_id());
    parameter(g, charge_card, 0, "card", "card");
    parameter(g, charge_card, 1, "amount", "amount");
    let decision = g.block(
        charge_card,
        Form::If,
        g.find_in(
            charge_card,
            "if amount <= 0:\n        raise PaymentDeclined(\"nothing to charge\")",
        ),
    );
    let then = part(
        g,
        decision,
        Form::Then,
        "if amount <= 0:\n        raise PaymentDeclined(\"nothing to charge\")",
        Some("amount <= 0"),
        "raise PaymentDeclined(\"nothing to charge\")",
    );
    let raise = exit(
        g,
        then,
        Form::Raise,
        "raise PaymentDeclined(\"nothing to charge\")",
        Some("PaymentDeclined(\"nothing to charge\")"),
    );
    let construction = g.construction(
        raise,
        &name("PaymentDeclined"),
        g.find_in(raise, "PaymentDeclined(\"nothing to charge\")"),
    );
    call_fields(g, construction, "PaymentDeclined", None, None);
    g.node_mut(construction).flags.insert(Flag::ExitValue);
    let head = g.node_mut(construction).head.expect("filled above");
    g.link(
        construction,
        LinkKind::Calls,
        head,
        "PaymentDeclined",
        project(&g.key(declined)),
    );
    argument(
        g,
        construction,
        0,
        "\"nothing to charge\"",
        Some(Pairing::Unpaired(PairingReason::NoSignature)),
    );
    let info = g.call(
        charge_card,
        &name("logger").member("info"),
        g.find_in(charge_card, "logger.info(\"charging %s\", card)"),
    );
    call_fields(
        g,
        info,
        "logger.info",
        Some("logger.info(\"charging %s\", card)"),
        None,
    );
    calls(g, info, unknown(Reason::UnknownReceiver));
    argument(g, info, 0, "\"charging %s\"", None);
    argument(g, info, 1, "card", None);
    exit(
        g,
        charge_card,
        Form::Return,
        "return {\"card\": card, \"amount\": amount}",
        Some("{\"card\": card, \"amount\": amount}"),
    );

    Payments {
        module,
        declined,
        charge_card,
    }
}

/// `shop/service.py`: diagram-blocks' "Proposed: 21 blocks".
fn service(g: &mut GraphBuilder, store: &Store, payments: &Payments) {
    let module = g.file("shop/service.py", Language::Python, SERVICE_PY);
    let imports = [
        ("import logging", "logging", library("python:logging")),
        (
            "from shop.payments",
            "shop.payments",
            project(&g.key(payments.module)),
        ),
        ("from shop.store", "shop.store", project(&g.key(store.module))),
    ];
    for (statement, specifier, to) in imports {
        let range = g.find_from(module, specifier, g.find(module, statement).start);
        g.link(module, LinkKind::Imports, range, specifier, to);
    }
    let get_logger = g.call(
        module,
        &name("logging").member("getLogger"),
        g.find(module, "logging.getLogger(__name__)"),
    );
    call_fields(
        g,
        get_logger,
        "logging.getLogger",
        Some("logger = logging.getLogger(__name__)"),
        Some("logger"),
    );
    calls(g, get_logger, library("python:logging"));

    let source = SERVICE_PY.trim_end();
    let place_order = function(
        g,
        module,
        "place_order",
        &source[source
            .find("def place_order")
            .expect("the sample defines place_order")..],
        "def place_order(order_id)",
    );
    g.node_mut(place_order).doc = Some(g.find_in(place_order, "\"\"\"Charge an order and save it.\"\"\""));
    parameter(g, place_order, 0, "order_id", "order_id");

    let load = g.call(
        place_order,
        &name("load_order"),
        g.find_in(place_order, "load_order(order_id)"),
    );
    call_fields(
        g,
        load,
        "load_order",
        Some("order = load_order(order_id)"),
        Some("order"),
    );
    calls(g, load, project(&g.key(store.load_order)));
    argument(g, load, 0, "order_id", Some(Pairing::Parameter(0)));

    let decision = g.block(
        place_order,
        Form::If,
        g.find_in(
            place_order,
            "if order is None:\n        raise ValueError(\"unknown order\")\n    elif order[\"paid\"]:\n        return order",
        ),
    );
    let then = part(
        g,
        decision,
        Form::Then,
        "if order is None:\n        raise ValueError(\"unknown order\")",
        Some("order is None"),
        "raise ValueError(\"unknown order\")",
    );
    let raise = exit(
        g,
        then,
        Form::Raise,
        "raise ValueError(\"unknown order\")",
        Some("ValueError(\"unknown order\")"),
    );
    let value_error = g.call(
        raise,
        &name("ValueError"),
        g.find_in(raise, "ValueError(\"unknown order\")"),
    );
    call_fields(g, value_error, "ValueError", None, None);
    g.node_mut(value_error).flags.insert(Flag::ExitValue);
    calls(g, value_error, builtin(PYTHON_BUILTINS));
    argument(g, value_error, 0, "\"unknown order\"", None);
    let elif = part(
        g,
        decision,
        Form::Elif,
        "elif order[\"paid\"]:\n        return order",
        Some("order[\"paid\"]"),
        "return order",
    );
    exit(g, elif, Form::Return, "return order", Some("order"));

    let sum = g.call(
        place_order,
        &name("sum"),
        g.find_in(place_order, "sum(item[\"price\"] for item in order[\"items\"])"),
    );
    call_fields(
        g,
        sum,
        "sum",
        Some("total = sum(item[\"price\"] for item in order[\"items\"])"),
        Some("total"),
    );
    calls(g, sum, builtin(PYTHON_BUILTINS));
    argument(g, sum, 0, "item[\"price\"] for item in order[\"items\"]", None);

    let attempt = g.block(
        place_order,
        Form::Try,
        g.find_in(place_order, "try:\n        receipt = charge_card(order[\"card\"], total)\n    except PaymentDeclined as error:\n        logger.warning(\"declined: %s\", error)\n        return mark_failed(order)"),
    );
    let body = part(
        g,
        attempt,
        Form::Body,
        "receipt = charge_card(order[\"card\"], total)",
        None,
        "receipt = charge_card(order[\"card\"], total)",
    );
    let charge = g.call(
        body,
        &name("charge_card"),
        g.find_in(body, "charge_card(order[\"card\"], total)"),
    );
    call_fields(
        g,
        charge,
        "charge_card",
        Some("receipt = charge_card(order[\"card\"], total)"),
        Some("receipt"),
    );
    calls(g, charge, project(&g.key(payments.charge_card)));
    argument(g, charge, 0, "order[\"card\"]", Some(Pairing::Parameter(0)));
    argument(g, charge, 1, "total", Some(Pairing::Parameter(1)));
    let except = part(
        g,
        attempt,
        Form::Except,
        "except PaymentDeclined as error:\n        logger.warning(\"declined: %s\", error)\n        return mark_failed(order)",
        Some("except PaymentDeclined as error"),
        "logger.warning(\"declined: %s\", error)\n        return mark_failed(order)",
    );
    let clause = g.find_in(except, "PaymentDeclined");
    g.link(
        except,
        LinkKind::Types,
        clause,
        "PaymentDeclined",
        project(&g.key(payments.declined)),
    );
    let warning = g.call(
        except,
        &name("logger").member("warning"),
        g.find_in(except, "logger.warning(\"declined: %s\", error)"),
    );
    call_fields(
        g,
        warning,
        "logger.warning",
        Some("logger.warning(\"declined: %s\", error)"),
        None,
    );
    calls(g, warning, unknown(Reason::UnknownReceiver));
    argument(g, warning, 0, "\"declined: %s\"", None);
    argument(g, warning, 1, "error", None);
    let early = exit(
        g,
        except,
        Form::Return,
        "return mark_failed(order)",
        Some("mark_failed(order)"),
    );
    let failed = g.call(
        early,
        &name("mark_failed"),
        g.find_in(early, "mark_failed(order)"),
    );
    call_fields(g, failed, "mark_failed", None, None);
    g.node_mut(failed).flags.insert(Flag::ExitValue);
    calls(g, failed, project(&g.key(store.mark_failed)));
    argument(g, failed, 0, "order", Some(Pairing::Parameter(0)));

    let save = g.call(
        place_order,
        &name("save_order"),
        g.find_in(place_order, "save_order(order)"),
    );
    call_fields(g, save, "save_order", Some("save_order(order)"), None);
    calls(g, save, project(&g.key(store.save_order)));
    argument(g, save, 0, "order", Some(Pairing::Parameter(0)));

    let lp = g.block(
        place_order,
        Form::For,
        g.find_in(
            place_order,
            "for item in order[\"items\"]:\n        reserve_stock(item)",
        ),
    );
    g.node_mut(lp).head = Some(g.find_in(lp, "for item in order[\"items\"]"));
    let lp_body = part(
        g,
        lp,
        Form::Body,
        "reserve_stock(item)",
        None,
        "reserve_stock(item)",
    );
    let reserve = g.call(
        lp_body,
        &name("reserve_stock"),
        g.find_in(lp_body, "reserve_stock(item)"),
    );
    call_fields(g, reserve, "reserve_stock", Some("reserve_stock(item)"), None);
    calls(g, reserve, project(&g.key(store.reserve_stock)));
    argument(g, reserve, 0, "item", Some(Pairing::Parameter(0)));
    let last = g.find_from(module, "return order", g.range(lp).end);
    let ret = g.block(place_order, Form::Return, last);
    let value = g.find_in(ret, "order");
    let node = g.node_mut(ret);
    node.stmt = Some(last);
    node.value = Some(value);
}

fn orders(g: &mut GraphBuilder) {
    let module = g.file("src/orders.ts", Language::TypeScript, ORDERS_TS);
    let specifier = g.find(module, "\"express\"");
    g.link(
        module,
        LinkKind::Imports,
        specifier,
        "express",
        library("express"),
    );

    let interface = g.def(
        module,
        Form::Interface,
        "Order",
        g.find(
            module,
            "export interface Order {\n  id: string;\n  total: number;\n}",
        ),
    );
    let head = g.find_in(interface, "interface Order");
    let body = g.find_in(interface, "{\n  id: string;\n  total: number;\n}");
    let doc = g.find(module, "/** An order as the API sends it. */");
    let node = g.node_mut(interface);
    node.head = Some(head);
    node.body = Some(body);
    node.doc = Some(doc);
    for (index, (member, ty)) in [("id", "string"), ("total", "number")].into_iter().enumerate() {
        let range = g.find_in(interface, &format!("{member}: {ty};"));
        let ty = g.written_from(interface, ty, range.start);
        g.types(interface).members.push(Member {
            node: interface.local_id(),
            index: u32::try_from(index).expect("two members"),
            range,
            kind: MemberKind::Property,
            name: Some(member.into()),
            optional: false,
            readonly: false,
            default: false,
            ty: Some(ty),
            value: None,
        });
    }

    let router = g.call(module, &name("Router"), g.find(module, "Router()"));
    call_fields(
        g,
        router,
        "Router",
        Some("export const ordersRouter = Router();"),
        Some("ordersRouter"),
    );
    calls(g, router, library("express"));

    let handler_source = "async (req, res) => {\n  const order: Order = await loadOrder(req.params.id);\n  res.status(200).json(order);\n}";
    let route = g.registration(
        module,
        &name("ordersRouter").member("post"),
        Some("/orders/:id"),
        g.find(
            module,
            &format!("ordersRouter.post(\"/orders/:id\", {handler_source})"),
        ),
    );
    call_fields(
        g,
        route,
        "ordersRouter.post",
        Some(&format!("ordersRouter.post(\"/orders/:id\", {handler_source});")),
        None,
    );
    calls(g, route, unknown(Reason::UnknownReceiver));
    argument(g, route, 0, "\"/orders/:id\"", None);
    argument(g, route, 1, handler_source, None);

    let handler = g.inline(route, Inline::Callback, g.find_in(route, handler_source));
    let head = g.find_in(handler, "async (req, res) =>");
    let body = g.find_in(handler, "{\n  const order");
    let body = ByteRange {
        start: body.start,
        end: g.range(handler).end,
    };
    let node = g.node_mut(handler);
    node.head = Some(head);
    node.body = Some(body);
    node.flags.insert(Flag::Async);
    parameter(g, handler, 0, "req", "req");
    parameter(g, handler, 1, "res", "res");
    signature(g, handler);

    let load = g.call(
        handler,
        &name("loadOrder"),
        g.find_in(handler, "loadOrder(req.params.id)"),
    );
    call_fields(
        g,
        load,
        "loadOrder",
        Some("const order: Order = await loadOrder(req.params.id);"),
        Some("order"),
    );
    g.node_mut(load).flags.insert(Flag::Awaited);
    argument(g, load, 0, "req.params.id", Some(Pairing::Parameter(0)));
    let annotation = g.find_in(handler, "Order");
    g.link(
        handler,
        LinkKind::Types,
        annotation,
        "Order",
        project(&g.key(interface)),
    );

    let json = g.call(
        handler,
        &name("res").member("status").call().member("json"),
        g.find_in(handler, "res.status(200).json(order)"),
    );
    call_fields(
        g,
        json,
        "res.status(200).json",
        Some("res.status(200).json(order);"),
        None,
    );
    calls(g, json, unknown(Reason::Parameter));
    argument(g, json, 0, "order", None);
    let status = g.call(
        json,
        &name("res").member("status"),
        g.find_in(json, "res.status(200)"),
    );
    call_fields(g, status, "res.status", None, None);
    calls(g, status, unknown(Reason::Parameter));
    argument(g, status, 0, "200", None);

    let load_order = g.def(
        module,
        Form::Function,
        "loadOrder",
        g.find(
            module,
            "async function loadOrder(id: string): Promise<Order> {\n  return { id, total: 0 };\n}",
        ),
    );
    let head = g.find_in(load_order, "async function loadOrder(id: string): Promise<Order>");
    let body = g.find_in(load_order, "{\n  return { id, total: 0 };\n}");
    let node = g.node_mut(load_order);
    node.head = Some(head);
    node.body = Some(body);
    node.flags.insert(Flag::Async);
    parameter(g, load_order, 0, "id: string", "id");
    let returns = g.written_from(load_order, "Promise<Order>", g.range(load_order).start);
    let promised = g.find_from(load_order, "Order", returns.range.start);
    g.types(load_order).signatures.push(Signature {
        node: load_order.local_id(),
        returns: Some(returns),
    });
    g.link(
        load_order,
        LinkKind::Types,
        promised,
        "Order",
        project(&g.key(interface)),
    );
    exit(
        g,
        load_order,
        Form::Return,
        "return { id, total: 0 };",
        Some("{ id, total: 0 }"),
    );

    // `loadOrder` is declared after its call, so the call's link is added once its key exists.
    let head = g.node_mut(load).head.expect("filled above");
    g.link(
        load,
        LinkKind::Calls,
        head,
        "loadOrder",
        project(&g.key(load_order)),
    )
    .bound = Some(0);
}

/// Adds a Python function spanning `source`, with its signature range (`head`), its body (the
/// suite after the head's `:`), and an empty signature record.
fn function(g: &mut GraphBuilder, parent: NodeRef, name: &str, source: &str, head: &str) -> NodeRef {
    let range = g.find_in(parent, source);
    let function = g.def(parent, Form::Function, name, range);
    let head = g.find_in(function, head);
    let after_head = g.source(
        function,
        ByteRange::new(head.end, range.end).expect("the head ends inside"),
    );
    let gap = after_head.len() - after_head.trim_start_matches([':', '\n', ' ']).len();
    let body = ByteRange {
        start: head.end + u32::try_from(gap).expect("test offsets fit in u32"),
        end: range.end,
    };
    let node = g.node_mut(function);
    node.head = Some(head);
    node.body = Some(body);
    signature(g, function);
    function
}

/// Adds a part of `form` spanning `source`, with its test or clause and its suite.
fn part(
    g: &mut GraphBuilder,
    parent: NodeRef,
    form: Form,
    source: &str,
    head: Option<&str>,
    body: &str,
) -> NodeRef {
    let part = g.block(parent, form, g.find_in(parent, source));
    let head = head.map(|head| g.find_in(part, head));
    let body = g.find_in(part, body);
    let node = g.node_mut(part);
    node.head = head;
    node.body = Some(body);
    part
}

/// Adds an exit of `form` spanning its whole statement `source`, with its value's range.
fn exit(g: &mut GraphBuilder, parent: NodeRef, form: Form, source: &str, value: Option<&str>) -> NodeRef {
    let range = g.find_in(parent, source);
    let exit = g.block(parent, form, range);
    let value = value.map(|value| g.find_from(exit, value, range.start + form.as_str().len() as u32));
    let node = g.node_mut(exit);
    node.stmt = Some(range);
    node.value = value;
    exit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{Key, ParsedKey};
    use crate::model::{Graph, Node, owner};
    use crate::vocab::{NodeKind, Outcome};

    /// The node `key` names in `graph`, if there is one.
    fn named<'g>(graph: &'g Graph, key: &Key) -> Option<&'g Node> {
        let Ok(ParsedKey::Block(block)) = key.parse() else {
            return None;
        };
        let file = graph.file(block.path().as_str())?;
        Some(&file.nodes[file.local_id(key.as_str())? as usize])
    }

    #[test]
    fn place_order_is_the_designs_21_blocks() {
        let graph = shop().build();
        let file = graph.file("shop/service.py").unwrap();
        let tree: Vec<String> = file
            .nodes
            .iter()
            .filter(|node| node.key.as_str().starts_with("shop/service.py#place_order"))
            .map(|node| {
                let Ok(ParsedKey::Block(block)) = node.key.parse() else {
                    panic!("{} is not a block key", node.key);
                };
                let segments = block.segments();
                let step = segments.last().unwrap();
                format!("{}{step} {}", "  ".repeat(segments.len() - 1), node.form)
            })
            .collect();
        assert_eq!(
            tree,
            [
                "place_order function",
                "  call:load_order call",
                "  if[0] if",
                "    then then",
                "      raise[0] raise",
                "        call:ValueError call",
                "    elif[0] elif",
                "      return[0] return",
                "  call:sum call",
                "  try[0] try",
                "    body body",
                "      call:charge_card call",
                "    except[0] except",
                "      call:logger.warning call",
                "      return[0] return",
                "        call:mark_failed call",
                "  call:save_order call",
                "  for[0] for",
                "    body body",
                "      call:reserve_stock call",
                "  return[0] return",
            ]
        );
        assert_eq!(tree.len(), 21);
        let node = |key: &str| {
            let id = file.local_id(key).unwrap_or_else(|| panic!("{key}"));
            &file.nodes[id as usize]
        };
        let charge = node("shop/service.py#place_order/try[0]/body/call:charge_card");
        assert_eq!(charge.binds.as_deref(), Some("receipt"));
        let text = &SERVICE_PY[charge.stmt.unwrap().as_usize()];
        assert_eq!(text, "receipt = charge_card(order[\"card\"], total)");
        assert_eq!(charge.lines.start, 19);
        let place_order = node("shop/service.py#place_order");
        assert_eq!(place_order.lines.start, 9);
        assert_eq!(place_order.lines.end, 28);
        assert_eq!(
            &SERVICE_PY[place_order.doc.unwrap().as_usize()],
            "\"\"\"Charge an order and save it.\"\"\""
        );
        let failed = node("shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed");
        assert!(failed.flags.contains(Flag::ExitValue));
        let ret = node("shop/service.py#place_order/return[0]");
        assert_eq!(ret.lines.start, 28);
        assert_eq!(&SERVICE_PY[ret.value.unwrap().as_usize()], "order");
        let except = node("shop/service.py#place_order/try[0]/except[0]");
        assert_eq!(
            &SERVICE_PY[except.head.unwrap().as_usize()],
            "except PaymentDeclined as error"
        );
    }

    #[test]
    fn every_node_is_keyed_and_placed_in_preorder() {
        let graph = shop().build();
        for file in &graph.files {
            for (id, node) in file.nodes.iter().enumerate() {
                let parsed = node.key.parse().unwrap_or_else(|error| panic!("{error}"));
                let ParsedKey::Block(block) = parsed else {
                    panic!("{} is not a block key", node.key);
                };
                assert_eq!(block.path(), &file.file.path);
                assert_eq!(block.to_key(), node.key);
                match node.parent {
                    None => assert_eq!(id, 0),
                    Some(parent) => {
                        assert!((parent as usize) < id);
                        let outer = &file.nodes[parent as usize];
                        assert!(
                            outer.range.contains(node.range),
                            "{} is outside its parent",
                            node.key
                        );
                    }
                }
                for range in [node.head, node.body, node.value].into_iter().flatten() {
                    assert!(node.range.contains(range), "{} has a range outside it", node.key);
                }
                if let Some(stmt) = node.stmt {
                    assert!(
                        stmt.contains(node.range),
                        "{}'s statement doesn't contain it",
                        node.key
                    );
                }
                assert!(node.flags.iter().all(|flag| flag.applies_to(node.form)));
                assert!(node.form.allowed_in(file.file.language), "{}", node.key);
                if id > 0 {
                    assert!(owner(&file.nodes, id as u32).is_some());
                }
            }
            for link in &file.links {
                if let LinkTarget::Project(key) = &link.to {
                    let target = named(&graph, key).unwrap_or_else(|| panic!("{key} exists"));
                    assert!(target.kind().is_definition());
                }
                let earlier = file.links.iter().take_while(|other| *other != link);
                let earlier = earlier.filter(|other| other.from == link.from).count();
                assert_eq!(
                    earlier, link.seq as usize,
                    "links of one node are numbered in order"
                );
            }
        }
    }

    #[test]
    fn links_reach_every_outcome_and_kind() {
        let graph = shop().build();
        let links: Vec<_> = graph.files.iter().flat_map(|file| &file.links).collect();
        for outcome in Outcome::ALL {
            assert!(
                links.iter().any(|link| link.to.outcome() == *outcome),
                "{outcome}"
            );
        }
        for kind in LinkKind::ALL {
            assert!(links.iter().any(|link| link.kind == *kind), "{kind}");
        }
        let kinds: Vec<NodeKind> = graph
            .files
            .iter()
            .flat_map(|file| &file.nodes)
            .map(|node| node.kind())
            .collect();
        for kind in [
            NodeKind::Module,
            NodeKind::Class,
            NodeKind::Function,
            NodeKind::Type,
            NodeKind::Decision,
            NodeKind::Loop,
            NodeKind::Try,
            NodeKind::Part,
            NodeKind::Call,
            NodeKind::Exit,
        ] {
            assert!(kinds.contains(&kind), "{kind}");
        }
    }

    #[test]
    fn the_route_handler_and_other_keys_match_the_designs() {
        let graph = shop().build();
        let orders = graph.file("src/orders.ts").unwrap();
        let keys: Vec<&str> = orders.nodes.iter().map(|node| node.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "src/orders.ts#",
                "src/orders.ts#Order",
                "src/orders.ts#/call:Router",
                "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")",
                ORDERS_HANDLER,
                "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]/call:loadOrder",
                "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]/call:res.status().json",
                "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]/call:res.status().json/call:res.status",
                "src/orders.ts#loadOrder",
                "src/orders.ts#loadOrder/return[0]",
            ]
        );
        let store = graph.file("shop/store.py").unwrap();
        assert!(store.local_id("shop/store.py#/call:dict").is_some());
        assert!(
            store
                .local_id("shop/store.py#reserve_stock/for[0]/call:sorted/callback[0]")
                .is_some()
        );
        assert!(
            store
                .local_id("shop/store.py#save_all/call:list/call:map")
                .is_some()
        );
        let payments = graph.file("shop/payments.py").unwrap();
        assert!(
            payments
                .local_id("shop/payments.py#retry/return[0]/lambda[0]")
                .is_some()
        );
        let decorator = payments.local_id("shop/payments.py#/call:retry").unwrap();
        let decorator = &payments.nodes[decorator as usize];
        assert_eq!(decorator.role, Some(CallRole::Decorator));
        let charge_card = payments.local_id("shop/payments.py#charge_card").unwrap();
        assert_eq!(decorator.r#for, Some(charge_card));
        assert!(
            payments
                .local_id("shop/payments.py#charge_card/if[0]/then/raise[0]/call:PaymentDeclined")
                .is_some()
        );
    }

    #[test]
    fn the_app_lists_its_entrances_with_their_states() {
        let graph = shop().build();
        let [app] = graph.apps.as_slice() else {
            panic!("one app");
        };
        assert_eq!(app.key.as_str(), "entrance:shop");
        let entrances: Vec<(&str, Vec<TargetState>)> = app
            .entrances
            .iter()
            .map(|entrance| {
                let states = entrance.code.iter().map(|target| target.state).collect();
                (entrance.key.as_str(), states)
            })
            .collect();
        assert_eq!(
            entrances,
            [
                ("entrance:shop/place-an-order", vec![TargetState::Found]),
                ("entrance:shop/orders-api", vec![TargetState::Found]),
                ("entrance:shop/refund-an-order", vec![TargetState::Missing]),
                (
                    "entrance:shop/charge-a-card",
                    vec![TargetState::NotAStart, TargetState::Found]
                ),
            ]
        );
        // Every found target names a start, and every other target's state matches the graph.
        for target in app
            .code
            .iter()
            .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
        {
            let expected = match named(&graph, &target.code).map(Node::kind) {
                None => TargetState::Missing,
                Some(kind) if kind.is_definition() => TargetState::Found,
                Some(_) => TargetState::NotAStart,
            };
            assert_eq!(target.state, expected, "{}", target.code);
        }
        assert_eq!(graph.inputs.len(), 1);
    }
}
