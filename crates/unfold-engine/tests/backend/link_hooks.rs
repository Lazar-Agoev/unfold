//! The two link-time hooks a language can use when extraction alone can't decide: an invocation
//! proven to be a construction, and conditional `import` blocks that exist only when linking
//! keeps them.

use unfold_core::vocab::{BlockKind, EdgeKind, Resolution};

use crate::alien::Project;
use crate::compile;

#[test]
fn a_proven_construction_changes_the_edge_kind_and_context() {
    let source = "(class Job (ctor)) (stmt (call (ref Job :construct))) (stmt (call (ref Job)))";
    let compiled = Project::new().file("a.alien", source).compile();
    let occurrences = compiled.occurrences("a.alien");
    assert_eq!(occurrences[0].kind, EdgeKind::Constructs);
    assert_eq!(
        compiled.target(occurrences[0]),
        Some((
            BlockKind::Constructor,
            "a.alien#class:Job/constructor:constructor"
        ))
    );
    // Without the hook, the fact's `calls` edge and call context stand: a class has no body.
    assert_eq!(occurrences[1].kind, EdgeKind::Calls);
    assert_eq!(occurrences[1].resolution, Resolution::Unresolved);
    // The answer must fit the call's block kind: alien files claim TypeScript, whose `call`
    // blocks never source `constructs` edges, so validation reports exactly that.
    let messages: Vec<String> = compiled.violations.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].contains("cannot source a constructs edge"),
        "{messages:?}"
    );
}

#[test]
fn unkept_conditional_imports_are_dropped_and_the_rest_renumbered() {
    let main = "(use helper \"./lib\" (import \"./gone\" :when-indexed) (import \"./lib\" :when-indexed) (import \"./gone\" :when-indexed) (import \"./lib\" :when-indexed)) (fn f) (stmt (call (ref f)))";
    let compiled = Project::new()
        .file("a.alien", main)
        .file("lib.alien", "(fn helper)")
        .compile()
        .valid();
    let file = compiled.file("a.alien");
    let keys: Vec<&str> = file.blocks.iter().map(|block| block.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "a.alien#",
            "a.alien#import[0]",
            "a.alien#import[0]/import[0]",
            "a.alien#import[0]/import[1]",
            "a.alien#function:f",
            "a.alien#expression_statement[0]",
            "a.alien#expression_statement[0]/call[0]",
        ]
    );
    assert!(
        file.blocks
            .iter()
            .enumerate()
            .all(|(index, block)| block.local_id == index as u32)
    );
    // The kept imports are the second and fourth candidates.
    let starts: Vec<usize> = main
        .match_indices("(import \"./lib\"")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(file.blocks[2].range.start as usize, starts[0]);
    assert_eq!(file.blocks[3].range.start as usize, starts[1]);
    let sources: Vec<(u32, EdgeKind, &str)> = file
        .edges
        .iter()
        .map(|edge| (edge.source, edge.kind, &*edge.text))
        .collect();
    assert_eq!(
        sources,
        [
            (1, EdgeKind::Imports, "./lib"),
            (2, EdgeKind::Imports, "./lib"),
            (3, EdgeKind::Imports, "./lib"),
            (6, EdgeKind::Calls, "f"),
        ]
    );
    assert_eq!(
        file.slots.iter().filter(|slot| slot.block == 3).count(),
        1,
        "the kept candidate keeps its specifier slot"
    );
    // Without the target file, every candidate goes.
    let alone = compile(main);
    assert_eq!(alone.file("a.alien").blocks.len(), 5);
}
