//! Mermaid export (architecture §10.5, §13): a board or an entrance as a Mermaid flowchart,
//! written to a file the person chose in the main process's save dialog, or returned as text
//! for the clipboard.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::board::files::write_atomic;
use unfold_reader::board::{Board, BoardGraph, Card, CardRef, View};
use unfold_reader::mermaid;

use super::boards::{held_board, load_graph};
use super::count;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};

/// The arguments of `export_mermaid_text`: exactly one of `board` and `entrance`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MermaidTextArgs {
    /// A board the calling window holds (its last saved version).
    #[serde(default)]
    #[ts(optional)]
    pub board: Option<String>,
    /// An entrance key: the entrance and a Flow card for each handler.
    #[serde(default)]
    #[ts(optional)]
    pub entrance: Option<String>,
}

/// The arguments of `export_mermaid`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExportMermaidArgs {
    /// A board the window holds.
    #[serde(default)]
    #[ts(optional)]
    pub board: Option<String>,
    /// An entrance key.
    #[serde(default)]
    #[ts(optional)]
    pub entrance: Option<String>,
    /// The absolute path the person chose in the save dialog.
    pub path: String,
}

/// Mermaid text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MermaidText {
    /// The flowchart.
    pub text: String,
}

/// A written export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    /// Where it was written.
    pub path: String,
    /// Its size in bytes.
    pub bytes: u32,
}

/// `export_mermaid_text`: the Mermaid text of a board or an entrance.
pub fn export_mermaid_text(ctx: &Ctx<'_>, args: MermaidTextArgs) -> CommandResult<MermaidText> {
    Ok(MermaidText {
        text: mermaid_of(ctx, args.board.as_deref(), args.entrance.as_deref())?,
    })
}

/// `export_mermaid`: writes the Mermaid text to an absolute path. Accepted only from the main
/// process (`trusted`), which got the path from a save dialog: a window can never name a path
/// outside its folder.
pub fn export_mermaid(ctx: &Ctx<'_>, args: ExportMermaidArgs) -> CommandResult<ExportedFile> {
    ctx.require_trusted("export_mermaid")?;
    let path = Path::new(&args.path);
    if !path.is_absolute() {
        return Err(CommandError::invalid_args("the export path must be absolute"));
    }
    let text = mermaid_of(ctx, args.board.as_deref(), args.entrance.as_deref())?;
    write_atomic(path, text.as_bytes())?;
    Ok(ExportedFile {
        path: args.path,
        bytes: count(text.len()),
    })
}

fn mermaid_of(ctx: &Ctx<'_>, board: Option<&str>, entrance: Option<&str>) -> CommandResult<String> {
    match (board, entrance) {
        (Some(id), None) => {
            let (workspace, board) = held_board(ctx, id)?;
            let graph = load_graph(&workspace, &board)?;
            Ok(mermaid::board(&board, &graph))
        }
        (None, Some(key)) => {
            let workspace = ctx.workspace()?;
            let reader = workspace.readers()?.get()?;
            // Load the entrance, then its handlers' files.
            let mut seed = Board::new("export");
            seed.cards.push(Card {
                id: "entrance".to_owned(),
                reference: CardRef::Entrance(unfold_core::Key::from_text(key)),
                at: [0.0, 0.0],
                view: Some(View::Code),
                folded: false,
                description: None,
            });
            let graph = BoardGraph::load(&reader, workspace.root(), &seed)?;
            let board = mermaid::entrance_board(&graph, key)
                .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))?;
            let graph = BoardGraph::load(&reader, workspace.root(), &board)?;
            Ok(mermaid::board(&board, &graph))
        }
        _ => Err(CommandError::invalid_args(
            "name exactly one of `board` and `entrance`",
        )),
    }
}
