//! The canonical export: a deterministic JSON Lines form of the whole graph.
//!
//! It is used for snapshots, parity with the proof of concept, determinism checks, and
//! debugging. Records refer to structural keys, never to local IDs, and reading an export and
//! writing it again gives identical bytes. `docs/graph-model.md` specifies every record.

mod read;
mod view;
mod write;

pub use read::{CanonicalError, read, read_versioned};
pub use view::version_one;
pub use write::{to_string, write, write_version};

/// The `format` field of the header record.
pub const FORMAT: &str = "unfold-graph";
/// The `version` field of the header record the writer produces.
pub const VERSION: u32 = 2;
/// The oldest `version` the reader accepts. Version 1 exports have no typed records (the frozen
/// proof-of-concept references are version 1).
pub const OLDEST_VERSION: u32 = 1;

#[cfg(test)]
mod tests;
