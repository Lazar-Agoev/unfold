//! Links: each node's `calls` link first and the rest in written order, `passes` links for the
//! project definitions a call passes, labels, module-label rules, and bounded provenance.

use unfold_core::LinkTarget;
use unfold_core::text::utf16_len;
use unfold_core::vocab::{LinkKind, Reason};

use crate::{assert_library, compile};

#[test]
fn passes_links_follow_their_call_in_argument_order() {
    let compiled = compile(
        "(fn register) (fn work) (fn other) (class Job)
         (stmt (call (ref register) (ref work) (lambda) (ref nothing) (star (ref other)) (ref Job) (ref other)))",
    );
    let file = compiled.file("a.alien");
    let call = compiled.call("a.alien", "register").from;
    let summary: Vec<_> = file
        .links
        .iter()
        .filter(|link| link.from == call)
        .map(|link| {
            let to = match &link.to {
                LinkTarget::Project(key) => &key.as_str()[8..],
                other => panic!("{other:?}"),
            };
            (link.seq, link.kind, &*link.text, to)
        })
        .collect();
    assert_eq!(
        summary,
        [
            (0, LinkKind::Calls, "register", "register"),
            // The inline function, the unknown name, and the spread pass nothing.
            (1, LinkKind::Passes, "work", "work"),
            (2, LinkKind::Passes, "Job", "Job"),
            (3, LinkKind::Passes, "other", "other"),
        ]
    );
    // The callback is a node in the call's arguments, with every argument recorded.
    assert!(compiled.node("a.alien#/call:register/callback[0]").name.is_none());
    let arguments = file
        .types
        .arguments
        .iter()
        .filter(|argument| argument.call == call)
        .count();
    assert_eq!(arguments, 6);
}

#[test]
fn a_nodes_calls_link_comes_first_then_written_order() {
    let compiled = compile(
        "(class Base) (class Mixin) (class Job :impl=Mixin :base=Base) \
         (fn f (param a :type=Job) (use x \"pkg\") (param b :type=Base)) (call (ref f))",
    );
    let file = compiled.file("a.alien");
    let summary: Vec<_> = file
        .links
        .iter()
        .map(|link| {
            (
                file.nodes[link.from as usize].key.as_str(),
                link.seq,
                link.kind,
                &*link.text,
                link.implements,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("a.alien#Job", 0, LinkKind::Extends, "Mixin", true),
            ("a.alien#Job", 1, LinkKind::Extends, "Base", false),
            ("a.alien#f", 0, LinkKind::Types, "Job", false),
            ("a.alien#f", 1, LinkKind::Imports, "pkg", false),
            ("a.alien#f", 2, LinkKind::Types, "Base", false),
            ("a.alien#/call:f", 0, LinkKind::Calls, "f", false),
        ]
    );
}

#[test]
fn labels_are_short_syntax() {
    let compiled = compile(
        "(call (get (call (ref adults)) map))
         (call (key (ref handlers) \"save\"))
         (call (dyn (ref handlers)))
         (call (wrap (get (this) run)))
         (call (spread (ref xs)))",
    );
    let texts: Vec<&str> = compiled
        .calls("a.alien")
        .into_iter()
        .map(|link| &*link.text)
        .collect();
    assert_eq!(
        texts,
        [
            "adults().map",
            "adults",
            "handlers[\"save\"]",
            "handlers[…]",
            "this.run",
            "(…)"
        ]
    );
    let long = format!("(call (get (ref {}) tail))", "x".repeat(200));
    let link = compile(&long).calls("a.alien")[0].clone();
    assert_eq!(utf16_len(&link.text), 120);
    assert!(link.text.starts_with("xxx"));
}

#[test]
fn call_nodes_carry_their_normalized_callee_and_statement() {
    let compiled = compile(
        "(fn f) (const receipt (call (get (call (get (ref res) status) \"x\") json)))
         (stmt (call (ref app.post) \"/orders\" (lambda (ret (call (ref f))))))",
    );
    let call = compiled.node("a.alien#/call:res.status().json");
    assert_eq!(call.binds.as_deref(), Some("receipt"));
    assert!(call.stmt.is_some());
    assert!(
        compiled
            .node("a.alien#/call:res.status().json/call:res.status")
            .stmt
            .is_none()
    );
    let route = "a.alien#/call:app.post(\"/orders\")";
    let returned = compiled.node(&format!("{route}/callback[0]/return[0]/call:f"));
    assert!(returned.flags.contains(unfold_core::vocab::Flag::ExitValue));
}

#[test]
fn module_label_rule_order() {
    // Rule 1: the import binding's package, even when declarations live elsewhere.
    assert_library(
        "(use express \"express\") (call (get (ref express) json :ext=types))",
        "express.json",
        "express",
    );
    // Rule 2: the receiver's provenance names its package before the declaration location.
    assert_library(
        "(use express \"express\") (const app (call (ref express))) (call (get (ref app) listen :ext=express-types))",
        "app.listen",
        "express",
    );
    // Three steps: the import, `Router`, and the factory call.
    assert_library(
        "(use express \"express\") (const router (call (get (ref express) Router))) \
         (call (get (ref router) use :ext=router-types))",
        "router.use",
        "express",
    );
    // A fourth step is beyond the budget, so rule 4 (the location) applies.
    assert_library(
        "(use express \"express\") (const deep (call (get (get (ref express) a) b))) \
         (call (get (ref deep) use :ext=deep-types))",
        "deep.use",
        "deep-types",
    );
    // A local factory proves nothing.
    assert_library(
        "(fn express) (const app (call (ref express))) (call (get (ref app) listen :ext=local-types))",
        "app.listen",
        "local-types",
    );
    // Rule 3 needs an undeclared root (the builtins here); a declared one falls through to its
    // locations.
    assert_eq!(
        compile("(call (get (ref console) log))")
            .call("a.alien", "console.log")
            .to,
        LinkTarget::Builtin(unfold_core::ModuleLabel::single("alien:lib").unwrap())
    );
    assert_library(
        "(const console) (call (get (ref console) log :ext=shim))",
        "console.log",
        "shim",
    );
}

#[test]
fn passes_links_obey_the_policy() {
    let compiled = compile(
        "(fn register) (let mutable (lambda)) (class Base (method ping)) (class Derived :base=Base (method ping))
         (fn run (param b :type=Base) (stmt (call (ref register) (ref mutable) (get (ref b) ping) (ref console))))",
    );
    let call = compiled.call("a.alien", "register");
    let passes = compiled
        .file("a.alien")
        .links
        .iter()
        .filter(|link| link.from == call.from && link.kind == LinkKind::Passes)
        .count();
    assert_eq!(
        passes, 0,
        "arguments that reach no project definition pass nothing"
    );
    assert!(matches!(call.to, LinkTarget::Project(_)));
    let unresolved = compile("(stmt (call (ref register)))");
    assert_eq!(
        unresolved.calls("a.alien")[0].to,
        LinkTarget::Unknown(Reason::UnknownName)
    );
}
