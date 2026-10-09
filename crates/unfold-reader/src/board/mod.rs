//! Boards (architecture §9.1, §10.5; decision U-22): the `unfold-board` version 1 format, its
//! JSON Schema, strict parsing, deterministic serialization, signatures, guards, groups,
//! health with repairs, the board outline, and the board files of a workspace.
//!
//! ```text
//! text ─parse─► Board ─check/health(BoardGraph)─► Health, repairs
//!                 │ └─prepare_save(previous, BoardGraph)─► signed Board ─serialize─► text ─files::save─► disk
//!                 └─outline(BoardGraph), mermaid::board(BoardGraph)
//! ```
//!
//! Everything except [`files`] and [`BoardGraph::load`] is pure.

mod check;
pub mod files;
mod graph;
mod json;
mod model;
mod outline;
mod parse;
mod save;

pub use check::{
    Health, HealthItem, HealthKind, Repair, RepairKind, Target, apply_repair, check, current_guard,
    group_problem, guard_problem, health,
};
pub use graph::{BlockAt, BoardGraph};
pub use json::serialize;
pub use model::{
    Author, AuthorKind, Board, Card, CardRef, Description, FORMAT, Group, Guard, SCHEMA, Signature, VERSION,
    View, is_ordinal,
};
pub use outline::{BoardOutline, CardLink, OutlineRow, Unlabeled, card_links, outline};
pub use parse::parse;
pub use save::{describe_card, describe_group, prepare_save, set_label, sign};

/// Why a board could not be read, checked, or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BoardError {
    /// The text is not valid JSON (or has a duplicate key).
    #[error("board file is not valid JSON at line {line}, column {column}: {message}")]
    Syntax {
        /// The one-based line (0 when unknown).
        line: usize,
        /// The one-based column (0 when unknown).
        column: usize,
        /// What the JSON reader said.
        message: String,
    },
    /// A value breaks the `unfold-board` shape.
    #[error("invalid board at {path}: {message}")]
    Invalid {
        /// The JSON pointer of the value.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// A reference or group breaks a rule against the current graph.
    #[error("{target:?}: {message}")]
    Problem {
        /// What it is about.
        target: Target,
        /// What is wrong.
        message: String,
    },
    /// Text cannot be signed: the source is missing, changed, or unreadable.
    #[error("cannot sign {what}: its source is missing or changed; compile first")]
    Unverified {
        /// The card, group, or key.
        what: String,
    },
    /// A board file name is not a plain stem.
    #[error(
        "board name `{name}` must use letters, digits, `.`, `_`, or `-`, starting with a letter or digit"
    )]
    InvalidName {
        /// The refused name.
        name: String,
    },
    /// A file with that name exists; boards are never overwritten by Publish or Rename.
    #[error("{path} already exists; choose another name (for example {suggestion})")]
    NameTaken {
        /// The existing file, relative to the folder.
        path: String,
        /// A free name to offer.
        suggestion: String,
    },
    /// Reading or writing a file failed.
    #[error("{path}: {message}")]
    Io {
        /// The file or directory.
        path: String,
        /// The system's message.
        message: String,
    },
}

#[cfg(test)]
mod tests;
