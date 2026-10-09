//! The shared backend, exercised end to end through an unrelated in-memory language: extraction
//! drives the shared selection, facts round-trip through the cache encoding, linking answers
//! evidence over IDs, and every compiled graph passes validation with the language's census.

mod alien;
mod determinism;
mod edges;
mod entrances;
mod link_hooks;
mod policy;
mod selection;
mod update;
mod validation;

use unfold_core::Edge;
use unfold_core::vocab::{Reason, Resolution};

use alien::{Compiled, Project};

/// Compiles one alien file, `a.alien`, and checks the graph is valid.
fn compile(source: &str) -> Compiled {
    Project::new().file("a.alien", source).compile().valid()
}

/// The occurrence edge labeled `text` in `a.alien`.
fn occurrence(source: &str, text: &str) -> Edge {
    compile(source).edge("a.alien", text).clone()
}

/// Asserts the occurrence labeled `text` is unresolved for `reason`.
#[track_caller]
fn assert_reason(source: &str, text: &str, reason: Reason) {
    let edge = occurrence(source, text);
    assert_eq!(
        (edge.resolution, edge.reason),
        (Resolution::Unresolved, Some(reason)),
        "{source}"
    );
}

/// Asserts the occurrence labeled `text` is external with `module`.
#[track_caller]
fn assert_external(source: &str, text: &str, module: &str) {
    let edge = occurrence(source, text);
    assert_eq!(edge.resolution, Resolution::External, "{source}: {edge:?}");
    assert_eq!(
        edge.module.as_ref().map(|module| module.as_str()),
        Some(module),
        "{source}"
    );
}

/// Asserts the occurrence labeled `text` resolves to the block with `key` (in `a.alien`).
#[track_caller]
fn assert_resolved(source: &str, text: &str, key: &str) {
    let edge = occurrence(source, text);
    assert_eq!(edge.resolution, Resolution::Resolved, "{source}: {edge:?}");
    assert_eq!(
        edge.target.as_ref().map(|key| key.as_str()),
        Some(format!("a.alien#{key}").as_str())
    );
}
