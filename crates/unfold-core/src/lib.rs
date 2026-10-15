//! The contract every Unfold crate shares: the graph's records (nodes, links, typed records,
//! apps and entrances) and their vocabularies, structural keys, the ownership rule, source text
//! (snapshots, SHA-256, the line index, UTF-8/UTF-16 offsets), and the `Highlighter` contract
//! for the code panel.
//!
//! `unfold-core` depends on no other Unfold crate. docs/graph-model.md describes the model this
//! crate encodes.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod highlight;
pub mod key;
pub mod model;
pub mod path;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod text;
pub mod typed;
pub mod validate;
pub mod vocab;

pub use key::{Callee, Child, Key, KeyBuilder, ParsedKey};
pub use model::{
    App, Entrance, EntranceTarget, File, FileGraph, Graph, Input, Link, LinkTarget, ModuleLabel, Node, owner,
};
pub use path::RepoPath;
pub use text::{ByteRange, ContentHash, LineBreaks, LineIndex, LineSpan, Snapshot, Utf16Index};
pub use typed::{Argument, FileTypes, Member, Pairing, Parameter, Signature, WrittenType};
