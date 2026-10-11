//! The contract every Unfold crate shares: the graph model and its vocabularies, structural
//! keys, source text (snapshots, SHA-256, the line index, UTF-8/UTF-16 offsets), the graph
//! validator, the canonical export, and the `Highlighter` contract for the Code view.
//!
//! `unfold-core` depends on no other Unfold crate. `docs/graph-model.md` describes the
//! semantics this crate encodes.

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

pub use key::{Key, KeyBuilder};
pub use model::{
    Block, Edge, Entrance, EntranceTarget, Evidence, FileGraph, FileRecord, Graph, InputRecord, ModuleLabel,
    Slot,
};
pub use path::RepoPath;
pub use text::{ByteRange, ContentHash, LineIndex, LineSpan, Snapshot, Utf16Index};
pub use typed::{
    Argument, CallResult, FileTypes, Member, Pairing, Parameter, Signature, TypeLink, WrittenType,
};
pub use validate::{Census, Validator, Violation, validate};
