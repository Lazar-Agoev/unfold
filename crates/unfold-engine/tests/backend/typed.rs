//! Typed records through the alien language: argument pairing with resolved targets, the `type`
//! policy context for written type names, and call results.

use unfold_core::typed::{Pairing, TypeLink};
use unfold_core::vocab::{PairingReason, Reason, Resolution, ResultBinding};

use crate::alien::{Compiled, Project};
use crate::compile;

/// The pairings of the arguments of the call labeled `text` in `a.alien`.
fn pairings(compiled: &Compiled, text: &str) -> Vec<Option<Pairing>> {
    let file = compiled.file("a.alien");
    let call = compiled.edge("a.alien", text).source;
    file.types
        .arguments
        .iter()
        .filter(|argument| argument.call == call)
        .map(|argument| argument.pairing)
        .collect()
}

fn links(compiled: &Compiled) -> Vec<&TypeLink> {
    compiled.file("a.alien").types.type_links.iter().collect()
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
    // An unresolved occurrence has no pairing at all.
    let compiled = compile("(stmt (call (ref missing) (ref x)))");
    assert_eq!(pairings(&compiled, "missing"), [None]);
    // Literal arguments, which record no value, still have their records.
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
    let file = compiled.file("a.alien");
    let bounds: Vec<Option<u32>> = file.types.results.iter().map(|result| result.bound).collect();
    // Resolved to a function: the alien language binds nothing; unresolved calls have no count.
    assert_eq!(bounds, [Some(0), None, None]);
    let passed = compiled.edge("a.alien", "g").source;
    let argument = file
        .types
        .arguments
        .iter()
        .find(|argument| argument.call == passed)
        .unwrap();
    // `f` is passed as a callable: its count is recorded on the argument.
    assert_eq!(argument.bound, Some(0));
    let first = compiled.edge("a.alien", "f").source;
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
    let outcomes: Vec<_> = links(&compiled)
        .iter()
        .map(|link| {
            (
                &*link.text,
                link.resolution,
                link.target.as_ref().map(|key| key.as_str()),
                link.reason,
                link.module.as_ref().map(|module| module.as_str()),
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            (
                "Cart",
                Resolution::Resolved,
                Some("a.alien#class:Cart"),
                None,
                None
            ),
            (
                "Missing",
                Resolution::Unresolved,
                None,
                Some(Reason::UnknownName),
                None
            ),
            ("console", Resolution::External, None, None, Some("global")),
            (
                "helper",
                Resolution::Unresolved,
                None,
                Some(Reason::NotConcrete),
                None
            ),
            (
                "money",
                Resolution::Unresolved,
                None,
                Some(Reason::MutableBinding),
                None
            ),
        ]
    );
    // Every link is sourced at the function, the innermost block around its parameter.
    let function = compiled.block("a.alien#function:f").local_id;
    assert!(links(&compiled).iter().all(|link| link.block == function));
}

#[test]
fn type_names_follow_the_reason_order() {
    let outcome = |source: &str| {
        let compiled = compile(source);
        let link = links(&compiled)[0].clone();
        (
            link.resolution,
            link.reason,
            link.module.map(|module| module.as_str().to_owned()),
        )
    };
    // Two declarations of the name: step 10.
    assert_eq!(
        outcome("(class Cart) (class Cart) (fn f (param a :type=Cart))"),
        (Resolution::Unresolved, Some(Reason::Ambiguous), None)
    );
    // An external package import binding: step 6, labeled from its specifier.
    assert_eq!(
        outcome("(use Stripe \"stripe\") (fn f (param a :type=Stripe))").0,
        Resolution::External
    );
    // A parameter bound to the name: step 11's first reason.
    assert_eq!(
        outcome("(fn f (param Cart) (param a :type=Cart))"),
        (Resolution::Unresolved, Some(Reason::Parameter), None)
    );
}

#[test]
fn type_names_on_variables_link_from_the_variable() {
    let compiled = compile("(class Cart) (const cart :type=Cart)");
    let variable = compiled.block("a.alien#variable:cart").local_id;
    let [link] = links(&compiled)[..] else {
        panic!("one link");
    };
    assert_eq!((link.block, link.resolution), (variable, Resolution::Resolved));
}

#[test]
fn results_say_where_each_value_lands() {
    let compiled = compile(
        "(fn f) (stmt (call (ref f))) (const x (call (ref f))) (fn g (ret (call (ref f)))) \
         (stmt (call (ref f) (wrap (call (ref f)))))",
    );
    let file = compiled.file("a.alien");
    let bindings: Vec<_> = file
        .types
        .results
        .iter()
        .map(|result| {
            (
                result.binding,
                result
                    .target
                    .map(|target| file.blocks[target as usize].key.as_str()),
                result.position,
            )
        })
        .collect();
    assert_eq!(
        bindings,
        [
            (ResultBinding::Discard, None, None),
            (ResultBinding::Variable, Some("a.alien#variable:x"), None),
            (ResultBinding::Return, None, None),
            (ResultBinding::Discard, None, None),
            (
                ResultBinding::Argument,
                Some("a.alien#expression_statement[1]/call[0]"),
                Some(0)
            ),
        ]
    );
}

#[test]
fn typed_records_survive_conditional_renumbering() {
    let compiled = Project::new()
        .file("b.alien", "(fn b)")
        .file(
            "a.alien",
            "(import \"./gone\" :when-indexed) (class Cart) (fn f (param a :type=Cart)) \
             (stmt (call (ref f) (ref x)))",
        )
        .compile()
        .valid();
    let file = compiled.file("a.alien");
    assert!(
        file.blocks
            .iter()
            .all(|block| block.kind != unfold_core::vocab::BlockKind::Import)
    );
    let [link] = links(&compiled)[..] else {
        panic!("one link");
    };
    assert_eq!(
        link.target.as_ref().map(|key| key.as_str()),
        Some("a.alien#class:Cart")
    );
    assert_eq!(pairings(&compiled, "f"), [Some(P(0))]);
}
