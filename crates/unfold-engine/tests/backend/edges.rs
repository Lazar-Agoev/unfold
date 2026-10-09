//! Edges in block order, references in argument order, labels, module-label rules, and bounded
//! provenance.

use unfold_core::text::utf16_len;
use unfold_core::vocab::{EdgeKind, Reason, Resolution};

use crate::{assert_external, compile};

#[test]
fn references_follow_their_call_in_argument_order() {
    let compiled = compile(
        "(fn register) (fn work) (fn other) (class Job)
         (stmt (call (ref register) (ref work) (lambda) (ref nothing) (star (ref other)) (ref Job) (ref other)))",
    );
    let summary: Vec<_> = compiled
        .file("a.alien")
        .edges
        .iter()
        .map(|edge| {
            (
                edge.seq,
                edge.kind,
                &*edge.text,
                edge.position,
                edge.target.as_ref().map(|key| &key.as_str()[8..]),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (0, EdgeKind::Calls, "register", None, Some("function:register")),
            // The inline function, the unknown name, and the expansion supply no reference;
            // positions keep counting every written argument.
            (1, EdgeKind::References, "work", Some(0), Some("function:work")),
            (2, EdgeKind::References, "Job", Some(4), Some("class:Job")),
            (3, EdgeKind::References, "other", Some(5), Some("function:other")),
        ]
    );
}

#[test]
fn occurrences_are_in_block_order_with_imports_interleaved() {
    let compiled = compile("(fn f) (call (ref f)) (import \"pkg\") (stmt (new (ref f) (call (ref f))))");
    let summary: Vec<_> = compiled
        .file("a.alien")
        .edges
        .iter()
        .map(|edge| (edge.source, edge.kind, &*edge.text))
        .collect();
    assert_eq!(
        summary,
        [
            (2, EdgeKind::Calls, "f"),
            (3, EdgeKind::Imports, "pkg"),
            (5, EdgeKind::Constructs, "f"),
            (6, EdgeKind::Calls, "f"),
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
        .occurrences("a.alien")
        .into_iter()
        .map(|edge| &*edge.text)
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
    let edge = compile(&long).occurrences("a.alien")[0].clone();
    assert_eq!(utf16_len(&edge.text), 120);
    assert!(edge.text.starts_with("xxx"));
}

#[test]
fn module_label_rule_order() {
    // Rule 1: the import binding's package, even when declarations live elsewhere.
    assert_external(
        "(use express \"express\") (call (get (ref express) json :ext=types))",
        "express.json",
        "express",
    );
    // Rule 2: the receiver's provenance names its package before the declaration location.
    assert_external(
        "(use express \"express\") (const app (call (ref express))) (call (get (ref app) listen :ext=express-types))",
        "app.listen",
        "express",
    );
    // Three steps: the import, `Router`, and the factory call.
    assert_external(
        "(use express \"express\") (const router (call (get (ref express) Router))) \
         (call (get (ref router) use :ext=router-types))",
        "router.use",
        "express",
    );
    // A fourth step is beyond the budget, so rule 4 (the location) applies.
    assert_external(
        "(use express \"express\") (const deep (call (get (get (ref express) a) b))) \
         (call (get (ref deep) use :ext=deep-types))",
        "deep.use",
        "deep-types",
    );
    // A local factory proves nothing.
    assert_external(
        "(fn express) (const app (call (ref express))) (call (get (ref app) listen :ext=local-types))",
        "app.listen",
        "local-types",
    );
    // Rule 3 needs an undeclared root; a declared one falls through to its locations.
    assert_external("(call (get (ref console) log))", "console.log", "global");
    assert_external(
        "(const console) (call (get (ref console) log :ext=shim))",
        "console.log",
        "shim",
    );
}

#[test]
fn reference_targets_obey_the_policy() {
    let compiled = compile(
        "(fn register) (let mutable (lambda)) (class Base (method ping)) (class Derived :base=Base (method ping))
         (fn run (param b :type=Base) (stmt (call (ref register) (ref mutable) (get (ref b) ping) (ref console))))",
    );
    let edges = &compiled.file("a.alien").edges;
    assert_eq!(
        edges.len(),
        1,
        "unresolved arguments produce no references: {edges:?}"
    );
    assert_eq!(edges[0].resolution, Resolution::Resolved);
    let unresolved = compile("(stmt (call (ref register)))");
    assert_eq!(
        unresolved.occurrences("a.alien")[0].reason,
        Some(Reason::UnknownName)
    );
}
