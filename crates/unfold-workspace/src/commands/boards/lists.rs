//! What lists and reads boards without opening them: a project's boards and empty folders,
//! where a key is on a project's boards, and one board's file. All of them read from disk and
//! serve resting projects without waking them; only a live project's boards carry a health
//! label (health needs a current graph).

use unfold_reader::board::files::{BoardFile, Loaded};

use super::files::{Files, parse_id};
use super::graphs::Source;
use super::session::health_of;
use super::types::{
    BoardDocument, BoardIdArgs, BoardList, BoardRefHit, BoardRefs, BoardSummary, ReadBoardResult,
};
use crate::commands::{ProjectArgs, ProjectExtras, ProjectKeyArgs, count, key_arg};
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::projects::ProjectId;
use crate::state::{LocalState, read_json};

/// The most boards `list_boards` lists.
const MAX_BOARDS: usize = 2_000;
/// The most pages `board_refs` lists.
const MAX_REFS: usize = 50;

/// Board `file` of project `project` as lists show it, from its file (or why it can't be read),
/// last edited at `updated_at_ms`, with health label `label`.
pub(super) fn summary(
    project: &ProjectId,
    file: &BoardFile,
    updated_at_ms: i64,
    loaded: CommandResult<Loaded>,
    label: Option<String>,
) -> BoardSummary {
    let mut summary = BoardSummary {
        project: project.clone(),
        id: file.relative(),
        name: file.name().to_owned(),
        title: None,
        folder: None,
        pages: 0,
        cards: 0,
        updated_at_ms,
        label,
        error: None,
    };
    match loaded {
        Ok(loaded) => {
            let board = loaded.board;
            summary.pages = count(board.pages.len());
            summary.cards = count(board.pages.iter().map(|page| page.cards.len()).sum());
            summary.title = Some(board.title);
            summary.folder = board.folder;
        }
        Err(error) => summary.error = Some(error.message),
    }
    summary
}

/// A board's health label as a live project's cache and graph give it; `None` while the
/// project rests or has no graph, when the board is healthy, or when it can't be read.
pub(super) fn label_of(
    health: Option<&(std::sync::Arc<ProjectExtras>, Source)>,
    id: &str,
    loaded: &CommandResult<Loaded>,
) -> Option<String> {
    let (Some((extras, source)), Ok(loaded)) = (health, loaded) else {
        return None;
    };
    health_of(&extras.boards, source, id, loaded)
        .ok()
        .flatten()
        .and_then(|health| health.label())
}

/// `list_boards`: a project's boards, most recently edited first (then by id), and its empty
/// folders, for the sidebar and Home's Boards. A board's last edit is its file's modification
/// time.
pub fn list_boards(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<BoardList> {
    let project = ctx.project(&args.project)?;
    let files = Files::reading(project.root());
    let (empty_folders, health) = match project.live() {
        Some(workspace) => (
            workspace.local_state().empty_folders,
            Some((project.extras(), Source::live(&workspace)?)),
        ),
        None => (
            read_json::<LocalState>(&project.layout().state_file()).empty_folders,
            None,
        ),
    };
    let mut edited: Vec<(i64, BoardFile)> = files
        .boards()?
        .into_iter()
        .map(|file| (files.modified_ms(&file), file))
        .collect();
    edited.sort_by(|(a_ms, a), (b_ms, b)| b_ms.cmp(a_ms).then_with(|| a.name().cmp(b.name())));
    edited.truncate(MAX_BOARDS);
    let mut boards = Vec::with_capacity(edited.len());
    for (updated_at_ms, file) in &edited {
        ctx.check_cancelled()?;
        let loaded = files.load(file);
        let label = label_of(health.as_ref(), &file.relative(), &loaded);
        boards.push(summary(project.id(), file, *updated_at_ms, loaded, label));
    }
    Ok(BoardList {
        project: args.project,
        boards,
        empty_folders,
    })
}

/// `read_board`: board `id` as its file holds it, with the file's revision. Neither opens the
/// board nor wakes its project.
pub fn read_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<ReadBoardResult> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    let loaded = Files::reading(project.root()).load(&file)?;
    Ok(ReadBoardResult {
        project: args.project,
        id: file.relative(),
        name: file.name().to_owned(),
        document: BoardDocument::new(loaded.board),
        revision: loaded.revision.0.to_string(),
    })
}

/// `board_refs`: the pages of project `project`'s boards where key `key` is a card ("Already
/// on …").
pub fn board_refs(ctx: &Ctx<'_>, args: ProjectKeyArgs) -> CommandResult<BoardRefs> {
    let project = ctx.project(&args.project)?;
    let key = key_arg(&args.key)?;
    let files = Files::reading(project.root());
    let mut refs = Vec::new();
    for file in files.boards()? {
        ctx.check_cancelled()?;
        let Ok(loaded) = files.load(&file) else {
            continue;
        };
        let board = &loaded.board;
        for page in &board.pages {
            if !page.cards.iter().any(|card| card.reference.key().as_str() == key) {
                continue;
            }
            if refs.len() == MAX_REFS {
                return Ok(BoardRefs {
                    refs,
                    truncated: true,
                });
            }
            refs.push(BoardRefHit {
                project: project.id().clone(),
                board: file.relative(),
                title: board.title.clone(),
                page: page.id.clone(),
                page_title: page.title.clone(),
            });
        }
    }
    Ok(BoardRefs {
        refs,
        truncated: false,
    })
}
