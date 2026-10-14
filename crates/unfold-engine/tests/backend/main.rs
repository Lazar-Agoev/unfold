//! The shared backend, exercised end to end through an unrelated in-memory language: extraction
//! lowers to nodes, facts round-trip through the cache encoding, linking answers evidence over
//! IDs, and every compiled graph passes validation with the language's census.

mod alien;
mod determinism;
mod link_hooks;
mod links;
mod policy;
mod typed;
mod update;
mod validation;

use unfold_core::vocab::Reason;
use unfold_core::{Link, LinkTarget};

use alien::{Compiled, Project};

/// Compiles one alien file, `a.alien`, and checks the graph is valid.
fn compile(source: &str) -> Compiled {
    Project::new().file("a.alien", source).compile().valid()
}

/// The `calls` link labeled `text` in `a.alien`.
fn call(source: &str, text: &str) -> Link {
    compile(source).call("a.alien", text).clone()
}

/// Asserts the call labeled `text` is `unknown` for `reason`.
#[track_caller]
fn assert_reason(source: &str, text: &str, reason: Reason) {
    assert_eq!(call(source, text).to, LinkTarget::Unknown(reason), "{source}");
}

/// Asserts the call labeled `text` reaches the library `module`.
#[track_caller]
fn assert_library(source: &str, text: &str, module: &str) {
    match call(source, text).to {
        LinkTarget::Library(label) => assert_eq!(label.as_str(), module, "{source}"),
        other => panic!("{source}: expected the library {module}, got {other:?}"),
    }
}

/// Asserts the call labeled `text` reaches the project node with `key` (in `a.alien`).
#[track_caller]
fn assert_resolved(source: &str, text: &str, key: &str) {
    match call(source, text).to {
        LinkTarget::Project(target) => assert_eq!(target.as_str(), format!("a.alien#{key}"), "{source}"),
        other => panic!("{source}: expected a.alien#{key}, got {other:?}"),
    }
}
