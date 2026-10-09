//! The canonical export: a deterministic JSON Lines form of the whole graph.
//!
//! It is used for snapshots, parity with the proof of concept, determinism checks, and
//! debugging. Records refer to structural keys, never to local IDs, and reading an export and
//! writing it again gives identical bytes. `docs/graph-model.md` specifies every record.

mod read;
mod write;

pub use read::{CanonicalError, read};
pub use write::{to_string, write};

/// The `format` field of the header record.
pub const FORMAT: &str = "unfold-graph";
/// The `version` field of the header record.
pub const VERSION: u32 = 1;

#[cfg(test)]
mod tests;
