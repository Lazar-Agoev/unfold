//! The value policy's reason order, one step at a time, including the step that wins when two
//! tests match.

use unfold_core::vocab::{EdgeKind, Reason, Resolution};

use crate::alien::Project;
use crate::{assert_external, assert_reason, assert_resolved, compile, occurrence};

#[test]
fn step_1_language_refusals_come_first() {
    for (option, reason) in [
        ("wrapper", Reason::Wrapper),
        ("dynamic", Reason::Dynamic),
        ("unsupported", Reason::Unsupported),
        ("not_concrete", Reason::NotConcrete),
    ] {
        assert_reason(&format!("(fn f) (call (ref f :refuse={option}))"), "f", reason);
    }
    // A refusal wins over every later test.
    assert_reason(
        "(fn f) (call (ref f :refuse=dynamic :ambiguous :excluded))",
        "f",
        Reason::Dynamic,
    );
}

#[test]
fn step_2_ambiguous_evidence() {
    assert_reason("(fn f) (call (ref f :ambiguous))", "f", Reason::Ambiguous);
    // Ambiguity wins over a failed import binding.
    assert_reason(
        "(fn f) (call (ref f :ambiguous :import=not_found))",
        "f",
        Reason::Ambiguous,
    );
}

#[test]
fn step_3_unbound_this_and_arrows() {
    assert_reason(
        "(fn plain (stmt (call (get (this) m))))",
        "this.m",
        Reason::UnboundThis,
    );
    assert_reason("(stmt (call (get (this) m)))", "this.m", Reason::UnboundThis);
    // Arrows inherit the member's `this`.
    assert_resolved(
        "(class C (method m) (method run (stmt (call (lambda (stmt (call (get (this) m))))))))",
        "this.m",
        "class:C/method:m",
    );
    // A non-arrow function inside a member does not.
    assert_reason(
        "(class C (method m) (method run (fn inner (stmt (call (get (this) m))))))",
        "this.m",
        Reason::UnboundThis,
    );
}

#[test]
fn step_4_failed_import_bindings() {
    assert_reason("(call (ref f :import=not_found))", "f", Reason::NotFound);
    assert_reason("(call (ref f :import=not_indexed))", "f", Reason::NotIndexed);
    assert_reason("(call (ref f :import=ambiguous))", "f", Reason::Ambiguous);
    // Import failures win over candidate origins.
    assert_reason(
        "(call (ref f :import=not_found :excluded))",
        "f",
        Reason::NotFound,
    );
    // Real bindings: a missing relative module, an unindexed one, and one that failed to parse.
    assert_reason(
        "(use helper \"./missing\") (call (ref helper))",
        "helper",
        Reason::NotFound,
    );
    let compiled = Project::new()
        .file(
            "a.alien",
            "(use helper \"./skipped\") (use broken \"./broken\") (call (ref helper)) (call (ref broken))",
        )
        .file("broken.alien", "(fn unclosed")
        .unindexed("skipped.alien")
        .compile()
        .valid();
    for text in ["helper", "broken"] {
        let edge = compiled.edge("a.alien", text);
        assert_eq!(edge.reason, Some(Reason::NotIndexed), "{text}");
    }
}

#[test]
fn step_5_unindexed_repository_candidates() {
    assert_reason("(call (ref f :excluded))", "f", Reason::NotIndexed);
    assert_reason("(call (ref f :extfile=vendor/lib.ts))", "f", Reason::NotIndexed);
    // An unindexed candidate wins over a package-import root.
    assert_reason(
        "(use express \"express\") (call (ref express :excluded))",
        "express",
        Reason::NotIndexed,
    );
}

#[test]
fn step_6_package_import_roots_are_external() {
    assert_external(
        "(use express \"express\") (call (ref express))",
        "express",
        "express",
    );
    assert_external(
        "(use router \"@scope/router/sub\") (call (ref router))",
        "router",
        "@scope/router",
    );
    // Members of a package import name the package, without any installed declaration.
    assert_external(
        "(use express \"express\") (call (get (ref express) Router))",
        "express.Router",
        "express",
    );
}

#[test]
fn step_7_no_candidates() {
    assert_reason("(call (ref nothing))", "nothing", Reason::UnknownName);
    assert_reason(
        "(call (get (ref nothing) member))",
        "nothing.member",
        Reason::UnknownReceiver,
    );
    assert_reason(
        "(const handlers) (call (dyn (ref handlers)))",
        "handlers[…]",
        Reason::Dynamic,
    );
    assert_reason("(fn f) (call (call (ref f)))", "f()", Reason::Dynamic);
    // Only external augmentations: still external, labeled by location.
    assert_external(
        "(const merged) (call (ref merged :augext))",
        "merged",
        "alien:lib",
    );
}

#[test]
fn step_8_external_and_mixed_origins() {
    assert_external("(call (ref console))", "console", "global");
    assert_external("(call (get (ref Math) max))", "Math.max", "global");
    // Declared roots are not `global`; their candidates' locations label them.
    assert_external(
        "(const helper) (call (ref helper :pkg=lodash/fp))",
        "helper",
        "lodash",
    );
    assert_external(
        "(const client) (call (ref client :pkg=client-b :mixed))",
        "client",
        r#"["client-b","mixed"]"#,
    );
    assert_reason("(fn f) (call (ref f :mixed))", "f", Reason::Ambiguous);
}

#[test]
fn step_9_indexed_candidates_without_a_declaration() {
    assert_reason("(call (ref f :nodecl))", "f", Reason::UnknownName);
}

#[test]
fn step_10_several_concrete_targets() {
    assert_reason("(fn f) (fn f) (call (ref f))", "f", Reason::Ambiguous);
}

#[test]
fn step_11_no_target_reasons_in_fixed_order() {
    let edge = occurrence("(fn run (param first) (param cb) (call (ref cb)))", "cb");
    assert_eq!((edge.reason, edge.position), (Some(Reason::Parameter), Some(1)));
    // C-4: an explicit `this` parameter takes no position, and nothing re-adds it.
    let edge = occurrence("(fn run (param this) (param cb) (call (ref cb)))", "cb");
    assert_eq!((edge.reason, edge.position), (Some(Reason::Parameter), Some(0)));
    // An outer closure's parameter, and a construction, have no position.
    let edge = occurrence(
        "(fn outer (param cb) (stmt (lambda (stmt (call (ref cb))))))",
        "cb",
    );
    assert_eq!((edge.reason, edge.position), (Some(Reason::Parameter), None));
    let edge = occurrence("(fn outer (param Cb) (stmt (new (ref Cb))))", "Cb");
    assert_eq!(
        (edge.kind, edge.reason, edge.position),
        (EdgeKind::Constructs, Some(Reason::Parameter), None)
    );
    assert_reason("(let g (lambda)) (call (ref g))", "g", Reason::MutableBinding);
    assert_reason(
        "(class C (getter value) (method run (stmt (call (get (this) value)))))",
        "this.value",
        Reason::Accessor,
    );
    assert_reason("(sig s) (call (ref s))", "s", Reason::NotConcrete);
    assert_reason("(class K) (call (ref K))", "K", Reason::NotConcrete);
    // Parameter beats a mutable binding regardless of declaration order.
    for source in [
        "(fn h (param g) (let g) (call (ref g)))",
        "(fn h (let g) (param g) (call (ref g)))",
    ] {
        let edge = occurrence(source, "g");
        assert_eq!(
            (edge.reason, edge.position),
            (Some(Reason::Parameter), Some(0)),
            "{source}"
        );
    }
}

#[test]
fn step_12_nonconcrete_evidence_beside_a_target() {
    assert_resolved("(sig f) (sig f) (fn f) (call (ref f))", "f", "function:f");
    assert_reason("(let f) (fn f) (call (ref f))", "f", Reason::Ambiguous);
    // Augmentations never count as non-concrete evidence.
    assert_resolved("(fn f) (call (ref f :aug))", "f", "function:f");
    // Ambiguity beside a target wins over overrides.
    assert_reason(
        "(let f) (fn f) (call (ref f :overridden))",
        "f",
        Reason::Ambiguous,
    );
}

#[test]
fn step_13_overrides() {
    let classes = "(class Base (method ping) (method make :static) (method secret :private)) \
                   (class Derived :base=Base (method ping) (method make :static) (method secret :private))";
    assert_reason(
        &format!("{classes} (fn run (param b :type=Base) (stmt (call (get (ref b) ping))))"),
        "b.ping",
        Reason::Overridden,
    );
    // `super` and static accesses stay exact; private names are their own slots.
    assert_resolved(
        "(class Base (method ping)) (class Derived :base=Base (method ping) (method run (stmt (call (get (base) ping)))))",
        "super.ping",
        "class:Base/method:ping",
    );
    assert_resolved(
        &format!("{classes} (call (get (ref Base) make))"),
        "Base.make",
        "class:Base/method:make",
    );
    assert_resolved(
        "(class Base (method secret :private) (method run (stmt (call (get (this) secret))))) \
         (class Derived :base=Base (method secret :private))",
        "this.secret",
        "class:Base/method:secret",
    );
    // Every declared base participates, and a callable field can override a method.
    assert_reason(
        "(class Base (method ping)) (class Other) (class Derived :base=Other :base=Base (field ping)) \
         (fn run (param b :type=Base) (stmt (call (get (ref b) ping))))",
        "b.ping",
        Reason::Overridden,
    );
    // The receiver's own class: a call through the subclass sees no further overrides.
    assert_resolved(
        &format!("{classes} (fn run (param d :type=Derived) (stmt (call (get (ref d) ping))))"),
        "d.ping",
        "class:Derived/method:ping",
    );
    // The language's own override evidence.
    assert_reason("(fn f) (call (ref f :overridden))", "f", Reason::Overridden);
}

#[test]
fn step_13_override_census_crosses_files() {
    let compiled = Project::new()
        .file(
            "a.alien",
            "(class Base (method ping)) (fn run (param b :type=Base) (stmt (call (get (ref b) ping))))",
        )
        .file(
            "b.alien",
            "(use Base \"./a\") (class Derived :base=Base (method ping))",
        )
        .compile()
        .valid();
    assert_eq!(
        compiled.edge("a.alien", "b.ping").reason,
        Some(Reason::Overridden)
    );
    let alone = Project::new()
        .file(
            "a.alien",
            "(class Base (method ping)) (fn run (param b :type=Base) (stmt (call (get (ref b) ping))))",
        )
        .compile()
        .valid();
    assert_eq!(alone.edge("a.alien", "b.ping").resolution, Resolution::Resolved);
}

#[test]
fn step_15_permitted_targets_by_context() {
    assert_resolved("(fn f) (call (ref f))", "f", "function:f");
    assert_resolved("(const f (lambda)) (call (ref f))", "f", "variable:f/function:f");
    // Constructions prefer a direct constructor, otherwise the class.
    let compiled = compile("(class K (ctor)) (class Bare) (stmt (new (ref K))) (stmt (new (ref Bare)))");
    let k = compiled.edge("a.alien", "K");
    assert_eq!(
        (k.kind, compiled.target(k)),
        (
            EdgeKind::Constructs,
            Some((
                unfold_core::vocab::BlockKind::Constructor,
                "a.alien#class:K/constructor:constructor"
            ))
        )
    );
    let bare = compiled.edge("a.alien", "Bare");
    assert_eq!(
        compiled.target(bare).map(|(kind, _)| kind),
        Some(unfold_core::vocab::BlockKind::Class)
    );
    // `super(...)` keeps its call block and `calls` edge but selects the construction target.
    let compiled = compile("(class B (ctor)) (class K :base=B (ctor (stmt (super (ref B)))))");
    let edge = compiled.edge("a.alien", "B");
    assert_eq!(edge.kind, EdgeKind::Calls);
    assert_eq!(
        edge.target.as_ref().map(|key| key.as_str()),
        Some("a.alien#class:B/constructor:constructor")
    );
}

#[test]
fn imports_resolve_through_module_targets() {
    let compiled = Project::new()
        .file(
            "a.alien",
            "(import \"./b\") (import \"express\") (import \"./missing\") (import \"./skipped\") \
             (import \"./broken\") (import (ref spec))",
        )
        .file("b.alien", "(fn f)")
        .file("broken.alien", "(")
        .unindexed("skipped.alien")
        .compile()
        .valid();
    let summary: Vec<_> = compiled
        .occurrences("a.alien")
        .into_iter()
        .map(|edge| {
            (
                &*edge.text,
                edge.resolution,
                edge.reason,
                edge.module.as_ref().map(|module| module.as_str().to_owned()),
                edge.target.as_ref().map(|key| key.as_str().to_owned()),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "./b",
                Resolution::Resolved,
                None,
                None,
                Some("b.alien#".to_owned())
            ),
            (
                "express",
                Resolution::External,
                None,
                Some("express".to_owned()),
                None
            ),
            (
                "./missing",
                Resolution::Unresolved,
                Some(Reason::NotFound),
                None,
                None
            ),
            (
                "./skipped",
                Resolution::External,
                None,
                Some("skipped.alien".to_owned()),
                None
            ),
            (
                "./broken",
                Resolution::Unresolved,
                Some(Reason::NotIndexed),
                None,
                None
            ),
            ("(…)", Resolution::Unresolved, Some(Reason::Dynamic), None, None),
        ]
    );
}
