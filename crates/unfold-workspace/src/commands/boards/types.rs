//! The board commands' argument and result types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::board::files::Place;
use unfold_reader::board::{Board, BoardOutline, Health, Repair, Target};

use crate::events::WindowId;

/// An `unfold-board` version 1 document, exactly as its file holds it (architecture §9.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardDocument {
    /// Always `unfold-board`.
    #[ts(type = "\"unfold-board\"")]
    pub format: &'static str,
    /// Always 1.
    #[ts(type = "1")]
    pub version: u32,
    /// The board's fields.
    #[serde(flatten)]
    pub board: Board,
}

impl BoardDocument {
    pub(super) fn new(board: Board) -> Self {
        Self {
            format: unfold_reader::board::FORMAT,
            version: unfold_reader::board::VERSION,
            board,
        }
    }
}

/// An open board, as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    /// The board's ID (its folder-relative file path).
    pub id: String,
    /// Published or draft.
    pub place: Place,
    /// The file name without `.json`.
    pub name: String,
    /// The document, as last loaded or saved (descriptions signed).
    pub document: BoardDocument,
    /// The file's revision (the SHA-256 of its bytes).
    pub revision: String,
    /// Its health against the current graph.
    pub health: Health,
    /// Its outline: cards in follow order, and Flow nodes without labels.
    pub outline: BoardOutline,
    /// The Boards view's label when something is wrong ("2 outdated").
    pub label: Option<String>,
}

/// One board in the Boards view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    /// The board's ID.
    pub id: String,
    /// Published or draft.
    pub place: Place,
    /// The file name without `.json`.
    pub name: String,
    /// The title, when the file reads.
    pub title: Option<String>,
    /// How many cards it has.
    pub cards: u32,
    /// A label when something is wrong ("1 missing · 2 outdated").
    pub label: Option<String>,
    /// The window that has it open, if any.
    pub held_by: Option<WindowId>,
    /// Why the file can't be read, if it can't.
    pub error: Option<String>,
}

/// The Boards view: published boards and drafts on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardList {
    /// `.unfold/boards/`, by name.
    pub published: Vec<BoardSummary>,
    /// `local/drafts/`, by name.
    pub drafts: Vec<BoardSummary>,
}

/// The arguments of a command about one board.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BoardIdArgs {
    /// The board's ID.
    pub id: String,
}

/// What `open_board` did.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum OpenBoardResult {
    /// The board is now open in the calling window.
    Opened {
        /// The board.
        board: Box<BoardState>,
    },
    /// Another window has it open: ask the main process to focus that window.
    Held {
        /// The window holding it.
        held_by: WindowId,
    },
}

/// The arguments of `new_board`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NewBoardArgs {
    /// The title ("Untitled board" when absent).
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
}

/// The arguments of `move_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveBoardArgs {
    /// The board's ID.
    pub id: String,
    /// The window to move it to; it must show the same folder.
    pub to_window: WindowId,
}

/// The arguments of `save_board`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveBoardArgs {
    /// The board's ID.
    pub id: String,
    /// The edited document.
    #[ts(as = "BoardDocument")]
    pub board: serde_json::Value,
    /// Write even though the file changed on disk since it was loaded (keep this version).
    #[serde(default)]
    #[ts(optional)]
    pub force: Option<bool>,
}

/// The arguments of `publish_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PublishBoardArgs {
    /// The draft's ID.
    pub id: String,
    /// The file name (the slug of the title when absent).
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
}

/// The arguments of `rename_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameBoardArgs {
    /// The board's ID.
    pub id: String,
    /// The new title.
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    /// A new file name, for a published board (never overwriting another board).
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
}

/// What a command that names a published file did.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum BoardWriteOutcome {
    /// Done; the board as it is now (its ID may have changed).
    Done {
        /// The board.
        board: Box<BoardState>,
    },
    /// A board file already has that name; nothing was written. Offer `suggestion`.
    NameTaken {
        /// The existing file.
        path: String,
        /// A free name.
        suggestion: String,
    },
}

/// A board's health and outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardHealthView {
    /// The health.
    pub health: Health,
    /// The outline.
    pub outline: BoardOutline,
    /// The Boards view's label when something is wrong.
    pub label: Option<String>,
}

/// What a repair is about (a card, group, or label).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TargetSpec {
    /// A card, by ID.
    Card(String),
    /// A group, by ID.
    Group(String),
    /// A Flow label, by block key.
    Label(String),
}

impl From<TargetSpec> for Target {
    fn from(target: TargetSpec) -> Self {
        match target {
            TargetSpec::Card(id) => Self::Card(id),
            TargetSpec::Group(id) => Self::Group(id),
            TargetSpec::Label(key) => Self::Label(key),
        }
    }
}

/// A repair a health item offers (its `repairs`), with what it needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum RepairSpec {
    /// Point reference `from` at `to` everywhere (after a rename or move).
    Reconnect {
        /// The missing key.
        from: String,
        /// The key it should point at.
        to: String,
    },
    /// Remove a card and its groups.
    RemoveCard {
        /// The card's ID.
        card: String,
    },
    /// Remove a Flow label.
    RemoveLabel {
        /// The block key.
        key: String,
    },
    /// Remove a group, keeping its statements.
    Ungroup {
        /// The group's ID.
        group: String,
    },
    /// Remove a card's or group's description.
    RemoveDescription {
        /// Whose.
        target: TargetSpec,
    },
    /// Keep a description's text and sign it against the current source.
    Resign {
        /// Whose.
        target: TargetSpec,
    },
    /// Accept a held reference's current code (save a fresh guard).
    Accept {
        /// The reference.
        key: String,
    },
}

impl From<RepairSpec> for Repair {
    fn from(repair: RepairSpec) -> Self {
        match repair {
            RepairSpec::Reconnect { from, to } => Self::Reconnect { from, to },
            RepairSpec::RemoveCard { card } => Self::RemoveCard(card),
            RepairSpec::RemoveLabel { key } => Self::RemoveLabel(key),
            RepairSpec::Ungroup { group } => Self::Ungroup(group),
            RepairSpec::RemoveDescription { target } => Self::RemoveDescription(target.into()),
            RepairSpec::Resign { target } => Self::Resign(target.into()),
            RepairSpec::Accept { key } => Self::Accept(key),
        }
    }
}

/// The arguments of `repair_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RepairBoardArgs {
    /// The board's ID.
    pub id: String,
    /// The repair.
    pub repair: RepairSpec,
}
