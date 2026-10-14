//! The two link-time hooks a language can use when extraction alone can't decide: a call proven
//! to be a construction, and conditional imports that exist only when linking keeps them.

use unfold_core::vocab::{Form, LinkKind, Reason};
use unfold_core::{Key, LinkTarget};

use crate::alien::Project;
use crate::compile;

#[test]
fn a_proven_construction_becomes_new_and_resolves_as_one() {
    let source = "(class Job (ctor)) (stmt (call (ref Job :construct))) (stmt (call (ref Job)))";
    let compiled = compile(source);
    let calls = compiled.calls("a.alien");
    assert_eq!(compiled.from("a.alien", calls[0]).form, Form::New);
    assert_eq!(
        calls[0].to,
        LinkTarget::Project(Key::from_text("a.alien#Job.constructor"))
    );
    // Without the hook, the call form and the call context stand: a class has no body.
    assert_eq!(compiled.from("a.alien", calls[1]).form, Form::Call);
    assert_eq!(calls[1].to, LinkTarget::Unknown(Reason::NotConcrete));
    // Keys don't depend on the form.
    assert_eq!(
        compiled.from("a.alien", calls[0]).key.as_str(),
        "a.alien#/call:Job"
    );
}

#[test]
fn unkept_conditional_imports_have_no_link_and_change_nothing_else() {
    let main = "(use helper \"./lib\" (import \"./gone\" :when-indexed) (import \"./lib\" :when-indexed) (import \"./gone\" :when-indexed) (import \"./lib\" :when-indexed)) (fn f) (stmt (call (ref f)))";
    let compiled = Project::new()
        .file("a.alien", main)
        .file("lib.alien", "(fn helper)")
        .compile()
        .valid();
    let file = compiled.file("a.alien");
    let keys: Vec<&str> = file.nodes.iter().map(|node| node.key.as_str()).collect();
    assert_eq!(keys, ["a.alien#", "a.alien#f", "a.alien#/call:f"]);
    // The use and the kept imports (the second and fourth), in written order.
    let starts: Vec<usize> = main.match_indices("\"./lib\"").map(|(at, _)| at).collect();
    let imports: Vec<(u32, u32, &str)> = file
        .links
        .iter()
        .filter(|link| link.kind == LinkKind::Imports)
        .map(|link| (link.seq, link.range.start, &*link.text))
        .collect();
    assert_eq!(
        imports,
        [
            (0, starts[0] as u32, "./lib"),
            (1, starts[1] as u32, "./lib"),
            (2, starts[2] as u32, "./lib"),
        ]
    );
    // Without the target file, every candidate goes and the nodes are the same.
    let alone = compile(main);
    let file = alone.file("a.alien");
    assert_eq!(file.nodes.len(), 3);
    assert_eq!(
        file.links
            .iter()
            .filter(|link| link.kind == LinkKind::Imports)
            .count(),
        1
    );
}
