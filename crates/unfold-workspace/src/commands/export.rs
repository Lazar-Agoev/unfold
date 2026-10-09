//! Mermaid export (contract §4.7, architecture §10.5, §13): one board page or one entrance as a
//! Mermaid flowchart, written to a file the person chose in the main process's save dialog, or
//! returned as text for the clipboard. A page's drawn links are dashed arrows with their labels,
//! and a card from a project that is not connected is a "From <name>, not connected"
//! placeholder.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::board::BoardGraph;
use unfold_reader::board::files::write_atomic;
use unfold_reader::mermaid;

use super::{boards, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// A page of a board the calling window holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BoardPage {
    /// The board's id.
    pub id: String,
    /// The page's id.
    pub page: String,
}

/// The arguments of `export_mermaid_text`: exactly one of `board` and `entrance`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MermaidTextArgs {
    /// The project.
    pub project: ProjectId,
    /// A page of a board the calling window holds (its last saved version).
    #[serde(default)]
    #[ts(optional)]
    pub board: Option<BoardPage>,
    /// An entrance key: the entrance and a Flow card for each handler.
    #[serde(default)]
    #[ts(optional)]
    pub entrance: Option<String>,
}

/// The arguments of `export_mermaid`: exactly one of `board` and `entrance`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExportMermaidArgs {
    /// The project.
    pub project: ProjectId,
    /// A page of a board the window the main process names holds.
    #[serde(default)]
    #[ts(optional)]
    pub board: Option<BoardPage>,
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

/// `export_mermaid_text`: the Mermaid text of a board page or an entrance.
pub fn export_mermaid_text(ctx: &Ctx<'_>, args: MermaidTextArgs) -> CommandResult<MermaidText> {
    Ok(MermaidText {
        text: mermaid_of(ctx, &args.project, args.board.as_ref(), args.entrance.as_deref())?,
    })
}

/// `export_mermaid`: writes the Mermaid text to an absolute path. Accepted only from the main
/// process (`trusted`), which got the path from a save dialog: a window can never name a path
/// outside its projects.
pub fn export_mermaid(ctx: &Ctx<'_>, args: ExportMermaidArgs) -> CommandResult<ExportedFile> {
    ctx.require_trusted("export_mermaid")?;
    let path = Path::new(&args.path);
    if !path.is_absolute() {
        return Err(CommandError::invalid_args("the export path must be absolute"));
    }
    let text = mermaid_of(ctx, &args.project, args.board.as_ref(), args.entrance.as_deref())?;
    write_atomic(path, text.as_bytes())?;
    Ok(ExportedFile {
        path: args.path,
        bytes: count(text.len()),
    })
}

fn mermaid_of(
    ctx: &Ctx<'_>,
    project: &ProjectId,
    board: Option<&BoardPage>,
    entrance: Option<&str>,
) -> CommandResult<String> {
    match (board, entrance) {
        (Some(board), None) => boards::page_mermaid(ctx, project, &board.id, &board.page),
        (None, Some(key)) => {
            let workspace = ctx.live(project)?;
            let reader = workspace.readers()?.get()?;
            // The entrance first, then the files of the handlers it draws.
            let graph = BoardGraph::load(&reader, workspace.root(), &[key])?;
            let board = mermaid::entrance_board(&graph, key)
                .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))?;
            let graph = BoardGraph::load(&reader, workspace.root(), &board.references(None))?;
            mermaid::entrance(&graph, key)
                .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))
        }
        _ => Err(CommandError::invalid_args(
            "name exactly one of `board` and `entrance`",
        )),
    }
}
