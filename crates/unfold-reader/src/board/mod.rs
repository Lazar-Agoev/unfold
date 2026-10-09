//! Boards (architecture §9.1, §10.5; decision U-22; contract §6): the `unfold-board`
//! version 1 format with pages, its JSON Schema, strict parsing, deterministic serialization,
//! signatures, guards, groups, drawn links, health with repairs per page and per project, page
//! outlines, the unpublished-changes count, and the board files of a project.
//!
//! ```text
//! text ─parse─► Board ─health(Page, PageGraphs)─► Health, repairs
//!   edit ─parse_edit─┘ └─prepare_save(previous, PageGraphs)─► signed Board ─serialize─► text ─BoardDirs─► disk
//!                      └─outline(Page, PageGraphs), changes(working, published), mermaid::page
//! ```
//!
//! A page's cards can come from several projects: each project's references are read through
//! its own [`BoardGraph`] ([`BoardGraph::load`] with [`Board::references`] of that project),
//! gathered in [`PageGraphs`]. Everything except [`files`] and [`BoardGraph::load`] is pure.

mod changes;
mod check;
pub mod files;
mod graph;
mod json;
mod model;
mod outline;
mod parse;
mod save;

pub use changes::changes;
pub use check::{
    Health, HealthItem, HealthKind, ProjectKey, Repair, RepairKind, Target, apply_repair, board_health,
    current_guard, health,
};
pub use graph::{BlockAt, BoardGraph, PageGraphs};
pub use json::{serialize, serialize_changes};
pub use model::{
    Author, AuthorKind, Board, Card, CardRef, Description, FORMAT, Group, GuardEntry, LabelEntry, Link, Page,
    ProjectName, SCHEMA, Signature, VERSION, View, is_folder_path,
};
pub use outline::{CardLink, OutlineRow, PageOutline, Unlabeled, card_links, outline};
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
    /// A reference, group, label, or link breaks a rule against the current graphs.
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
        /// The card, group, key, or link.
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
    /// A file with that name exists; a draft's first Publish never overwrites a board.
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
