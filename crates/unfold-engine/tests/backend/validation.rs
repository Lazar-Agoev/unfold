//! The graph-level validation hook: core invariants plus the language's independent census.

use crate::alien::Project;

#[test]
fn the_census_catches_ranges_off_token_boundaries() {
    let project = Project::new().file("a.alien", "(fn work (call (ref work)))");
    let mut graph = project.compile().valid().graph;
    assert!(project.validate(&graph).is_empty());
    // Move the call's start inside its `call` atom: still inside its parent, but not a token.
    let call = graph.files[0]
        .blocks
        .iter_mut()
        .find(|block| block.kind == unfold_core::vocab::BlockKind::Call)
        .expect("a call block");
    call.range.start += 2;
    let violations = project.validate(&graph);
    assert!(
        violations
            .iter()
            .any(|violation| violation.message.contains("token boundaries")),
        "{violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.message.contains("call site")),
        "the census call site no longer matches a block: {violations:?}"
    );
}
