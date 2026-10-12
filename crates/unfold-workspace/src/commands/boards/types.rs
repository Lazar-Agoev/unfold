//! The board commands' argument and result types (docs/ipc.md › Boards).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::{Key, RepoPath};
use unfold_reader::board::files::{Place, Revision};
use unfold_reader::board::{
    Anchor, Board, Health, PageOutline, ProjectKey, ProjectName, Repair, Target, View,
};

use crate::events::WindowId;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::state::BoardView;

/// An `unfold-board` document as the window edits it: the board's working version, without a
/// changes copy's `publishes` and `baseRevision`.
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

/// What a window saves: the title and the pages. The folder, the `projects`
/// list, and the file framing are Rust's. Labels, guards, and links (and frames, resources, and
/// notes, as in the file) are optional on input; signatures may be `""` (signed on save).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    type = "{ title: string; pages: Array<Omit<Page, \"labels\" | \"guards\" | \"links\"> & Partial<Pick<Page, \"labels\" | \"guards\" | \"links\">>> }"
)]
pub struct BoardEdit(pub(super) serde_json::Value);

/// A published board's publish state, as of the last open, save, or publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishedState {
    /// The published file's revision now, or `None` when it was deleted on disk.
    pub revision: Option<String>,
    /// How many unpublished changes the working version has (0: none).
    pub changes: u32,
    /// Whether the published file changed since the changes copy started from it (a pull).
    pub base_changed: bool,
}

impl PublishedState {
    /// A published board read from its published file, at `revision`: no unpublished changes.
    pub(super) fn current(revision: Revision) -> Self {
        Self {
            revision: Some(revision.0.to_string()),
            changes: 0,
            base_changed: false,
        }
    }
}

/// An open board, as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardState {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id (its public file's folder-relative path).
    pub id: String,
    /// Published or draft.
    pub place: Place,
    /// The public file's name without `.json`.
    pub name: String,
    /// The working version, as last loaded or saved (descriptions and labels signed).
    pub document: BoardDocument,
    /// The revision of the file the board's session reads and writes.
    pub revision: String,
    /// For a published board, its publish state; `None` for a draft.
    pub published: Option<PublishedState>,
    /// Its health against the graphs of the projects its cards come from.
    pub health: Health,
    /// Each page's outline, in page order.
    pub outlines: Vec<PageOutline>,
    /// The board list's label when something is wrong ("2 outdated").
    pub label: Option<String>,
    /// The view state remembered in `state.json`.
    pub view: Option<BoardView>,
}

/// A board as its working file holds it, read without opening it (`read_board`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReadBoardResult {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// Published or draft.
    pub place: Place,
    /// The public file's name without `.json`.
    pub name: String,
    /// The working version: the draft, the changes copy, or the published file.
    pub document: BoardDocument,
    /// The working file's revision.
    pub revision: String,
    /// For a published board, its publish state, as `open_board` reports it; `None` for a draft.
    pub published: Option<PublishedState>,
    /// The window that has it open, if any.
    pub held_by: Option<WindowId>,
}

/// A board's health and outlines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardHealthView {
    /// The health.
    pub health: Health,
    /// Each page's outline, in page order.
    pub outlines: Vec<PageOutline>,
    /// The board list's label when something is wrong.
    pub label: Option<String>,
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

/// One board in the sidebar tree, Home's Boards, or Drafts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// Published or draft.
    pub place: Place,
    /// The public file's name without `.json`.
    pub name: String,
    /// The title, when the file reads.
    pub title: Option<String>,
    /// The folder, or `None` at the project's top level.
    pub folder: Option<String>,
    /// How many pages it has.
    pub pages: u32,
    /// How many cards its pages have together.
    pub cards: u32,
    /// The other projects its cards come from (shared names).
    pub projects: Vec<ProjectName>,
    /// For a published board, its unpublished changes (0: none); 0 for a draft.
    pub changes: u32,
    /// When the working file (the changes copy, when there is one) was last written, in
    /// milliseconds since the Unix epoch.
    pub updated_at_ms: i64,
    /// The window that has it open, if any.
    pub held_by: Option<WindowId>,
    /// A label when something is wrong ("1 missing · 2 outdated"); `None` when healthy or while
    /// the project rests.
    pub label: Option<String>,
    /// Why the file can't be read, if it can't.
    pub error: Option<String>,
}

/// A project's boards and empty folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardList {
    /// The project.
    pub project: ProjectId,
    /// Its published boards, then its drafts, each by name (at most 2,000).
    pub boards: Vec<BoardSummary>,
    /// Its folders with no board in them, sorted.
    pub empty_folders: Vec<String>,
}

/// Every project's drafts and published boards with unpublished changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Drafts {
    /// Most recently written first (at most 500).
    pub drafts: Vec<BoardSummary>,
}

/// A thumbnail's page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailPage {
    /// The page's id.
    pub id: String,
    /// Its title.
    pub title: String,
}

/// What a card is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    /// An entrance card.
    Entrance,
    /// A block card of a type declaration (an interface, type alias, or enum): a type card, which
    /// shows its fields in Diagram and Flow view.
    Type,
    /// A block card of a class: a type card in Diagram view.
    Class,
    /// Any other block card.
    Block,
}

/// One card of a thumbnail. The window sizes it with the canvas's own rules.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailCard {
    /// The card's id.
    pub id: String,
    /// Its position on the canvas.
    pub at: [f64; 2],
    /// Entrance or block.
    pub kind: CardKind,
    /// The view it shows.
    pub view: View,
    /// Whether it is folded.
    pub folded: bool,
    /// The block's line count (`None`: an entrance, a missing block, or a project that is not
    /// connected).
    pub lines: Option<u32>,
    /// The rows a type card lists: its members, or an alias's aliased type (`None`: any other
    /// card).
    pub fields: Option<u32>,
    /// Whether it comes from another project.
    pub foreign: bool,
}

/// What a board thumbnail draws: its first page's cards and the arrows between them.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnail {
    /// The board's id.
    pub board: String,
    /// The first page, or `None` when the file can't be read.
    pub page: Option<ThumbnailPage>,
    /// How many pages the board has.
    pub pages: u32,
    /// The first page's cards (at most 200).
    pub cards: Vec<ThumbnailCard>,
    /// Calls between cards of one project, as card id pairs.
    pub edges: Vec<[String; 2]>,
    /// Drawn links, as card id pairs.
    pub links: Vec<[String; 2]>,
    /// The page has more cards than are listed.
    pub truncated: bool,
    /// Why the file can't be read, if it can't.
    pub error: Option<String>,
}

/// Board thumbnails, in the order asked for.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnails {
    /// One per board asked for.
    pub thumbnails: Vec<Thumbnail>,
}

/// One page of a board where a key is a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardRefs {
    /// The pages, by project in workspace order, then board id and page order (at most 50).
    pub refs: Vec<BoardRefHit>,
    /// More pages hold it than are listed.
    pub truncated: bool,
}

/// What `publish_board` did.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PublishOutcome {
    /// Published (or nothing to publish); the board as it is now (its id may have changed).
    Done {
        /// The board.
        board: Box<BoardState>,
    },
    /// A draft's first Publish: a board file already has that name; nothing was written.
    NameTaken {
        /// The existing file.
        path: String,
        /// A free name to offer.
        suggestion: String,
    },
    /// The published file changed since the changes copy started from it: keep mine
    /// (`resolve: "keep_mine"`) or take theirs (`discard_changes`).
    Conflict {
        /// The published file's revision now, or `None` when it was deleted.
        theirs_revision: Option<String>,
    },
}

/// How many boards a folder rename moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenamedFolder {
    /// The boards whose folder changed.
    pub boards: u32,
}

/// The arguments of a command about one board.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BoardIdArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
}

/// The arguments of `new_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewBoardArgs {
    /// The project.
    pub project: ProjectId,
    /// The title ("Untitled board" when absent or blank).
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    /// The folder (the project's top level when absent).
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    /// The first page's title ("Page 1" when absent or blank).
    #[serde(default)]
    #[ts(optional)]
    pub page_title: Option<String>,
}

/// The arguments of `move_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The window to move it to.
    pub to_window: WindowId,
}

/// The arguments of `save_board`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The edited board.
    pub board: BoardEdit,
    /// Write even though the working file changed on disk since it was loaded (keep mine).
    #[serde(default)]
    #[ts(optional)]
    pub force: Option<bool>,
}

/// The arguments of `save_board_view`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveBoardViewArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The view state.
    pub view: BoardView,
}

/// How `publish_board` settles a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Resolve {
    /// Publish the working version over the published file anyway.
    KeepMine,
}

/// The arguments of `publish_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PublishBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The published file's name, for a draft's first Publish (the slug of the title when
    /// absent).
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    /// How to settle a conflict with the published file.
    #[serde(default)]
    #[ts(optional)]
    pub resolve: Option<Resolve>,
}

/// The arguments of `set_board_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetBoardFolderArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The folder, or `None` for the project's top level.
    pub folder: Option<String>,
}

/// The arguments of `rename_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameFolderArgs {
    /// The project.
    pub project: ProjectId,
    /// The folder now.
    pub from: String,
    /// Its new path.
    pub to: String,
}

/// The arguments of `board_thumbnails`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailsArgs {
    /// The boards' project.
    pub project: ProjectId,
    /// The boards (at most 60).
    pub boards: Vec<String>,
}

/// A Flow label's address in a repair: its block key and, for another project, that project's
/// shared name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LabelSpec {
    /// The project's shared name; absent for the board's own.
    #[serde(default)]
    #[ts(optional, as = "Option<ProjectName>")]
    pub project: Option<String>,
    /// The block key.
    pub key: String,
}

/// What a repair is about (a card, group, label, link, resource card, frame, or proposed step) on
/// its page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TargetSpec {
    /// A card, by id.
    Card(String),
    /// A group, by id.
    Group(String),
    /// A Flow label.
    Label(LabelSpec),
    /// A drawn link or a connection, by id.
    Link(String),
    /// A resource card, by id.
    Resource(String),
    /// A frame, by id.
    Frame(String),
    /// A proposed step, by id.
    Proposal(String),
}

/// An anchor a repair points a resource card or connection at: a block or entrance key, or
/// lines of an evidence file, with the project's shared name for another project.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(untagged, deny_unknown_fields)]
pub enum AnchorSpec {
    /// A block key (`path#…`) or an entrance key (`entrance:[…]`).
    Key {
        /// The key.
        key: String,
        /// The project's shared name; absent for the board's own.
        #[serde(default)]
        #[ts(optional, as = "Option<ProjectName>")]
        project: Option<String>,
    },
    /// Lines of an evidence file.
    File {
        /// The file, relative to its project's folder.
        file: String,
        /// The first and last line, 1-based and inclusive.
        lines: [u32; 2],
        /// The project's shared name; absent for the board's own.
        #[serde(default)]
        #[ts(optional, as = "Option<ProjectName>")]
        project: Option<String>,
    },
}

/// One repair a health item offers (its `repairs`), with what it needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepairKindSpec {
    /// Point reference `from` at `to` everywhere on the page (after a rename or move).
    Reconnect {
        /// The reference's project; absent for the board's own.
        #[serde(default)]
        #[ts(optional, as = "Option<ProjectName>")]
        project: Option<String>,
        /// The missing key.
        from: String,
        /// The key it should point at.
        to: String,
    },
    /// Remove a card, its groups, and its links.
    RemoveCard {
        /// The card's id.
        card: String,
    },
    /// Remove a Flow label.
    RemoveLabel {
        /// The label's project; absent for the board's own.
        #[serde(default)]
        #[ts(optional, as = "Option<ProjectName>")]
        project: Option<String>,
        /// The block key.
        key: String,
    },
    /// Remove a group, keeping its statements.
    Ungroup {
        /// The group's id.
        group: String,
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
    /// Remove a drawn link or a connection.
    RemoveLink {
        /// The link's id.
        link: String,
    },
    /// Remove a resource card and the links to it.
    RemoveResource {
        /// The resource card's id.
        resource: String,
    },
    /// Remove a card's, group's, resource card's, or frame's description, a label, a drawn
    /// link's label, or a connection's description and label.
    RemoveDescription {
        /// Whose.
        target: TargetSpec,
    },
    /// Keep a description's text and sign it against the current source (a resource card or
    /// connection with its texts).
    Resign {
        /// Whose.
        target: TargetSpec,
    },
    /// Accept a held reference's current code (save a fresh guard).
    Accept {
        /// The reference's project; absent for the board's own.
        #[serde(default)]
        #[ts(optional, as = "Option<ProjectName>")]
        project: Option<String>,
        /// The reference.
        key: String,
    },
    /// Remove a proposed step.
    RemoveProposal {
        /// The proposed step's id.
        proposal: String,
    },
}

/// A repair on one page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub struct RepairSpec {
    /// The page.
    pub page: String,
    /// The repair.
    #[serde(flatten)]
    #[ts(flatten)]
    pub repair: RepairKindSpec,
}

/// A shared name a repair names, checked.
fn project_name(name: Option<String>) -> CommandResult<Option<ProjectName>> {
    name.map(|name| {
        ProjectName::parse(&name)
            .ok_or_else(|| CommandError::invalid_args(format!("`{name}` is not a project name")))
    })
    .transpose()
}

impl LabelSpec {
    fn into_key(self) -> CommandResult<ProjectKey> {
        Ok(ProjectKey::new(project_name(self.project)?.as_ref(), &self.key))
    }
}

impl TargetSpec {
    fn into_target(self) -> CommandResult<Target> {
        Ok(match self {
            Self::Card(id) => Target::Card(id),
            Self::Group(id) => Target::Group(id),
            Self::Label(label) => Target::Label(label.into_key()?),
            Self::Link(id) => Target::Link(id),
            Self::Resource(id) => Target::Resource(id),
            Self::Frame(id) => Target::Frame(id),
            Self::Proposal(id) => Target::Proposal(id),
        })
    }
}

impl AnchorSpec {
    /// The board's anchor (`invalid_args` for a malformed key, path, line range, or project
    /// name).
    fn into_anchor(self) -> CommandResult<Anchor> {
        match self {
            Self::Key { key, project } => {
                if !key.starts_with("entrance:") && !key.contains('#') {
                    return Err(CommandError::invalid_args(format!(
                        "`{key}` is not a block key (`path#…`) or an entrance key (`entrance:[…]`)"
                    )));
                }
                Ok(Anchor::Key {
                    key: Key::from_text(&key),
                    project: project_name(project)?,
                })
            }
            Self::File {
                file,
                lines: [first, last],
                project,
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
                    project: project_name(project)?,
                })
            }
        }
    }
}

impl RepairKindSpec {
    /// The reader's repair (`invalid_args` for a malformed project name).
    pub(super) fn into_repair(self) -> CommandResult<Repair> {
        Ok(match self {
            Self::Reconnect { project, from, to } => Repair::Reconnect {
                project: project_name(project)?,
                from,
                to,
            },
            Self::ReconnectAnchor { item, anchor, to } => Repair::ReconnectAnchor {
                item,
                index: anchor as usize,
                to: to.into_anchor()?,
            },
            Self::RemoveCard { card } => Repair::RemoveCard(card),
            Self::RemoveLabel { project, key } => Repair::RemoveLabel(LabelSpec { project, key }.into_key()?),
            Self::Ungroup { group } => Repair::Ungroup(group),
            Self::RemoveLink { link } => Repair::RemoveLink(link),
            Self::RemoveResource { resource } => Repair::RemoveResource(resource),
            Self::RemoveDescription { target } => Repair::RemoveDescription(target.into_target()?),
            Self::Resign { target } => Repair::Resign(target.into_target()?),
            Self::Accept { project, key } => Repair::Accept(LabelSpec { project, key }.into_key()?),
            Self::RemoveProposal { proposal } => Repair::RemoveProposal(proposal),
        })
    }
}

/// The arguments of `repair_board`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RepairBoardArgs {
    /// The board's project.
    pub project: ProjectId,
    /// The board's id.
    pub id: String,
    /// The repair.
    pub repair: RepairSpec,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_name_their_page_and_refuse_unknown_fields() {
        let spec: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "page-2", "kind": "remove_label", "project": "web", "key": "a.ts#function:f"
        }))
        .expect("parse");
        assert_eq!(spec.page, "page-2");
        assert_eq!(
            spec.repair.into_repair().expect("repair"),
            Repair::RemoveLabel(ProjectKey::new(
                ProjectName::parse("web").as_ref(),
                "a.ts#function:f"
            ))
        );
        let target: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "resign", "target": { "label": { "key": "k" } }
        }))
        .expect("parse");
        assert_eq!(
            target.repair.into_repair().expect("repair"),
            Repair::Resign(Target::Label(ProjectKey::new(None, "k")))
        );
        for bad in [
            serde_json::json!({ "page": "p", "kind": "remove_card", "card": "c", "extra": 1 }),
            serde_json::json!({ "kind": "remove_card", "card": "c" }),
            serde_json::json!({ "page": "p", "kind": "explode" }),
        ] {
            assert!(
                serde_json::from_value::<RepairSpec>(bad.clone()).is_err(),
                "{bad}"
            );
        }
        let bad_name: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "accept", "project": "../x", "key": "k"
        }))
        .expect("parse");
        assert_eq!(
            bad_name.repair.into_repair().expect_err("bad name").code,
            "invalid_args"
        );
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
            repair(serde_json::json!({ "file": "db/schema.sql", "lines": [2, 4], "project": "web" })),
            Ok(Repair::ReconnectAnchor {
                item: "orders".to_owned(),
                index: 1,
                to: Anchor::File {
                    file: RepoPath::new("db/schema.sql").expect("path"),
                    lines: [2, 4],
                    project: ProjectName::parse("web"),
                },
            })
        );
        assert_eq!(
            repair(serde_json::json!({ "key": "a.ts#variable:queue" })),
            Ok(Repair::ReconnectAnchor {
                item: "orders".to_owned(),
                index: 1,
                to: Anchor::Key {
                    key: Key::from_text("a.ts#variable:queue"),
                    project: None,
                },
            })
        );
        for bad in [
            serde_json::json!({ "key": "queue" }),
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
            "page": "p", "kind": "remove_resource", "resource": "orders"
        }))
        .expect("parse");
        assert_eq!(
            removed.repair.into_repair(),
            Ok(Repair::RemoveResource("orders".to_owned()))
        );
        let unproposed: RepairSpec = serde_json::from_value(serde_json::json!({
            "page": "p", "kind": "remove_proposal", "proposal": "email"
        }))
        .expect("parse");
        assert_eq!(
            unproposed.repair.into_repair(),
            Ok(Repair::RemoveProposal("email".to_owned()))
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
