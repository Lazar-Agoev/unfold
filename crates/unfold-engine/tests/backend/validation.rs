//! The graph-level validation hook: core invariants plus the language's independent census.

use unfold_core::vocab::NodeKind;

use crate::alien::Project;

#[test]
fn the_census_catches_a_call_node_off_its_call_site() {
    let project = Project::new().file("a.alien", "(fn work (call (ref work)))");
    let mut graph = project.compile().valid().graph;
    assert!(project.validate(&graph).is_empty());
    // Move the call's start inside its `call` atom: still inside its parent, but no longer the
    // call the parser found.
    let call = graph.files[0]
        .nodes
        .iter_mut()
        .find(|node| node.kind() == NodeKind::Call)
        .expect("a call node");
    call.range.start += 2;
    let violations = project.validate(&graph);
    assert!(
        violations
            .iter()
            .any(|violation| violation.message.contains("needs exactly one call node")),
        "{violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.message.contains("does not match exactly one call site")),
        "{violations:?}"
    );
}

#[test]
fn the_census_finds_every_call_without_a_node() {
    let project = Project::new().file("a.alien", "(fn work) (call (ref work))");
    let mut graph = project.compile().valid().graph;
    // Drop the call node (the last in preorder) with its links and arguments, as a lowering
    // that forgot a call would.
    let file = &mut graph.files[0];
    let call = file.nodes.len() as u32 - 1;
    assert_eq!(file.nodes[call as usize].kind(), NodeKind::Call);
    file.nodes.pop();
    file.links.retain(|link| link.from != call);
    file.types.arguments.retain(|argument| argument.call != call);
    let violations = project.validate(&graph);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].message.contains("needs exactly one call node"),
        "{violations:?}"
    );
}
