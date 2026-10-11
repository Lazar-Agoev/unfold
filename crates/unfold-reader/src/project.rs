//! The reader projections the store keeps (crates/unfold-store/src/schema.rs): definitions per file,
//! applications, and memberships, computed from a whole graph and its launch records after every
//! update.
//!
//! [`project`] is a pure function of the graph and launch records: one index pass, one catalog
//! pass, and one closure per distinct launch seed set. The workspace compares the result with the previous projections and
//! commits what changed (per-file definitions in `FileChange::definitions`, the small sets
//! whole).

use unfold_core::{Graph, Launches};
use unfold_store::{Application, Definition, Membership};

use crate::applications::applications;
use crate::catalog::catalog;
use crate::index::GraphIndex;

/// What the store keeps for the reader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projections {
    /// Each file's definitions in preorder, aligned with the graph's files.
    pub definitions: Vec<Vec<Definition>>,
    /// Applications, sorted by ID.
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

/// Computes the projections of `graph` and its launch records.
pub fn project(graph: &Graph, launches: &Launches) -> Projections {
    let index = GraphIndex::from_graph(graph, launches);
    let (applications, memberships) = applications(graph, &index, launches);
    Projections {
        definitions: catalog(graph, &index),
        applications,
        memberships,
    }
}
