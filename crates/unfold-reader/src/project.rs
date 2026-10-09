//! The reader projections the store keeps (architecture §8.3): definitions per file,
//! applications, and memberships, computed from a whole graph after every update.
//!
//! [`project`] is a pure function of the graph: one index pass, one catalog pass, and one
//! closure per application. The workspace compares the result with the previous projections and
//! commits what changed (per-file definitions in `FileChange::definitions`, the small sets
//! whole).

use unfold_core::Graph;
use unfold_store::{Application, Definition, Membership};

use crate::applications::applications;
use crate::catalog::catalog;
use crate::index::GraphIndex;

/// What the store keeps for the reader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projections {
    /// Each file's definitions in preorder, aligned with the graph's files.
    pub definitions: Vec<Vec<Definition>>,
    /// Applications, sorted by startup key.
    pub applications: Vec<Application>,
    /// Memberships, sorted by entrance key, then application.
    pub memberships: Vec<Membership>,
}

impl Projections {
    /// The definitions of the file at position `file` in the graph's files (empty past the end).
    pub fn file_definitions(&self, file: usize) -> &[Definition] {
        self.definitions.get(file).map_or(&[], Vec::as_slice)
    }
}

/// Computes the projections of `graph`.
pub fn project(graph: &Graph) -> Projections {
    let index = GraphIndex::from_graph(graph);
    let (applications, memberships) = applications(graph, &index);
    Projections {
        definitions: catalog(graph, &index),
        applications,
        memberships,
    }
}
