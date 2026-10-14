//! Typed records through the alien language: argument pairing with resolved targets, bound
//! counts, and the `type` policy context for written type names.

use unfold_core::typed::Pairing;
use unfold_core::vocab::{LinkKind, PairingReason, Reason};
use unfold_core::{Key, Link, LinkTarget, ModuleLabel};

use crate::alien::Compiled;
use crate::compile;

/// The pairings of the arguments of the call labeled `text` in `a.alien`.
fn pairings(compiled: &Compiled, text: &str) -> Vec<Option<Pairing>> {
    let file = compiled.file("a.alien");
    let call = compiled.call("a.alien", text).from;
    file.types
        .arguments
        .iter()
        .filter(|argument| argument.call == call)
        .map(|argument| argument.pairing)
        .collect()
}

fn type_links(compiled: &Compiled) -> Vec<&Link> {
    compiled
        .file("a.alien")
        .links
        .iter()
        .filter(|link| matches!(link.kind, LinkKind::Types | LinkKind::Extends))
        .collect()
}

use Pairing::{Parameter as P, Unpaired as U};

#[test]
fn resolved_calls_pair_positions_with_parameters() {
    let compiled = compile(
        "(fn f (param this) (param a) (param b) (param rest :rest)) \
         (stmt (call (ref f) (ref x) (ref y) (ref z) (ref w)))",
    );
    assert_eq!(
        pairings(&compiled, "f"),
        [Some(P(1)), Some(P(2)), Some(P(3)), Some(P(3))]
    );
    let compiled = compile("(fn g (param a)) (stmt (call (ref g) (ref x) (ref y)))");
    assert_eq!(
        pairings(&compiled, "g"),
        [Some(P(0)), Some(U(PairingReason::NoParameter))]
    );
}

#[test]
fn spreads_and_unresolved_calls_are_not_paired() {
    let compiled = compile("(fn f (param a) (param b)) (stmt (call (ref f) (star (ref x)) (ref y)))");
    assert_eq!(
        pairings(&compiled, "f"),
        [Some(U(PairingReason::Spread)), Some(U(PairingReason::Spread))]
    );
    // An unresolved call has no pairing at all.
    let compiled = compile("(stmt (call (ref missing) (ref x)))");
    assert_eq!(pairings(&compiled, "missing"), [None]);
    // Literal arguments, which may pass nothing, still have their records.
    let compiled = compile("(fn f (param a)) (stmt (call (ref f) \"text\"))");
    assert_eq!(pairings(&compiled, "f"), [Some(P(0))]);
}

#[test]
fn constructions_of_classes_without_constructors_take_nothing() {
    let compiled = compile("(class C) (stmt (new (ref C) (ref x)))");
    assert_eq!(pairings(&compiled, "C"), [Some(U(PairingReason::NoParameter))]);
    let compiled = compile("(class B) (class C :base=B) (stmt (new (ref C) (ref x)))");
    assert_eq!(pairings(&compiled, "C"), [Some(U(PairingReason::NoSignature))]);
    // A decorator may replace the class with anything: nothing proves it takes no arguments.
    let compiled = compile("(class C (deco (ref wrap))) (stmt (new (ref C) (ref x)))");
    assert_eq!(pairings(&compiled, "C"), [Some(U(PairingReason::NoSignature))]);
    let compiled = compile("(class C (ctor (param a))) (stmt (new (ref C) (ref x)))");
    assert_eq!(pairings(&compiled, "C"), [Some(P(0))]);
}

#[test]
fn bound_counts_follow_the_language_answer() {
    let compiled = compile(
        "(fn f (param a)) (stmt (call (ref f) (ref x))) (stmt (call (ref g) (ref f))) \
         (stmt (call (ref missing)))",
    );
    let bounds: Vec<Option<u32>> = compiled.calls("a.alien").iter().map(|link| link.bound).collect();
    // Resolved to a function: the alien language binds nothing; unresolved calls have no count.
    assert_eq!(bounds, [Some(0), None, None]);
    let file = compiled.file("a.alien");
    let passed = compiled.call("a.alien", "g").from;
    let argument = file
        .types
        .arguments
        .iter()
        .find(|argument| argument.call == passed)
        .unwrap();
    // `f` is passed as a function, but `g` reaches nothing, so `f`'s count is still recorded.
    assert_eq!(argument.bound, Some(0));
    let first = compiled.call("a.alien", "f").from;
    assert!(
        file.types
            .arguments
            .iter()
            .filter(|argument| argument.call == first)
            .all(|argument| argument.bound.is_none())
    );
}

#[test]
fn type_names_resolve_through_the_type_context() {
    let compiled = compile(
        "(class Cart) (fn helper) (let money) \
         (fn f (param a :type=Cart) (param b :type=Missing) (param c :type=console) \
            (param d :type=helper) (param e :type=money))",
    );
    let outcomes: Vec<(&str, &LinkTarget)> = type_links(&compiled)
        .into_iter()
        .map(|link| (&*link.text, &link.to))
        .collect();
    assert_eq!(
        outcomes,
        [
            ("Cart", &LinkTarget::Project(Key::from_text("a.alien#Cart"))),
            ("Missing", &LinkTarget::Unknown(Reason::UnknownName)),
            (
                "console",
                &LinkTarget::Builtin(ModuleLabel::single("alien:lib").unwrap())
            ),
            ("helper", &LinkTarget::Unknown(Reason::NotConcrete)),
            ("money", &LinkTarget::Unknown(Reason::MutableBinding)),
        ]
    );
    // Every link starts at the function whose parameters name the types.
    let function = compiled.file("a.alien").local_id("a.alien#f").unwrap();
    assert!(type_links(&compiled).iter().all(|link| link.from == function));
}

#[test]
fn type_names_follow_the_reason_order() {
    let outcome = |source: &str| type_links(&compile(source))[0].to.clone();
    // Two declarations of the name: step 10.
    assert_eq!(
        outcome("(class Cart) (class Cart) (fn f (param a :type=Cart))"),
        LinkTarget::Unknown(Reason::Ambiguous)
    );
    // An outside package import binding: step 6, labeled from its specifier.
    assert_eq!(
        outcome("(use Stripe \"stripe\") (fn f (param a :type=Stripe))"),
        LinkTarget::Library(ModuleLabel::single("stripe").unwrap())
    );
    // A parameter bound to the name: step 11's first reason.
    assert_eq!(
        outcome("(fn f (param Cart) (param a :type=Cart))"),
        LinkTarget::Unknown(Reason::Parameter)
    );
}

#[test]
fn bases_are_extends_links_and_bindings_link_from_their_definition() {
    let compiled =
        compile("(class Cart) (class Base) (class Mine :base=Base :impl=Cart) (const cart :type=Cart)");
    let file = compiled.file("a.alien");
    let summary: Vec<_> = type_links(&compiled)
        .into_iter()
        .map(|link| {
            (
                file.nodes[link.from as usize].key.as_str(),
                link.kind,
                &*link.text,
                link.implements,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("a.alien#", LinkKind::Types, "Cart", false),
            ("a.alien#Mine", LinkKind::Extends, "Base", false),
            ("a.alien#Mine", LinkKind::Extends, "Cart", true),
        ]
    );
}
