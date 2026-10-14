//! The value policy's reason order, one step at a time, including the step that wins when two
//! tests match.

use unfold_core::LinkTarget;
use unfold_core::vocab::{Form, LinkKind, Reason};

use crate::alien::Project;
use crate::{assert_library, assert_reason, assert_resolved, call, compile};

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
        "C.m",
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
        let link = compiled.call("a.alien", text);
        assert_eq!(link.to, LinkTarget::Unknown(Reason::NotIndexed), "{text}");
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
    assert_library(
        "(use express \"express\") (call (ref express))",
        "express",
        "express",
    );
    assert_library(
        "(use router \"@scope/router/sub\") (call (ref router))",
        "router",
        "@scope/router",
    );
    // Members of a package import name the package, without any installed declaration.
    assert_library(
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
    // Only outside augmentations: still outside, labeled by location (here the builtins).
    assert_eq!(
        call("(const merged) (call (ref merged :augext))", "merged").to,
        builtin()
    );
}

#[test]
fn step_8_external_and_mixed_origins() {
    // The standard library is the language's builtins.
    assert_eq!(call("(call (ref console))", "console").to, builtin());
    assert_eq!(call("(call (get (ref Math) max))", "Math.max").to, builtin());
    // An ambient root whose candidates are all the language's own builtins is labeled by
    // their location, not as a global.
    assert_eq!(
        call("(call (get (ref JSON) parse))", "JSON.parse").to,
        LinkTarget::Builtin(unfold_core::ModuleLabel::single("alien:core").expect("a label"))
    );
    // Declared roots are not `global`; their candidates' locations label them.
    assert_library(
        "(const helper) (call (ref helper :pkg=lodash/fp))",
        "helper",
        "lodash",
    );
    assert_library(
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
    assert_reason(
        "(fn run (param first) (param cb) (call (ref cb)))",
        "cb",
        Reason::Parameter,
    );
    // An outer function's parameter, and a construction.
    assert_reason(
        "(fn outer (param cb) (stmt (lambda (stmt (call (ref cb))))))",
        "cb",
        Reason::Parameter,
    );
    assert_reason(
        "(fn outer (param Cb) (stmt (new (ref Cb))))",
        "Cb",
        Reason::Parameter,
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
        assert_reason(source, "g", Reason::Parameter);
    }
}

#[test]
fn step_12_nonconcrete_evidence_beside_a_target() {
    assert_resolved("(sig f) (sig f) (fn f) (call (ref f))", "f", "f");
    assert_reason("(let f) (fn f) (call (ref f))", "f", Reason::Ambiguous);
    // Augmentations never count as non-concrete evidence.
    assert_resolved("(fn f) (call (ref f :aug))", "f", "f");
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
        "Base.ping",
    );
    assert_resolved(
        &format!("{classes} (call (get (ref Base) make))"),
        "Base.make",
        "Base.make",
    );
    assert_resolved(
        "(class Base (method secret :private) (method run (stmt (call (get (this) secret))))) \
         (class Derived :base=Base (method secret :private))",
        "this.secret",
        "Base.secret",
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
        "Derived.ping",
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
        compiled.call("a.alien", "b.ping").to,
        LinkTarget::Unknown(Reason::Overridden)
    );
    let alone = Project::new()
        .file(
            "a.alien",
            "(class Base (method ping)) (fn run (param b :type=Base) (stmt (call (get (ref b) ping))))",
        )
        .compile()
        .valid();
    assert!(matches!(
        alone.call("a.alien", "b.ping").to,
        LinkTarget::Project(_)
    ));
}

#[test]
fn step_15_permitted_targets_by_context() {
    assert_resolved("(fn f) (call (ref f))", "f", "f");
    assert_resolved("(const f (lambda)) (call (ref f))", "f", "f");
    // Constructions prefer a direct constructor, otherwise the class.
    let compiled = compile("(class K (ctor)) (class Bare) (stmt (new (ref K))) (stmt (new (ref Bare)))");
    let k = compiled.call("a.alien", "K");
    assert_eq!(compiled.from("a.alien", k).form, Form::New);
    assert_eq!(k.to, project("a.alien#K.constructor"));
    assert_eq!(compiled.call("a.alien", "Bare").to, project("a.alien#Bare"));
    // `super(...)` keeps its call form but selects the construction target.
    let compiled = compile("(class B (ctor)) (class K :base=B (ctor (stmt (super (ref B)))))");
    let link = compiled.call("a.alien", "B");
    assert_eq!(compiled.from("a.alien", link).form, Form::Call);
    assert_eq!(link.to, project("a.alien#B.constructor"));
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
    let imports: Vec<(&str, &LinkTarget)> = compiled
        .file("a.alien")
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Imports)
        .map(|link| (&*link.text, &link.to))
        .collect();
    assert_eq!(
        imports,
        [
            ("./b", &project("b.alien#")),
            ("express", &library("express")),
            ("./missing", &LinkTarget::Unknown(Reason::NotFound)),
            ("./skipped", &library("skipped.alien")),
            ("./broken", &LinkTarget::Unknown(Reason::NotIndexed)),
            ("(…)", &LinkTarget::Unknown(Reason::Dynamic)),
        ]
    );
}

fn project(key: &str) -> LinkTarget {
    LinkTarget::Project(unfold_core::Key::from_text(key))
}

fn library(label: &str) -> LinkTarget {
    LinkTarget::Library(unfold_core::ModuleLabel::single(label).expect("a label"))
}

fn builtin() -> LinkTarget {
    LinkTarget::Builtin(unfold_core::ModuleLabel::single("alien:lib").expect("a label"))
}
