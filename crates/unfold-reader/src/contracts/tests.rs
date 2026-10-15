//! Contracts on the sample graph (with its sources written out) and on the playgrounds' goldens.

use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, builtin, project, sample, unknown};
use unfold_core::vocab::{Flag, Form, Language, LinkKind, Outcome, Reason};
use unfold_core::{Graph, Key, RepoPath};

use super::*;
use crate::testing::Playground;

fn contract(playground: &Playground, key: &str) -> CallContract {
    let reader = playground.reader();
    let mut facts = Facts::new(&reader, &playground.root);
    call_contract(&mut facts, key)
        .unwrap()
        .unwrap_or_else(|| panic!("{key} is a call"))
}

fn key(text: &str) -> Key {
    Key::from_text(text)
}

/// Each span's text and what it reaches.
fn spans(text: &TypeText) -> Vec<(String, Outcome, Option<String>)> {
    let units: Vec<u16> = text.text.encode_utf16().collect();
    text.spans
        .iter()
        .map(|span| {
            let name = String::from_utf16(&units[span.start as usize..span.end as usize]).unwrap();
            (
                name,
                span.to.outcome,
                span.to.key.as_ref().map(ToString::to_string),
            )
        })
        .collect()
}

#[test]
fn a_call_with_no_annotations_shows_them_missing_and_its_raise_caught() {
    let playground = Playground::built(&sample::shop());
    let contract = contract(
        &playground,
        "shop/service.py#place_order/try[0]/body/call:charge_card",
    );
    assert_eq!(&*contract.text, "charge_card");
    assert_eq!(contract.callee.outcome, Outcome::Project);
    assert_eq!(contract.callee.key, Some(key("shop/payments.py#charge_card")));
    assert_eq!(contract.line, 19);
    let sends: Vec<(Option<&str>, Option<&str>, bool)> = contract
        .sends
        .iter()
        .map(|send| {
            let parameter = send.parameter.as_ref().unwrap();
            (
                send.argument
                    .as_ref()
                    .and_then(|argument| argument.text.as_deref()),
                parameter.name.as_deref(),
                parameter.ty.is_some(),
            )
        })
        .collect();
    assert_eq!(
        sends,
        [
            (Some("order[\"card\"]"), Some("card"), false),
            (Some("total"), Some("amount"), false)
        ]
    );
    assert_eq!(
        contract.returns,
        ContractReturn {
            ty: None,
            awaited: false,
            lands: Landing::Binds {
                target: "receipt".to_owned()
            },
        }
    );
    let [raise] = contract.raises.as_slice() else {
        panic!("one raise: {:?}", contract.raises);
    };
    assert_eq!(
        raise.exit,
        key("shop/payments.py#charge_card/if[0]/then/raise[0]")
    );
    assert_eq!(raise.line, 19);
    assert_eq!(
        raise.value.as_deref(),
        Some("PaymentDeclined(\"nothing to charge\")")
    );
    let ty = raise.ty.as_ref().unwrap();
    assert_eq!(
        spans(ty),
        [(
            "PaymentDeclined".to_owned(),
            Outcome::Project,
            Some("shop/payments.py#PaymentDeclined".to_owned())
        )]
    );
    assert_eq!(
        raise.caught_by,
        Some(key("shop/service.py#place_order/try[0]/except[0]"))
    );
}

#[test]
fn where_a_value_lands() {
    let playground = Playground::built(&sample::shop());
    let lands = |key: &str| contract(&playground, key).returns.lands;
    assert_eq!(
        lands("shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed"),
        Landing::Exit {
            exit: key("shop/service.py#place_order/try[0]/except[0]/return[0]"),
            form: Form::Return,
        }
    );
    assert_eq!(
        lands("shop/store.py#save_all/call:list/call:map"),
        Landing::Argument {
            call: key("shop/store.py#save_all/call:list"),
            index: 0,
        }
    );
    let handler = sample::ORDERS_HANDLER;
    assert_eq!(
        lands(&format!("{handler}/call:res.status().json/call:res.status")),
        Landing::Receiver {
            call: key(&format!("{handler}/call:res.status().json")),
        }
    );
    assert_eq!(
        lands("shop/service.py#place_order/call:save_order"),
        Landing::Nowhere
    );

    // Awaited, with a written return type naming a project type.
    let load = contract(&playground, &format!("{handler}/call:loadOrder"));
    assert!(load.returns.awaited);
    assert_eq!(
        load.returns.lands,
        Landing::Binds {
            target: "order".to_owned()
        }
    );
    let ty = load.returns.ty.as_ref().unwrap();
    assert_eq!(ty.text, "Promise<Order>");
    assert_eq!(
        spans(ty),
        [(
            "Order".to_owned(),
            Outcome::Project,
            Some("src/orders.ts#Order".to_owned())
        )]
    );
    let parameter = load.sends[0].parameter.as_ref().unwrap();
    assert_eq!(parameter.ty.as_ref().map(|ty| ty.text.as_str()), Some("string"));
    assert_eq!(
        load.sends[0].argument.as_ref().unwrap().text.as_deref(),
        Some("req.params.id")
    );

    // A builtin call sends its arguments, fills no known parameter, and raises nothing known.
    let map = contract(&playground, "shop/store.py#save_all/call:list/call:map");
    assert_eq!(map.callee.outcome, Outcome::Builtin);
    assert_eq!(map.sends.len(), 2);
    assert!(
        map.sends
            .iter()
            .all(|send| send.parameter.is_none() && send.unpaired.is_none())
    );
    assert!(map.raises.is_empty());
    assert!(map.returns.ty.is_none());
}

#[test]
fn a_construction_returns_its_class_and_an_unfilled_parameter_is_listed() {
    let py = Playground::load("py-orders");
    let built = contract(
        &py,
        "src/orders_playground/services/order_service.py#/call:OrderService",
    );
    let ty = built.returns.ty.as_ref().unwrap();
    assert_eq!(
        spans(ty),
        [(
            "OrderService".to_owned(),
            Outcome::Project,
            Some("src/orders_playground/services/order_service.py#OrderService".to_owned())
        )]
    );
    let names: Vec<Option<&str>> = built
        .sends
        .iter()
        .map(|send| {
            send.parameter
                .as_ref()
                .and_then(|parameter| parameter.name.as_deref())
        })
        .collect();
    assert_eq!(names, [Some("repo"), Some("payments")]);

    let ts = Playground::load("ts-orders");
    let info = contract(&ts, "src/cli.ts#main/switch[0]/case[5]/call:logger.info");
    let [sent, unfilled] = info.sends.as_slice() else {
        panic!("an argument and an unfilled parameter: {:?}", info.sends);
    };
    assert!(sent.argument.is_some());
    assert!(unfilled.argument.is_none());
    let parameter = unfilled.parameter.as_ref().unwrap();
    assert_eq!(parameter.name.as_deref(), Some("details"));
    assert!(parameter.optional);
    assert_eq!(parameter.ty.as_ref().map(|ty| ty.text.as_str()), Some("unknown"));
}

#[test]
fn raises_are_caught_only_by_handlers_that_name_them() {
    let py = Playground::load("py-orders");
    let caught = |key: &str| -> Vec<Option<String>> {
        contract(&py, key)
            .raises
            .iter()
            .map(|raise| raise.caught_by.as_ref().map(ToString::to_string))
            .collect()
    };
    // `validate_order` raises `ValueError` seven times; `except ValueError` catches them.
    let import = "src/orders_playground/jobs/import_orders.py#import_orders/for[0]/body/try[0]";
    let handler = Some(format!("{import}/except[0]"));
    assert_eq!(
        caught(&format!("{import}/body/call:validate_order")),
        vec![handler; 7]
    );
    let body = "src/orders_playground/routes/middleware.py#validate_body/try[0]";
    assert_eq!(
        caught(&format!("{body}/body/call:validate_order")),
        vec![Some(format!("{body}/except[0]")); 7]
    );
    assert_eq!(
        caught("src/orders_playground/services/order_service.py#OrderService.place/call:validate_order"),
        vec![None; 7]
    );
    let validate = contract(&py, &format!("{import}/body/call:validate_order"));
    let raise = &validate.raises[0];
    assert_eq!(
        raise.value.as_deref(),
        Some("ValueError(\"Order must be an object\")")
    );
    assert_eq!(
        spans(raise.ty.as_ref().unwrap()),
        [("ValueError".to_owned(), Outcome::Builtin, None)]
    );
    let returns = validate.returns.ty.as_ref().unwrap();
    assert_eq!(
        spans(returns),
        [(
            "OrderDraft".to_owned(),
            Outcome::Project,
            Some("src/orders_playground/types.py#OrderDraft".to_owned())
        )]
    );
    let parameter = validate.sends[0].parameter.as_ref().unwrap();
    assert_eq!(
        spans(parameter.ty.as_ref().unwrap()),
        [("object".to_owned(), Outcome::Builtin, None)]
    );
    assert_eq!(
        validate.returns.lands,
        Landing::Binds {
            target: "draft".to_owned()
        }
    );

    // A TypeScript `catch` catches everything. `validateOrder` throws four times itself; the
    // other four throws are in the callback it passes to `items.map`, so they aren't direct.
    let ts = Playground::load("ts-orders");
    let attempt = "src/jobs/import-orders.ts#importOrders/for[0]/body/try[0]";
    let validate = contract(&ts, &format!("{attempt}/body/call:validateOrder"));
    assert_eq!(validate.raises.len(), 4);
    assert!(
        validate
            .raises
            .iter()
            .all(|raise| raise.caught_by == Some(key(&format!("{attempt}/catch"))))
    );
    let process = contract(&ts, "src/jobs/process-order.ts#processOrder/call:validateOrder");
    assert!(process.raises.iter().all(|raise| raise.caught_by.is_none()));
    // `withRetry` rethrows what it catches with a bare `throw error` and throws after its loop:
    // the rethrow is a value, so no class is named.
    let retry = contract(
        &ts,
        "src/queues/email-worker.ts#/call:Worker(\"emails\")/callback[0]/call:withRetry",
    );
    let values: Vec<(Option<&str>, bool)> = retry
        .raises
        .iter()
        .map(|raise| (raise.value.as_deref(), raise.ty.is_some()))
        .collect();
    assert_eq!(
        values,
        [
            (
                Some("new RangeError(\"Attempts must be a positive integer\")"),
                true
            ),
            (Some("error"), false),
            (Some("new Error(\"Retry attempts exhausted\")"), true),
        ]
    );
}

#[test]
fn a_changed_caller_shows_no_argument_text() {
    let playground = Playground::built(&sample::shop());
    let service = playground.root.join("shop/service.py");
    let text = std::fs::read_to_string(&service).unwrap();
    std::fs::write(&service, text.replace("total)", "total )")).unwrap();
    let contract = contract(
        &playground,
        "shop/service.py#place_order/try[0]/body/call:charge_card",
    );
    assert!(
        contract
            .sends
            .iter()
            .all(|send| send.argument.as_ref().unwrap().text.is_none())
    );
    // The callee's file didn't change: its raise still shows its value.
    assert!(contract.raises[0].value.is_some());
    // Facts are read from the store; a key that is no call has no contract.
    let reader = playground.reader();
    let mut facts = Facts::new(&reader, &playground.root);
    assert_eq!(
        call_contract(&mut facts, "shop/service.py#place_order").unwrap(),
        None
    );
    assert_eq!(call_contract(&mut facts, "shop/service.py#absent").unwrap(), None);
}

fn definition(playground: &Playground, key: &str) -> TypeDefinition {
    let reader = playground.reader();
    let mut facts = Facts::new(&reader, &playground.root);
    type_definition(&mut facts, key)
        .unwrap()
        .unwrap_or_else(|| panic!("{key} is a class or type"))
}

fn member_names(members: &[TypeMember]) -> Vec<&str> {
    members
        .iter()
        .filter_map(|member| member.name.as_deref())
        .collect()
}

#[test]
fn type_definitions_show_their_own_and_inherited_members() {
    let ts = Playground::load("ts-orders");
    let order = definition(&ts, "src/types.ts#Order");
    assert_eq!(order.kind, TypeKind::Interface);
    assert_eq!(order.name, "Order");
    assert_eq!(order.line, 16);
    assert_eq!(member_names(&order.members), ["id", "status", "createdAt"]);
    let status = order.members[1].ty.as_ref().unwrap();
    assert_eq!(
        spans(status),
        [(
            "OrderStatus".to_owned(),
            Outcome::Project,
            Some("src/types.ts#OrderStatus".to_owned())
        )]
    );
    let [draft] = order.inherited.as_slice() else {
        panic!("one base: {:?}", order.inherited);
    };
    assert_eq!(draft.from.text, "OrderDraft");
    assert_eq!(member_names(&draft.members), ["customerId", "email", "items"]);
    assert_eq!(order.uses, definition(&ts, "src/types.ts#Order").uses);

    let alias = definition(&ts, "src/types.ts#OrderStatus");
    assert_eq!(alias.kind, TypeKind::TypeAlias);
    assert!(alias.members.is_empty());
    assert_eq!(
        alias.aliased.as_ref().map(|ty| ty.text.as_str()),
        Some("typeof ORDER_STATUSES[number]")
    );

    let py = Playground::load("py-orders");
    let combined = definition(&py, "src/orders_playground/features/inheritance.py#Combined");
    assert_eq!(combined.kind, TypeKind::Class);
    let bases: Vec<&str> = combined
        .inherited
        .iter()
        .map(|group| group.from.text.as_str())
        .collect();
    // Depth first in written order, each class once: the diamond's `Root` comes once.
    assert_eq!(bases, ["Left", "Root", "Right"]);

    let reader = ts.reader();
    let mut facts = Facts::new(&reader, &ts.root);
    assert_eq!(
        type_definition(&mut facts, "src/routes/orders.ts#createOrder").unwrap(),
        None
    );
}

#[test]
fn type_uses_come_a_page_at_a_time() {
    let ts = Playground::load("ts-orders");
    let reader = ts.reader();
    let first = type_uses(&reader, &ts.root, "src/types.ts#OrderDraft", None, 4)
        .unwrap()
        .unwrap();
    assert_eq!(first.total, 6);
    assert_eq!(first.places.len(), 4);
    let rest = type_uses(
        &reader,
        &ts.root,
        "src/types.ts#OrderDraft",
        first.next.as_ref(),
        4,
    )
    .unwrap()
    .unwrap();
    assert!(rest.next.is_none());
    let places: Vec<(&str, &str, UseKind)> = first
        .places
        .iter()
        .chain(&rest.places)
        .map(|place| (place.owner.as_str(), place.label.as_str(), place.kind))
        .collect();
    assert_eq!(
        places,
        [
            (
                "src/lib/validate.ts#validateOrder",
                "validateOrder",
                UseKind::Annotation
            ),
            (
                "src/routes/orders.ts#createOrder",
                "createOrder",
                UseKind::Annotation
            ),
            (
                "src/services/payment.ts#PaymentProvider.charge",
                "charge",
                UseKind::Annotation
            ),
            (
                "src/services/payment.ts#StripeProvider.charge",
                "charge",
                UseKind::Annotation
            ),
            ("src/types.ts#Order", "Order", UseKind::Extends),
            ("src/types.ts#OrderJob", "OrderJob", UseKind::Annotation),
        ]
    );
    assert_eq!(first.places[0].line, 3);
    assert_eq!(definition(&ts, "src/types.ts#OrderDraft").uses, 6);

    // An `except` clause names a type for the definition its part belongs to.
    let built = Playground::built(&sample::shop());
    let reader = built.reader();
    let uses = type_uses(&reader, &built.root, "shop/payments.py#PaymentDeclined", None, 10)
        .unwrap()
        .unwrap();
    let [place] = uses.places.as_slice() else {
        panic!("one use: {:?}", uses.places);
    };
    assert_eq!(place.owner, key("shop/service.py#place_order"));
    assert_eq!(place.kind, UseKind::Handler);
    assert_eq!(place.line, 20);
    assert!(
        type_uses(&reader, &built.root, "shop/service.py#place_order", None, 10)
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_in_memory_graph_is_a_fact_source() {
    let graph: Graph = sample::shop().build();
    let source: &dyn FactSource = &graph;
    assert_eq!(
        source.locate("shop/payments.py#charge_card").unwrap(),
        Some((RepoPath::new("shop/payments.py").unwrap(), 7))
    );
    assert_eq!(source.type_use_count("src/orders.ts#Order").unwrap(), 2);
    assert!(source.file_graph("shop/absent.py").unwrap().is_none());
}

/// Only a clause that writes no type expression catches everything: `except ERRORS[0]:` names
/// a type the graph can't follow, so it catches nothing proven, and the bare `except:` after it
/// catches the raise.
#[test]
fn only_a_bare_except_catches_everything() {
    const CODE: &str = "def risky():\n    raise Oops()\n\n\ndef guarded():\n    try:\n        risky()\n    except ERRORS[0]:\n        pass\n    except:\n        pass\n";
    let mut g = GraphBuilder::new();
    let file = g.file("g.py", Language::Python, CODE);
    let risky = g.def(
        file,
        Form::Function,
        "risky",
        g.find(file, "def risky():\n    raise Oops()"),
    );
    let raise = g.block(risky, Form::Raise, g.find(file, "raise Oops()"));
    g.node_mut(raise).stmt = Some(g.find(file, "raise Oops()"));
    let oops = g.construction(raise, &Callee::name("Oops"), g.find(file, "Oops()"));
    g.node_mut(oops).flags.insert(Flag::ExitValue);
    let head = g.find(file, "Oops");
    g.link(oops, LinkKind::Calls, head, "Oops", unknown(Reason::NotFound));

    let guarded_text = &CODE[CODE.find("def guarded").unwrap()..CODE.len() - 1];
    let guarded = g.def(file, Form::Function, "guarded", g.find(file, guarded_text));
    let attempt = g.block(
        guarded,
        Form::Try,
        g.find_in(guarded, &guarded_text[guarded_text.find("try").unwrap()..]),
    );
    let body = g.block(attempt, Form::Body, g.find_in(guarded, "risky()"));
    let call = g.call(body, &Callee::name("risky"), g.find_in(guarded, "risky()"));
    g.node_mut(call).stmt = Some(g.find_in(guarded, "risky()"));
    let head = g.find_in(guarded, "risky");
    g.link(call, LinkKind::Calls, head, "risky", project(&key("g.py#risky")));
    let named = g.block(
        attempt,
        Form::Except,
        g.find_in(guarded, "except ERRORS[0]:\n        pass"),
    );
    g.node_mut(named).head = Some(g.find_in(guarded, "except ERRORS[0]"));
    let bare_range = g.find_from(file, "except:\n        pass", g.range(named).end);
    let bare = g.block(attempt, Form::Except, bare_range);
    g.node_mut(bare).head = Some(g.find_from(file, "except", bare_range.start));

    let playground = Playground::built(&g);
    let caught: Vec<Option<Key>> = contract(&playground, "g.py#guarded/try[0]/body/call:risky")
        .raises
        .into_iter()
        .map(|raise| raise.caught_by)
        .collect();
    assert_eq!(caught, [Some(key("g.py#guarded/try[0]/except[1]"))]);
}

/// `e.py`: project exceptions derived from Python's builtins, a callee that raises them and
/// builtins, and two callers whose handlers name a project class, `Exception`, `LookupError`,
/// and `IOError` (which names `OSError`).
fn builtin_exceptions() -> GraphBuilder {
    const CODE: &str = r#"class AppError(Exception):
    pass


class NotFound(AppError):
    pass


class Missing(KeyError):
    pass


def finder():
    raise NotFound()
    raise ValueError("bad")
    raise KeyError("k")
    raise Missing()
    raise FileNotFoundError("f")


def caller_a():
    try:
        finder()
    except AppError:
        pass
    except Exception:
        pass


def caller_b():
    try:
        finder()
    except LookupError:
        pass
    except IOError:
        pass
"#;
    let builtins = || builtin("python:builtins");
    let mut g = GraphBuilder::new();
    let file = g.file("e.py", Language::Python, CODE);
    for (name, base, to) in [
        ("AppError", "Exception", builtins()),
        ("NotFound", "AppError", project(&key("e.py#AppError"))),
        ("Missing", "KeyError", builtins()),
    ] {
        let written = format!("class {name}({base}):\n    pass");
        let class = g.def(file, Form::Class, name, g.find(file, &written));
        let head = g.find_in(class, base);
        g.link(class, LinkKind::Extends, head, base, to);
    }
    let text = |from: &str, to: &str| &CODE[CODE.find(from).unwrap()..CODE.find(to).unwrap()];
    let finder = g.def(
        file,
        Form::Function,
        "finder",
        g.find(file, text("def finder", "\n\n\ndef caller_a")),
    );
    for (class, written) in [
        ("NotFound", "raise NotFound()"),
        ("ValueError", "raise ValueError(\"bad\")"),
        ("KeyError", "raise KeyError(\"k\")"),
        ("Missing", "raise Missing()"),
        ("FileNotFoundError", "raise FileNotFoundError(\"f\")"),
    ] {
        let raise = g.block(finder, Form::Raise, g.find_in(finder, written));
        g.node_mut(raise).stmt = Some(g.range(raise));
        let call = g.call(
            raise,
            &Callee::name(class),
            g.find_in(raise, &written["raise ".len()..]),
        );
        g.node_mut(call).flags.insert(Flag::ExitValue);
        let to = if class == "NotFound" || class == "Missing" {
            project(&key(&format!("e.py#{class}")))
        } else {
            builtins()
        };
        let head = g.find_in(call, class);
        g.link(call, LinkKind::Calls, head, class, to);
    }
    for (name, handlers) in [
        (
            "caller_a",
            [
                ("AppError", project(&key("e.py#AppError"))),
                ("Exception", builtins()),
            ],
        ),
        ("caller_b", [("LookupError", builtins()), ("IOError", builtins())]),
    ] {
        let end = if name == "caller_a" {
            "\n\n\ndef caller_b"
        } else {
            "\n"
        };
        let written = &CODE[CODE.find(&format!("def {name}")).unwrap()..];
        let written = &written[..written.rfind(end).unwrap()];
        let caller = g.def(file, Form::Function, name, g.find(file, written));
        let attempt = g.block(
            caller,
            Form::Try,
            g.find_in(caller, &written[written.find("try").unwrap()..]),
        );
        let body = g.block(attempt, Form::Body, g.find_in(caller, "finder()"));
        let call = g.call(body, &Callee::name("finder"), g.find_in(caller, "finder()"));
        g.node_mut(call).stmt = Some(g.range(call));
        let head = g.find_in(call, "finder");
        g.link(
            call,
            LinkKind::Calls,
            head,
            "finder",
            project(&key("e.py#finder")),
        );
        for (class, to) in handlers {
            let clause = format!("except {class}");
            let part = g.block(
                attempt,
                Form::Except,
                g.find_in(caller, &format!("{clause}:\n        pass")),
            );
            g.node_mut(part).head = Some(g.find_in(part, &clause));
            let named = g.find_in(part, class);
            g.link(part, LinkKind::Types, named, class, to);
        }
    }
    g
}

/// A handler catches what derives from the class it names, through project classes and
/// Python's builtin exception classes alike: `except Exception` catches `ValueError` and a
/// project class whose bases reach `Exception`, `except LookupError` catches `KeyError` and a
/// project class deriving from it, and `except IOError` catches `FileNotFoundError`.
#[test]
fn handlers_catch_through_builtin_exception_bases() {
    let playground = Playground::built(&builtin_exceptions());
    let caught = |caller: &str| -> Vec<Option<String>> {
        contract(&playground, &format!("e.py#{caller}/try[0]/body/call:finder"))
            .raises
            .iter()
            .map(|raise| raise.caught_by.as_ref().map(ToString::to_string))
            .collect()
    };
    let a = |n: u32| Some(format!("e.py#caller_a/try[0]/except[{n}]"));
    assert_eq!(caught("caller_a"), [a(0), a(1), a(1), a(1), a(1)]);
    let b = |n: u32| Some(format!("e.py#caller_b/try[0]/except[{n}]"));
    assert_eq!(caught("caller_b"), [None, None, b(0), b(0), b(1)]);
}
