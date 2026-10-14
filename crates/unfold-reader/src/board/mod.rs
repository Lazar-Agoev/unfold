//! Boards (docs/reader.md#boards): the `unfold-board` version 1
//! format with pages, its JSON Schema, strict parsing, deterministic serialization, signatures,
//! guards, the page's overlay (names, combined steps, proposed steps, and calls opened in place)
//! checked against the overlay rules, connections, frames, resource cards, notes, the claims'
//! signatures and grounding, health with repairs per page, and the board files of a project.
//!
//! ```text
//! text ─parse─► Board ─health(Page, BoardGraph)─► Health, repairs
//!   edit ─parse_edit─┘ └─prepare_save(previous, BoardGraph)─► signed Board ─serialize─► text ─BoardDirs─► disk
//! ```
//!
//! A board shows one project: its references are read through that project's [`BoardGraph`]
//! ([`BoardGraph::load`] with [`Board::references`]). Everything except [`files`] and
//! [`BoardGraph::load`] is pure.

mod check;
mod claims;
pub mod files;
mod graph;
mod items;
mod json;
mod model;
mod parse;
mod save;

pub use check::{
    Health, HealthItem, HealthKind, Repair, RepairKind, Target, apply_repair, board_health, current_guard,
    health,
};
pub use graph::{BoardGraph, Lines};
pub use json::serialize;
pub use model::{
    Anchor, Author, AuthorKind, Board, Card, CardRef, Cardinality, Column, ColumnKey, Combined, Connection,
    ConnectionKind, Description, FORMAT, Frame, FrameDirection, FrameIcon, FrameLook, GuardEntry,
    MAX_FRAME_DEPTH, MAX_NOTE_CHARACTERS, MAX_PROPOSAL_REASON, MAX_PROPOSAL_TITLE, MAX_PROPOSALS, NameEntry,
    Note, Page, Place, Proposal, References, Resource, ResourceKind, SCHEMA, Signature, VERSION, View,
    is_folder_name,
};
pub use parse::{parse, parse_edit};
pub use save::prepare_save;

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
    /// A reference, overlay item, or claim breaks a rule against the current graph.
    #[error("page `{page}`, {target}: {message}")]
    Problem {
        /// The page it is on.
        page: String,
        /// What it is about.
        target: Target,
        /// What is wrong.
        message: String,
    },
    /// Text cannot be signed: the source is missing, changed, or unreadable.
    #[error("cannot sign {what}: its source is missing or changed; compile first")]
    Unverified {
        /// The card, name, combined step, claim, or key.
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
    /// A file with that name exists; a new board never overwrites one.
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
mod claims_tests;
#[cfg(test)]
mod tests;
