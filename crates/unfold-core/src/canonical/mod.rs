//! The canonical export: a deterministic JSON Lines form of the whole graph
//! (docs/graph-model.md#order-and-canonical-export).
//!
//! It is used for goldens, the oracle tooling, determinism checks, and debugging. Each line is
//! one record, named by its `t`:
//!
//! | `t` | One per |
//! | --- | --- |
//! | `header` | export: `format` and `version` |
//! | `file` | file, in path order, followed by its records below |
//! | `node` | node of the file, in preorder |
//! | `link` | link of the file, by `from` and `seq` |
//! | `signature`, `parameter`, `member`, `argument` | typed record of the file, by node and index |
//! | `input` | input, in path order |
//! | `app` | app, in file order, followed by its records below |
//! | `entrance` | entrance of the app, in file order, followed by its targets |
//! | `entrance_target` | `code` key of the app or entrance before it, in order |
//!
//! Records refer to nodes by key, never by local ID, and leave out absent fields, false flags,
//! and empty flag sets. Ranges are UTF-8 byte ranges; a node's `lines` are for reading. The
//! export has one version, and reading an export and writing it again gives identical bytes.

mod read;
mod write;

pub use read::{CanonicalError, read};
pub use write::{to_string, write};

/// The `format` field of the header record.
pub const FORMAT: &str = "unfold-graph";
/// The `version` field of the header record.
pub const VERSION: u32 = 2;

#[cfg(test)]
mod tests;
