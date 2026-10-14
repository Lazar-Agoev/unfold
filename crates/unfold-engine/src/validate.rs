//! The graph-level validation hook every compile and update runs before committing.
//!
//! It runs `unfold_core`'s validator over the candidate graph, against the source snapshots when
//! the caller has them, and against each linked language's independent census of call sites, so
//! validation never just trusts extraction.

use unfold_core::Graph;
use unfold_core::validate::{Sources, Validator, Violation};

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
