//! The graph-level validation hook every compile and update runs before committing.
//!
//! It runs `unfold_core`'s validator over the candidate graph, against the source snapshots when
//! the caller has them, and against each linked language's independent census of call sites, so
//! validation never just trusts extraction. The apps are resolved after the graph, and again
//! when only the `[[app]]` tables change, so they are checked as they are attached
//! ([`validate_apps`]).

use unfold_core::validate::{Sources, Validator, Violation};
use unfold_core::{App, Graph};

use crate::language::Linked;

/// Every violation in `graph`, in a deterministic order. An empty result means the graph may be
/// committed.
pub fn validate_graph(
    graph: &Graph,
    sources: Option<&dyn Sources>,
    languages: &[&dyn Linked],
) -> Vec<Violation> {
    let mut validator = Validator::new();
    if let Some(sources) = sources {
        validator = validator.with_sources(sources);
    }
    for census in languages.iter().filter_map(|linked| linked.census()) {
        validator = validator.with_census(census);
    }
    validator.validate(graph)
}

/// Checks `apps` as they are attached to a graph that was validated without them, by the rules
/// [`validate_graph`] applies to a graph's apps: keys re-derived from names and unique, and every
/// entrance starting somewhere. The settings parser keeps out every entry that would break them,
/// so a violation is the engine's error, never a bad `unfold.toml` entry.
pub(crate) fn validate_apps(apps: Vec<App>) -> Result<Vec<App>, Vec<Violation>> {
    let graph = Graph {
        apps,
        ..Graph::default()
    };
    let violations = Validator::new().validate(&graph);
    if violations.is_empty() {
        Ok(graph.apps)
    } else {
        Err(violations)
    }
}
