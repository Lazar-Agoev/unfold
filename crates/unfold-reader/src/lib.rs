//! What people and agents see of a stored graph, computed from its records: owners, the
//! catalog, relations, verified source, contracts, the overlay rules, and boards. It never
//! parses source and never reads key text: kinds, parents, and files come from the records.
//!
//! # Shape
//!
//! ```text
//! after each commit (workspace):  catalog::definitions(file) ─► store       ReaderIndex::from_graph ─► ReaderCache
//! per request:                    Reader::file_graph(path) ─► FileTree ─► overlay rules, relations::outgoing
//!                                 ReaderCache::get(pool) ─► ReaderIndex ─► owner, reached_from, closure
//!                                 relations::callers(reader, index, key)    (the store's reverse index)
//!                                 contracts::Facts(reader, root) ─► call_contract, type_definition
//!                                 source::verify_file ─► Source ─► text, lines, UTF-16 ranges
//! ```
//!
//! - [`tree`]: one file's nodes with their structure ([`FileTree`]), and blocks found by key
//!   across files ([`Trees`], [`Blocks`]). Every view below walks it.
//! - [`catalog`]: the definitions that can be cards, their labels and qualified names, and the
//!   apps and entrances found by key.
//! - [`ReaderIndex`]: graph-wide owners, closures, and Reached from, shared through a
//!   [`ReaderCache`] until the next commit.
//! - [`relations`]: callers (from the store's reverse index) and a definition's outgoing links.
//! - [`source`]: source verified against the facts' hash, with UTF-8 to UTF-16 conversion.
//! - [`contracts`]: a call's contract, and classes and types with their uses.
//! - [`overlay`]: what names, combined steps, proposals, and calls opened in place may do.
//! - [`notes`]: the words for modules, reasons, and pairings, and [`notes::LinkEnd`].
//! - [`board`]: the `unfold-board` format, its checks, health, and files.

#![forbid(unsafe_code)]

pub mod board;
pub mod catalog;
mod closure;
pub mod code_view;
pub mod contracts;
pub mod drawing;
mod error;
mod index;
pub mod notes;
pub mod overlay;
mod reader_index;
pub mod relations;
pub mod source;
pub mod tree;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

pub use error::{ReaderError, Result};
pub use reader_index::{ReaderCache, ReaderIndex};
pub use tree::{Block, Blocks, FileTree, Trees};
