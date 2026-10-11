//! Derived views over a stored graph: owners, the catalog, activation closures, applications,
//! relations, the Flow model, the code render model, typed lines, and Mermaid export; and boards (the
//! `unfold-board` format with pages, its JSON Schema, validation, signatures, guards, drawn
//! links, health across projects, drafts, unpublished changes, and publishing). It never parses source; highlighting arrives through a `Highlighter` trait.
//!
//! `docs/reader.md` is the authoritative description of the rules implemented here.
//!
//! # Shape
//!
//! ```text
//! after each update (workspace):   project(&graph, &launches) ─► Projections ─► store commit
//! per request:                     ReaderCache::get(pool) ─► ReaderIndex ─► callers, reached-from, closures
//!                                  Reader::file_graph(path) ─► outgoing(), code_view()
//!                                  contracts::Facts(reader) ─► line_label(), call_contract(), type_definition()
//! ```
//!
//! - [`project`] computes the rows the store keeps (definitions, applications, memberships)
//!   from the whole in-memory graph.
//! - [`ReaderIndex`] answers graph-wide questions from a compact [`GraphIndex`] of one graph
//!   version, with closures cached per seed; [`ReaderCache`] holds it until the next commit.
//! - [`code::code_view`] builds what a Code card or source tab renders, in UTF-16 offsets, from
//!   one file's stored facts and bytes verified against the facts' hash ([`source`]).
//! - [`flow::flow_view`] builds a card's Flow (paths from written structure) from the same facts.
//! - [`contracts`] builds typed lines (an arrow's label, a call's contract, a type's definition
//!   and uses) from one or two files' typed records, per request.
//! - [`board`] reads, checks, signs, and writes `unfold-board` files against
//!   [`board::PageGraphs`] (one [`board::BoardGraph`] of the files a board touches per connected
//!   project); [`mermaid`] exports a board page, an entrance, or a Flow.

#![forbid(unsafe_code)]

mod applications;
pub mod board;
pub mod catalog;
mod closure;
pub mod code;
pub mod contracts;
mod error;
pub mod flow;
mod index;
pub mod launches;
pub mod mermaid;
pub mod notes;
mod project;
mod reader_index;
pub mod relations;
pub mod source;

#[cfg(test)]
mod tests;

pub use error::{ReaderError, Result};
pub use index::{BlockId, GraphIndex, GraphIndexBuilder, file_owners};
pub use project::{Projections, project};
pub use reader_index::{ReaderCache, ReaderIndex};
