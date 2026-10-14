//! The overlay rules on a small hand-built graph and on the playgrounds' goldens.

use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, NodeRef, project};
use unfold_core::vocab::{CallRole, Form, Inline, Language, LinkKind};
use unfold_core::{Argument, FileGraph, Key};

use super::*;
use crate::testing::Playground;
use crate::tree::{FileTree, Trees};

const SOURCE: &str = "\
def f(xs):
    a()
    if ok(xs):
        b()
    else:
        return
    c()
    try:
        d()
    except E:
        raise
    for x in xs:
        g(x)
    p(1 if xs else h())
    run(lambda: k())


def g(n):
    k()


def k():
    f([])
";

fn key(text: &str) -> Key {
    Key::from_text(text)
}

fn keys(texts: &[&str]) -> Vec<Key> {
    texts.iter().map(|text| key(text)).collect()
}

/// A call to the project function `target`.
fn project_call(builder: &mut GraphBuilder, parent: NodeRef, name: &str, written: &str, target: &str) {
    let range = builder.find_in(parent, written);
    let call = builder.call(parent, &Callee::name(name), range);
    let callee = builder.find_in(call, name);
    builder.link(call, LinkKind::Calls, callee, name, project(&key(target)));
}

/// An argument record of `call` written as `text`.
fn argument(builder: &mut GraphBuilder, call: NodeRef, text: &str) {
    let range = builder.find_in(call, text);
    builder.types(call).arguments.push(Argument {
        call: call.local_id(),
        index: 0,
        range,
        expand: None,
        keyword: None,
        pairing: None,
        bound: None,
    });
}

/// The graph of [`SOURCE`], as one file's tree.
fn tree() -> FileTree {
    let mut g = GraphBuilder::new();
    let file = g.file("m.py", Language::Python, SOURCE);
    let f_text = &SOURCE[..SOURCE.find("\n\n\n").unwrap()];
    let f = g.def(file, Form::Function, "f", g.find(file, f_text));
    let a = g.find_in(f, "a()");
    g.call(f, &Callee::name("a"), a);
    let decision = g.block(
        f,
        Form::If,
        g.find_in(f, "if ok(xs):\n        b()\n    else:\n        return"),
    );
    let then = g.block(
        decision,
        Form::Then,
        g.find_in(decision, "if ok(xs):\n        b()"),
    );
    let test = g.call(then, &Callee::name("ok"), g.find_in(then, "ok(xs)"));
    g.node_mut(test).role = Some(CallRole::Test);
    g.call(then, &Callee::name("b"), g.find_in(then, "b()"));
    let otherwise = g.block(decision, Form::Else, g.find_in(decision, "else:\n        return"));
    g.block(otherwise, Form::Return, g.find_in(otherwise, "return"));
    g.call(f, &Callee::name("c"), g.find_in(f, "c()"));
    let attempt = g.block(
        f,
        Form::Try,
        g.find_in(f, "try:\n        d()\n    except E:\n        raise"),
    );
    let body = g.block(attempt, Form::Body, g.find_in(attempt, "d()"));
    g.call(body, &Callee::name("d"), g.find_in(body, "d()"));
    let handler = g.block(
        attempt,
        Form::Except,
        g.find_in(attempt, "except E:\n        raise"),
    );
    g.block(handler, Form::Raise, g.find_in(handler, "raise"));
    let each = g.block(f, Form::For, g.find_in(f, "for x in xs:\n        g(x)"));
    let each_body = g.block(each, Form::Body, g.find_in(each, "g(x)"));
    project_call(&mut g, each_body, "g", "g(x)", "m.py#g");
    let p = g.call(f, &Callee::name("p"), g.find_in(f, "p(1 if xs else h())"));
    argument(&mut g, p, "1 if xs else h()");
    let ternary = g.block(p, Form::Ternary, g.find_in(p, "1 if xs else h()"));
    g.block(ternary, Form::Then, g.find_in(ternary, "1"));
    let ternary_else = g.block(ternary, Form::Else, g.find_in(ternary, "h()"));
    g.call(ternary_else, &Callee::name("h"), g.find_in(ternary_else, "h()"));
    let run = g.call(f, &Callee::name("run"), g.find_in(f, "run(lambda: k())"));
    argument(&mut g, run, "lambda: k()");
    let callback = g.inline(run, Inline::Callback, g.find_in(run, "lambda: k()"));
    project_call(&mut g, callback, "k", "k()", "m.py#k");

    let def_g = g.def(file, Form::Function, "g", g.find(file, "def g(n):\n    k()"));
    project_call(&mut g, def_g, "k", "k()", "m.py#k");
    let def_k = g.def(file, Form::Function, "k", g.find(file, "def k():\n    f([])"));
    project_call(&mut g, def_k, "f", "f([])", "m.py#f");
    FileTree::new(FileGraph::clone(&g.build().files[0]))
}

fn id(tree: &FileTree, text: &str) -> u32 {
    tree.id(text).unwrap_or_else(|| panic!("{text} is in the tree"))
}

#[test]
fn a_body_s_steps_leave_out_parts_and_test_calls() {
    let tree = tree();
    let body: Vec<&str> = steps(&tree, id(&tree, "m.py#f"))
        .map(|step| tree.key(step).as_str())
        .collect();
    assert_eq!(
        body,
        [
            "m.py#f/call:a",
            "m.py#f/if[0]",
            "m.py#f/call:c",
            "m.py#f/try[0]",
            "m.py#f/for[0]",
            "m.py#f/call:p",
            "m.py#f/call:run"
        ]
    );
    let then: Vec<&str> = steps(&tree, id(&tree, "m.py#f/if[0]/then"))
        .map(|step| tree.key(step).as_str())
        .collect();
    assert_eq!(then, ["m.py#f/if[0]/then/call:b"]);
    assert_eq!(steps(&tree, id(&tree, "m.py#f/if[0]")).count(), 0);
    assert_eq!(steps(&tree, id(&tree, "m.py#f/call:a")).count(), 0);
}

#[test]
fn what_each_block_allows() {
    let tree = tree();
    let allows = |text: &str| allows(&tree, id(&tree, text));
    let f = id(&tree, "m.py#f");
    assert_eq!(
        allows("m.py#f/call:a"),
        Allows {
            combine: Some(Combinable::Steps { container: f }),
            after: true,
            start: false,
            open: None,
        }
    );
    assert_eq!(
        allows("m.py#f/for[0]/body/call:g"),
        Allows {
            combine: Some(Combinable::Steps {
                container: id(&tree, "m.py#f/for[0]/body")
            }),
            after: true,
            start: false,
            open: Some(Openable::Call {
                callee: key("m.py#g")
            }),
        }
    );
    assert_eq!(
        allows("m.py#f/if[0]/then"),
        Allows {
            combine: None,
            after: false,
            start: true,
            open: None,
        }
    );
    assert_eq!(allows("m.py#f/if[0]/then/call:ok").combine, None);
    assert!(!allows("m.py#f/if[0]/then/call:ok").after);
    assert_eq!(
        allows("m.py#f/call:p/ternary[0]").combine,
        Some(Combinable::Alone)
    );
    assert!(!allows("m.py#f/call:p/ternary[0]").after);
    let callback = allows("m.py#f/call:run/callback[0]");
    assert_eq!(callback.open, Some(Openable::Callback));
    assert!(callback.start && !callback.after && callback.combine.is_none());
    assert!(allows("m.py#f").after && allows("m.py#f").start);
    assert_eq!(allows("m.py#f/call:run").open, None);
    assert!(allows("m.py#f/if[0]/else/return[0]").after);
}

#[test]
fn combined_steps_are_consecutive_steps_of_one_body_or_one_control_block() {
    let tree = tree();
    let check = |members: &[&str]| check_combined(&tree, &keys(members));
    assert_eq!(check(&["m.py#f/call:a", "m.py#f/if[0]", "m.py#f/call:c"]), Ok(()));
    assert_eq!(check(&["m.py#f/try[0]/except[0]/raise[0]"]), Ok(()));
    assert_eq!(check(&["m.py#f/call:p/ternary[0]"]), Ok(()));
    assert_eq!(check(&["m.py#f/call:p/ternary[0]/else/call:h"]), Ok(()));
    assert_eq!(check(&[]), Err(OverlayError::NoMembers));
    assert_eq!(
        check(&["m.py#f/call:a", "m.py#f/call:c"]),
        Err(OverlayError::NotConsecutive {
            key: key("m.py#f/call:c"),
            previous: key("m.py#f/call:a"),
        })
    );
    assert_eq!(
        check(&["m.py#f/if[0]", "m.py#f/call:a"]),
        Err(OverlayError::NotConsecutive {
            key: key("m.py#f/call:a"),
            previous: key("m.py#f/if[0]"),
        })
    );
    assert_eq!(
        check(&["m.py#f/call:a", "m.py#f/if[0]/then/call:b"]),
        Err(OverlayError::OtherBody {
            key: key("m.py#f/if[0]/then/call:b"),
            first: key("m.py#f/call:a"),
        })
    );
    assert_eq!(
        check(&["m.py#f/call:p/ternary[0]", "m.py#f/call:run"]),
        Err(OverlayError::NotCombinable(key("m.py#f/call:run")))
    );
    assert_eq!(
        check(&["m.py#f/if[0]/then"]),
        Err(OverlayError::NotCombinable(key("m.py#f/if[0]/then")))
    );
    assert_eq!(
        check(&["m.py#f/if[0]/then/call:ok"]),
        Err(OverlayError::NotCombinable(key("m.py#f/if[0]/then/call:ok")))
    );
    assert_eq!(
        check(&["m.py#f/call:a", "m.py#f/call:a"]),
        Err(OverlayError::Repeated(key("m.py#f/call:a")))
    );
    assert_eq!(
        check(&["m.py#f/call:absent"]),
        Err(OverlayError::Missing(key("m.py#f/call:absent")))
    );
}

#[test]
fn combined_steps_never_overlap_or_nest() {
    let tree = tree();
    let first = keys(&["m.py#f/call:a", "m.py#f/if[0]"]);
    let check = |other: &[&str]| check_disjoint(&tree, &[&first, &keys(other)]);
    assert_eq!(check(&["m.py#f/call:c"]), Ok(()));
    assert_eq!(
        check(&["m.py#f/if[0]", "m.py#f/call:c"]),
        Err(OverlayError::Overlaps(key("m.py#f/if[0]")))
    );
    assert_eq!(
        check(&["m.py#f/if[0]/then/call:b"]),
        Err(OverlayError::Nests {
            key: key("m.py#f/if[0]/then/call:b"),
            outer: key("m.py#f/if[0]"),
        })
    );
    // The outer block may come second.
    let inner = keys(&["m.py#f/if[0]/else/return[0]"]);
    assert_eq!(
        check_disjoint(&tree, &[&inner, &first]),
        Err(OverlayError::Nests {
            key: key("m.py#f/if[0]/else/return[0]"),
            outer: key("m.py#f/if[0]"),
        })
    );
    assert_eq!(check_disjoint(&tree, &[&first]), Ok(()));
}

#[test]
fn a_proposal_goes_after_a_step_or_at_the_start_of_a_body() {
    let tree = tree();
    let after = |text: &str| check_proposal(&tree, &ProposalSpot::After(key(text)));
    let start = |text: &str| check_proposal(&tree, &ProposalSpot::Start(key(text)));
    assert_eq!(after("m.py#f/call:a"), Ok(()));
    assert_eq!(after("m.py#f/if[0]/then/call:b"), Ok(()));
    assert_eq!(after("m.py#f/if[0]/else/return[0]"), Ok(()));
    assert_eq!(after("m.py#f/try[0]"), Ok(()));
    assert_eq!(after("m.py#g"), Ok(()));
    assert_eq!(
        after("m.py#f/if[0]/then"),
        Err(OverlayError::NotAfter(key("m.py#f/if[0]/then")))
    );
    assert_eq!(
        after("m.py#f/if[0]/then/call:ok"),
        Err(OverlayError::NotAfter(key("m.py#f/if[0]/then/call:ok")))
    );
    assert_eq!(
        after("m.py#f/call:p/ternary[0]"),
        Err(OverlayError::NotAfter(key("m.py#f/call:p/ternary[0]")))
    );
    assert_eq!(start("m.py#f"), Ok(()));
    assert_eq!(start("m.py#"), Ok(()));
    assert_eq!(start("m.py#f/if[0]/then"), Ok(()));
    assert_eq!(start("m.py#f/try[0]/except[0]"), Ok(()));
    assert_eq!(start("m.py#f/call:run/callback[0]"), Ok(()));
    assert_eq!(
        start("m.py#f/call:a"),
        Err(OverlayError::NoStart(key("m.py#f/call:a")))
    );
    assert_eq!(
        start("m.py#f/if[0]"),
        Err(OverlayError::NoStart(key("m.py#f/if[0]")))
    );
    assert_eq!(
        start("m.py#absent"),
        Err(OverlayError::Missing(key("m.py#absent")))
    );
}

#[test]
fn open_paths_are_project_calls_or_callbacks_each_inside_the_last() {
    let tree = tree();
    let card = key("m.py#f");
    let check = |path: &[&str]| check_open_path(&tree, &card, &keys(path));
    assert_eq!(check(&["m.py#f/for[0]/body/call:g"]), Ok(()));
    assert_eq!(
        check(&["m.py#f/for[0]/body/call:g", "m.py#g/call:k", "m.py#k/call:f"]),
        Ok(())
    );
    assert_eq!(
        check(&[
            "m.py#f/call:run/callback[0]",
            "m.py#f/call:run/callback[0]/call:k"
        ]),
        Ok(())
    );
    assert_eq!(
        check(&[
            "m.py#f/for[0]/body/call:g",
            "m.py#g/call:k",
            "m.py#k/call:f",
            "m.py#f/for[0]/body/call:g"
        ]),
        Err(OverlayError::PathLength(4))
    );
    assert_eq!(check(&[]), Err(OverlayError::PathLength(0)));
    assert_eq!(
        check(&["m.py#f/call:a"]),
        Err(OverlayError::NotOpenable(key("m.py#f/call:a")))
    );
    assert_eq!(
        check(&["m.py#g/call:k"]),
        Err(OverlayError::NotInside {
            key: key("m.py#g/call:k"),
            inside: card.clone(),
        })
    );
    assert_eq!(
        check(&["m.py#f/for[0]/body/call:g", "m.py#k/call:f"]),
        Err(OverlayError::NotInside {
            key: key("m.py#k/call:f"),
            inside: key("m.py#g"),
        })
    );
    // A call in a callback is drawn only when the callback is open.
    assert_eq!(
        check(&["m.py#f/call:run/callback[0]/call:k"]),
        Err(OverlayError::NotInside {
            key: key("m.py#f/call:run/callback[0]/call:k"),
            inside: card.clone(),
        })
    );
}

#[test]
fn a_card_opens_each_path_once_and_at_most_64() {
    let tree = tree();
    let card = key("m.py#f");
    let g = keys(&["m.py#f/for[0]/body/call:g"]);
    let callback = keys(&["m.py#f/call:run/callback[0]"]);
    fn paths<'a>(paths: &[&'a Vec<Key>]) -> Vec<&'a [Key]> {
        paths.iter().map(|path| path.as_slice()).collect()
    }
    assert_eq!(check_open_paths(&tree, &card, paths(&[&g, &callback])), Ok(()));
    assert_eq!(
        check_open_paths(&tree, &card, paths(&[&g, &callback, &g])),
        Err(OverlayError::RepeatedPath(key("m.py#f/for[0]/body/call:g")))
    );
    let many: Vec<&Vec<Key>> = std::iter::repeat_n(&g, MAX_OPEN_PATHS + 1).collect();
    assert_eq!(
        check_open_paths(&tree, &card, paths(&many)),
        Err(OverlayError::TooManyPaths(65))
    );
    // 64 is allowed by the count; the same path repeated is refused for repeating.
    let full: Vec<&Vec<Key>> = std::iter::repeat_n(&g, MAX_OPEN_PATHS).collect();
    assert_eq!(
        check_open_paths(&tree, &card, paths(&full)),
        Err(OverlayError::RepeatedPath(key("m.py#f/for[0]/body/call:g")))
    );
}

#[test]
fn the_rules_hold_on_the_goldens() {
    let playground = Playground::load("ts-orders");
    let trees: Trees = playground.trees();
    let order = "src/routes/orders.ts#createOrder";
    let combined = keys(&[
        "src/routes/orders.ts#createOrder/call:orderService.place",
        "src/routes/orders.ts#createOrder/call:slack.post",
    ]);
    assert_eq!(check_combined(&trees, &combined), Ok(()));
    assert_eq!(
        check_combined(
            &trees,
            &keys(&["src/routes/orders.ts#createOrder/if[0]/then/call:req.header"])
        ),
        Err(OverlayError::NotCombinable(key(
            "src/routes/orders.ts#createOrder/if[0]/then/call:req.header"
        )))
    );
    let nested = keys(&["src/routes/orders.ts#createOrder/call:res.status().json/call:res.status"]);
    assert_eq!(
        check_combined(&trees, &nested),
        Err(OverlayError::NotCombinable(nested[0].clone()))
    );
    assert_eq!(
        check_proposal(
            &trees,
            &ProposalSpot::Start(key("src/routes/orders.ts#createOrder/if[0]/then"))
        ),
        Ok(())
    );

    // Across files: the call opens its callee, whose own call opens in turn.
    let path = keys(&[
        "src/routes/orders.ts#createOrder/call:orderService.place",
        "src/services/order-service.ts#OrderService.place/call:validateOrder",
    ]);
    assert_eq!(check_open_path(&trees, &key(order), &path), Ok(()));
    // Three callbacks and calls deep, from a module's own code.
    let worker = "src/queues/email-worker.ts#/call:Worker(\"emails\")/callback[0]";
    let deep = keys(&[
        worker,
        &format!("{worker}/call:withRetry/callback[0]"),
        &format!("{worker}/call:withRetry/callback[0]/call:sendEmail"),
    ]);
    assert_eq!(
        check_open_path(&trees, &key("src/queues/email-worker.ts#"), &deep),
        Ok(())
    );
    let library = keys(&["src/queues/email-worker.ts#/call:Worker(\"emails\")"]);
    assert_eq!(
        check_open_path(&trees, &key("src/queues/email-worker.ts#"), &library),
        Err(OverlayError::NotOpenable(library[0].clone()))
    );

    let py = Playground::load("py-orders");
    let (tree, callback) =
        py.block("src/orders_playground/queues/email_worker.py#email_task/call:with_retry/callback[0]");
    assert_eq!(openable(&tree, callback), Some(Openable::Callback));
}
