//! The board commands' argument and result types (docs/ipc.md#boards).

use serde::{Deserialize, Serialize};
use unfold_core::RepoPath;
use unfold_reader::board::files::Revision;
use unfold_reader::board::{Anchor, Board, Health, Repair, Target};

use crate::commands::key_of;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// An `unfold-board` document as the window edits it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardDocument {
    /// Always `unfold-board`.
    #[cfg_attr(feature = "ts", ts(type = "\"unfold-board\""))]
    pub format: &'static str,
    /// Always 1.
    #[cfg_attr(feature = "ts", ts(type = "1"))]
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

/// What a window saves: the title and the pages. The folder and the file framing are Rust's.
/// A page's names, combined steps, guards, connections, frames, resources, notes, and proposed
/// steps are optional on input, as in the file; signatures may be `""` (signed on save).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
#[cfg_attr(
    feature = "ts",
    ts(
        type = "{ title: string; pages: Array<Pick<Page, \"id\" | \"title\" | \"defaultView\" | \"cards\"> & Partial<Omit<Page, \"id\" | \"title\" | \"defaultView\" | \"cards\">>> }"
    )
)]
pub struct BoardEdit(pub(super) serde_json::Value);

/// An open board, as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id (its file's folder-relative path).
    pub id: String,
    /// The file's name without `.json`.
    pub name: String,
    /// The board as last loaded or saved (descriptions, names, and combined steps signed).
    pub document: BoardDocument,
    /// The revision of the board's file then.
    pub revision: String,
    /// Its health against the project's graph.
    pub health: Health,
    /// The board list's label when something is wrong ("2 outdated").
    pub label: Option<String>,
}

/// A board as its file holds it, read without opening it (`read_board`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ReadBoardResult {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The file's name without `.json`.
    pub name: String,
    /// The board.
    pub document: BoardDocument,
    /// The file's revision.
    pub revision: String,
}

/// A board's health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardHealthView {
    /// The health.
    pub health: Health,
    /// The board list's label when something is wrong.
    pub label: Option<String>,
}

/// One board in the sidebar or Home's Boards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The file's name without `.json`.
    pub name: String,
    /// The title, when the file reads.
    pub title: Option<String>,
    /// The folder, or `None` at the project's top level.
    pub folder: Option<String>,
    /// How many pages it has.
    pub pages: u32,
    /// How many cards its pages have together.
    pub cards: u32,
    /// When it was last edited (its file's modification time), in milliseconds since the Unix
    /// epoch.
    pub updated_at_ms: i64,
    /// A label when something is wrong ("1 missing · 2 outdated"); `None` when healthy or while
    /// the project rests.
    pub label: Option<String>,
    /// Why the file can't be read, if it can't: not a valid board, or a name a board file
    /// can't have (it says which names are allowed).
    pub error: Option<String>,
}

/// A project's boards and empty folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardList {
    /// The project.
    pub project: ProjectId,
    /// Its boards, most recently edited first, then by id (at most 2,000).
    pub boards: Vec<BoardSummary>,
    /// Its folders with no board in them, sorted.
    pub empty_folders: Vec<String>,
}

/// One page of a board where a key is a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardRefHit {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub board: String,
    /// The board's title.
    pub title: String,
    /// The page's id.
    pub page: String,
    /// The page's title.
    pub page_title: String,
}

/// Where a key is on boards ("Already on …").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoardRefs {
    /// The pages, by board id, then page order (at most 50).
    pub refs: Vec<BoardRefHit>,
    /// More pages hold it than are listed.
    pub truncated: bool,
}

/// How many boards a folder rename moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RenamedFolder {
    /// The boards whose folder changed.
    pub boards: u32,
}

/// The arguments of a command about one board.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct BoardIdArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
}

/// The arguments of `new_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewBoardArgs {
    /// The project.
    pub project: ProjectId,
    /// The title ("Untitled board" when absent or blank).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub title: Option<String>,
    /// The folder, one name (the project's top level when absent).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub folder: Option<String>,
    /// The first page's title ("Page 1" when absent or blank).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub page_title: Option<String>,
}

/// The arguments of `save_board`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SaveBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The revision of the board's file the edited board builds on: the one last loaded or
    /// saved. A file that changed since, or isn't a valid board, refuses the save
    /// (`board_changed`); a deleted file is written again.
    pub base: Revision,
    /// Writes the edited board over the file whatever it holds now: the person chose their
    /// version (Save my version, Keep my edits).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub overwrite: Option<bool>,
    /// The edited board.
    pub board: BoardEdit,
}

/// The arguments of `set_board_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SetBoardFolderArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The folder, one name, or `None` for the project's top level.
    pub folder: Option<String>,
}

/// The arguments of `rename_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RenameFolderArgs {
    /// The project.
    pub project: ProjectId,
    /// The folder now.
    pub from: String,
    /// Its new name.
    pub to: String,
}

/// What a repair is about (a card, combined step, name, connection, resource card, frame, or
/// proposed step) on its page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum TargetSpec {
    /// A card, by id.
    Card(String),
    /// A combined step, by id.
    Combined(String),
    /// A block's name, by block key.
    Name(String),
    /// A connection, by id.
    Connection(String),
    /// A resource card, by id.
    Resource(String),
    /// A frame, by id.
    Frame(String),
    /// A proposed step, by id.
    Proposal(String),
}

/// An anchor a repair points a resource card or connection at: a block or entrance key, or
/// lines of an evidence file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged, deny_unknown_fields)]
pub enum AnchorSpec {
    /// A block key or an app's or entrance's key.
    Key {
        /// The key.
        key: String,
    },
    /// Lines of an evidence file.
    File {
        /// The file, relative to the project's folder.
        file: String,
        /// The first and last line, 1-based and inclusive.
        lines: [u32; 2],
    },
}

/// One repair a health item offers (its `repairs`), with what it needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepairKindSpec {
    /// Point reference `from` at `to` everywhere on the page (after a rename or move); the
    /// references to blocks inside `from` move with it.
    Reconnect {
        /// The missing key.
        from: String,
        /// The key it should point at.
        to: String,
    },
    /// Point anchor `anchor` (its position) of resource card or connection `item` at `to`.
    ReconnectAnchor {
        /// The resource card or connection.
        item: String,
        /// The anchor's position among its anchors (a health item's `anchor`).
        anchor: u32,
        /// The new anchor.
        to: AnchorSpec,
    },
    /// Keep a description's, name's, or combined step's text and sign it against the current
    /// source (a resource card or connection with its texts).
    Resign {
        /// Whose.
        target: TargetSpec,
    },
    /// Accept a held reference's current code (save a fresh guard).
    Accept {
        /// The reference.
        key: String,
    },
    /// Remove an item: a card, resource card, or frame with the connections to and from it.
    Remove {
        /// The item.
        target: TargetSpec,
    },
    /// Remove the description of a card, combined step, resource card, connection, or frame.
    RemoveDescription {
        /// Whose.
        target: TargetSpec,
    },
}

/// A repair on one page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RepairSpec {
    /// The page.
    pub page: String,
    /// The repair.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub repair: RepairKindSpec,
}

impl TargetSpec {
    fn into_target(self) -> Target {
        match self {
            Self::Card(id) => Target::Card(id),
            Self::Combined(id) => Target::Combined(id),
            Self::Name(key) => Target::Name(key),
            Self::Connection(id) => Target::Connection(id),
            Self::Resource(id) => Target::Resource(id),
            Self::Frame(id) => Target::Frame(id),
            Self::Proposal(id) => Target::Proposal(id),
        }
    }
}

impl AnchorSpec {
    /// The board's anchor (`invalid_args` for a malformed key, path, or line range).
    fn into_anchor(self) -> CommandResult<Anchor> {
        match self {
            Self::Key { key } => Ok(Anchor::Key { key: key_of(&key)? }),
            Self::File {
                file,
                lines: [first, last],
            } => {
                let file = RepoPath::new(&file).map_err(|_| {
                    CommandError::invalid_args(format!(
                        "`{file}` is not a path relative to the project's folder"
                    ))
                })?;
                if first == 0 || last < first {
                    return Err(CommandError::invalid_args(
                        "an anchor's lines are 1-based, the last not before the first",
                    ));
                }
                Ok(Anchor::File {
                    file,
                    lines: [first, last],
                })
            }
        }
    }
}

impl RepairKindSpec {
    /// The reader's repair (`invalid_args` for a malformed anchor).
    pub(super) fn into_repair(self) -> CommandResult<Repair> {
        Ok(match self {
            Self::Reconnect { from, to } => Repair::Reconnect { from, to },
            Self::ReconnectAnchor { item, anchor, to } => Repair::ReconnectAnchor {
                item,
                index: anchor as usize,
                to: to.into_anchor()?,
            },
            Self::Resign { target } => Repair::Resign(target.into_target()),
            Self::Accept { key } => Repair::Accept(key),
            Self::Remove { target } => Repair::Remove(target.into_target()),
            Self::RemoveDescription { target } => Repair::RemoveDescription(target.into_target()),
        })
    }
}

/// The arguments of `repair_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RepairBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The revision of the board's file the window's version builds on, as for `save_board`
    /// (`board_changed` when the file changed since).
    pub base: Revision,
    /// The repair.
    pub repair: RepairSpec,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_name_their_page_and_refuse_unknown_fields() {
        let spec: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "page-2", "kind": "accept", "key": "a.ts#f"
        }))
        .expect("parse");
        assert_eq!(spec.page, "page-2");
        assert_eq!(
            spec.repair.into_repair().expect("repair"),
            Repair::Accept("a.ts#f".to_owned())
        );
        let target: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "resign", "target": { "combined": "k" }
        }))
        .expect("parse");
        assert_eq!(
            target.repair.into_repair().expect("repair"),
            Repair::Resign(Target::Combined("k".to_owned()))
        );
        for bad in [
            serde_json::json!({ "page": "p", "kind": "remove", "target": { "card": "c" }, "extra": 1 }),
            serde_json::json!({ "kind": "remove", "target": { "card": "c" } }),
            serde_json::json!({ "page": "p", "kind": "remove", "target": { "label": "k" } }),
            serde_json::json!({ "page": "p", "kind": "explode" }),
            serde_json::json!({ "page": "p", "kind": "accept", "project": "web", "key": "k" }),
        ] {
            assert!(
                serde_json::from_value::<RepairSpec>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn anchors_are_reconnected_by_position_to_a_key_or_lines() {
        let repair = |to: serde_json::Value| {
            serde_json::from_value::<RepairSpec>(serde_json::json!({
                "page": "p", "kind": "reconnect_anchor", "item": "orders", "anchor": 1, "to": to
            }))
            .expect("parse")
            .repair
            .into_repair()
        };
        assert_eq!(
            repair(serde_json::json!({ "file": "db/schema.sql", "lines": [2, 4] })),
            Ok(Repair::ReconnectAnchor {
                item: "orders".to_owned(),
                index: 1,
                to: Anchor::File {
                    file: RepoPath::new("db/schema.sql").expect("path"),
                    lines: [2, 4],
                },
            })
        );
        assert_eq!(
            repair(serde_json::json!({ "key": "a.ts#queue" })),
            Ok(Repair::ReconnectAnchor {
                item: "orders".to_owned(),
                index: 1,
                to: Anchor::Key {
                    key: unfold_core::Key::from_text("a.ts#queue"),
                },
            })
        );
        for bad in [
            serde_json::json!({ "key": "" }),
            serde_json::json!({ "file": "../schema.sql", "lines": [1, 1] }),
            serde_json::json!({ "file": "db/schema.sql", "lines": [3, 2] }),
            serde_json::json!({ "file": "db/schema.sql", "lines": [0, 2] }),
        ] {
            assert_eq!(
                repair(bad.clone()).expect_err("refused").code,
                "invalid_args",
                "{bad}"
            );
        }
        let removed: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "remove", "target": { "resource": "orders" }
        }))
        .expect("parse");
        assert_eq!(
            removed.repair.into_repair(),
            Ok(Repair::Remove(Target::Resource("orders".to_owned())))
        );
        let unnamed: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "remove", "target": { "name": "a.ts#f/call:g" }
        }))
        .expect("parse");
        assert_eq!(
            unnamed.repair.into_repair(),
            Ok(Repair::Remove(Target::Name("a.ts#f/call:g".to_owned())))
        );
        let resign: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "resign", "target": { "frame": "server" }
        }))
        .expect("parse");
        assert_eq!(
            resign.repair.into_repair(),
            Ok(Repair::Resign(Target::Frame("server".to_owned())))
        );
    }
}
