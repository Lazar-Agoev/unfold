//! Board commands (architecture §8.7, §10.5, §11.2; decisions U-22, U-37).
//!
//! A board is open in one window at a time: `open_board` claims it for the calling window (or
//! answers `held` with the window that has it, so the UI asks the main process to focus that
//! window), `move_board` hands the claim to another window, and closing a window releases its
//! boards. Every other board command checks that the calling window holds the board.
//!
//! The workspace keeps the version of each open board last loaded or saved (a
//! [`BoardSession`]). The UI edits its own copy and autosaves it with `save_board`, which
//! validates it against the graph (`prepare_save`: no new missing references, broken groups, or
//! held references), signs new descriptions from verified source, and writes it atomically.
//! A save is refused with `board_conflict` when the file changed on disk since the session's
//! revision (an agent or a teammate wrote it), unless `force` is set; the holding window also
//! gets a `board_conflict` event as soon as the watcher sees such a change. Open boards' health
//! feeds the Problems list and is recomputed after every graph update.
//!
//! Board IDs are folder-relative file paths ([`store::parse_id`]).

mod session;
mod store;
mod types;

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_reader::board::files::{BoardFile, Change, Place, slug};
use unfold_reader::board::{
    Board, BoardError, BoardGraph, Health, apply_repair, health, outline, parse, prepare_save, serialize,
};

pub use session::{BoardSession, HealthRefresh};
pub(crate) use session::{check_disk, load_graph, refresh_health};
pub use store::{board_id, parse_id};
pub use types::{
    BoardDocument, BoardHealthView, BoardIdArgs, BoardList, BoardState, BoardSummary, BoardWriteOutcome,
    MoveBoardArgs, NewBoardArgs, OpenBoardResult, PublishBoardArgs, RenameBoardArgs, RepairBoardArgs,
    RepairSpec, SaveBoardArgs, TargetSpec,
};

use self::session::{health_label, publish_problems};
use self::store::BoardStore;
use super::{FolderExtras, NoArgs, count};
use crate::events::{WindowId, WorkspaceEvent};
use crate::host::HostState;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::shared::lock;
use crate::workspace::Workspace;

/// What a board command works with.
struct Opened {
    workspace: Arc<Workspace>,
    extras: Arc<FolderExtras>,
    file: BoardFile,
    id: String,
}

/// Resolves a board command's ID and checks that the calling window holds the board.
fn held(ctx: &Ctx<'_>, id: &str) -> CommandResult<Opened> {
    let workspace = ctx.workspace()?;
    let file = parse_id(id)?;
    workspace.claims().check(id, ctx.window)?;
    Ok(Opened {
        extras: ctx.extras(&workspace),
        workspace,
        file,
        id: id.to_owned(),
    })
}

/// The view of an open board.
fn state_of(id: &str, session: &BoardSession, graph: &BoardGraph) -> BoardState {
    BoardState {
        id: id.to_owned(),
        place: session.file.place,
        name: session.file.name.clone(),
        document: BoardDocument::new(session.board.clone()),
        revision: session.revision.0.to_string(),
        health: session.health.clone(),
        outline: outline(&session.board, graph),
        label: health_label(&session.health),
    }
}

/// Parses a document the UI sent with the strict `unfold-board` parser.
fn parse_document(value: &serde_json::Value) -> CommandResult<Board> {
    Ok(parse(&value.to_string())?)
}

/// Tells every window of the folder that board files changed, without waiting for the watcher
/// (which may be off, or scanning periodically).
fn announce(workspace: &Workspace, files: &[&BoardFile]) {
    let mut published = Vec::new();
    let mut drafts = Vec::new();
    for file in files {
        match file.place {
            Place::Published => published.push(file.relative()),
            Place::Draft => drafts.push(format!("{}.json", file.name)),
        }
    }
    workspace.emit(&WorkspaceEvent::BoardsChanged {
        folder: workspace.folder().to_owned(),
        published,
        drafts,
    });
}

/// Validates, signs, and writes `board` over the session's file, refusing outside changes
/// unless `force`.
fn save_session(
    workspace: &Workspace,
    id: &str,
    session: &mut BoardSession,
    board: Board,
    force: bool,
) -> CommandResult<BoardState> {
    let store = BoardStore::new(workspace);
    if !force {
        match store.change(&session.file, session.revision)? {
            Change::Unchanged => {}
            Change::Changed(_) | Change::Missing => {
                return Err(CommandError::new(
                    "board_conflict",
                    format!("`{id}` changed on disk since it was loaded; reload it or save with force"),
                ));
            }
        }
    }
    let graph = load_graph(workspace, &board)?;
    let (signed, checked) = prepare_save(board, Some((&session.board, &session.health)), &graph)?;
    let revision = store.save(&session.file, &serialize(&signed))?;
    session.board = signed;
    session.revision = revision;
    session.health = checked;
    session.reported = None;
    Ok(state_of(id, session, &graph))
}

/// `list_boards`: published boards and drafts, with titles, card counts, and a label only when
/// something is wrong.
pub fn list_boards(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<BoardList> {
    let workspace = ctx.workspace()?;
    let extras = ctx.extras(&workspace);
    let store = BoardStore::new(&workspace);
    let open: BTreeMap<String, BoardSession> = lock(&extras.boards).clone();
    let summarize = |file: BoardFile| -> CommandResult<BoardSummary> {
        let id = board_id(&file);
        let held_by = workspace.claims().holder(&id);
        let mut summary = BoardSummary {
            id: id.clone(),
            place: file.place,
            name: file.name.clone(),
            title: None,
            cards: 0,
            label: None,
            held_by,
            error: None,
        };
        let (board, board_health) = match open.get(&id) {
            Some(session) => (session.board.clone(), session.health.clone()),
            None => match store.load(&file) {
                Ok(loaded) => match load_graph(&workspace, &loaded.board) {
                    Ok(graph) => {
                        let checked = health(&loaded.board, &graph);
                        (loaded.board, checked)
                    }
                    Err(error) => {
                        summary.error = Some(error.message);
                        return Ok(summary);
                    }
                },
                Err(error) => {
                    summary.error = Some(error.message);
                    return Ok(summary);
                }
            },
        };
        summary.title = Some(board.title.clone());
        summary.cards = count(board.cards.len());
        summary.label = health_label(&board_health);
        Ok(summary)
    };
    let mut list = BoardList {
        published: Vec::new(),
        drafts: Vec::new(),
    };
    for file in store.list(Place::Published)? {
        ctx.check_cancelled()?;
        list.published.push(summarize(file)?);
    }
    for file in store.list(Place::Draft)? {
        ctx.check_cancelled()?;
        list.drafts.push(summarize(file)?);
    }
    Ok(list)
}

/// `open_board`: claims the board for the calling window and returns it with its health and
/// outline, or answers `held` with the window that has it.
pub fn open_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<OpenBoardResult> {
    let workspace = ctx.workspace()?;
    let file = parse_id(&args.id)?;
    match workspace.claims().claim(&args.id, ctx.window) {
        Ok(()) => {}
        Err(crate::error::WorkspaceError::BoardHeld { window, .. }) => {
            return Ok(OpenBoardResult::Held { held_by: window });
        }
        Err(other) => return Err(other.into()),
    }
    let extras = ctx.extras(&workspace);
    let opened = load_into_session(&workspace, &extras, &args.id, &file, false);
    if opened.is_err() {
        workspace.claims().release(&args.id, ctx.window);
    }
    let board = opened?;
    publish_problems(&workspace, &extras);
    Ok(OpenBoardResult::Opened {
        board: Box::new(board),
    })
}

/// Loads a board into its session (reusing an existing session unless `reload`).
fn load_into_session(
    workspace: &Workspace,
    extras: &FolderExtras,
    id: &str,
    file: &BoardFile,
    reload: bool,
) -> CommandResult<BoardState> {
    let mut sessions = lock(&extras.boards);
    if !reload && let Some(session) = sessions.get(id) {
        let graph = load_graph(workspace, &session.board)?;
        return Ok(state_of(id, session, &graph));
    }
    let loaded = BoardStore::new(workspace).load(file)?;
    let graph = load_graph(workspace, &loaded.board)?;
    let session = BoardSession {
        file: file.clone(),
        health: health(&loaded.board, &graph),
        board: loaded.board,
        revision: loaded.revision,
        reported: None,
    };
    let state = state_of(id, &session, &graph);
    sessions.insert(id.to_owned(), session);
    Ok(state)
}

/// `reload_board`: reads the board from disk again (after an outside edit) and returns it.
pub fn reload_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let opened = held(ctx, &args.id)?;
    let state = load_into_session(&opened.workspace, &opened.extras, &opened.id, &opened.file, true)?;
    publish_problems(&opened.workspace, &opened.extras);
    Ok(state)
}

/// `new_board`: creates an empty draft, opens it in the calling window, and returns it.
pub fn new_board(ctx: &Ctx<'_>, args: NewBoardArgs) -> CommandResult<BoardState> {
    let workspace = ctx.workspace()?;
    if !workspace.layout().is_compiled() {
        return Err(crate::error::WorkspaceError::NotCompiled.into());
    }
    let title = args
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Untitled board".to_owned());
    let store = BoardStore::new(&workspace);
    let file = store.free_draft(&format!("board-{}", crate::commit::now_ms()))?;
    let board = Board::new(&title);
    let revision = store.save(&file, &serialize(&board))?;
    let id = board_id(&file);
    workspace.claims().claim(&id, ctx.window)?;
    let graph = BoardGraph::new();
    let session = BoardSession {
        file: file.clone(),
        health: Health::default(),
        board,
        revision,
        reported: None,
    };
    let state = state_of(&id, &session, &graph);
    lock(&ctx.extras(&workspace).boards).insert(id, session);
    announce(&workspace, &[&file]);
    Ok(state)
}

/// `close_board`: releases the calling window's claim (save first; unsaved edits are the UI's).
pub fn close_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let opened = held(ctx, &args.id)?;
    opened.workspace.claims().release(&opened.id, ctx.window);
    lock(&opened.extras.boards).remove(&opened.id);
    publish_problems(&opened.workspace, &opened.extras);
    Ok(())
}

/// `move_board`: moves the claim to another window of the folder (Move to New Window).
pub fn move_board(ctx: &Ctx<'_>, args: MoveBoardArgs) -> CommandResult<()> {
    let opened = held(ctx, &args.id)?;
    opened
        .workspace
        .transfer_board(&opened.id, ctx.window, args.to_window)?;
    Ok(())
}

/// `save_board`: validates, signs, and atomically writes the edited board (drafts autosave
/// here), and returns it as saved.
pub fn save_board(ctx: &Ctx<'_>, args: SaveBoardArgs) -> CommandResult<BoardState> {
    let opened = held(ctx, &args.id)?;
    let board = parse_document(&args.board)?;
    let state = {
        let mut sessions = lock(&opened.extras.boards);
        let session = sessions.get_mut(&opened.id).ok_or_else(|| {
            CommandError::from(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()))
        })?;
        save_session(
            &opened.workspace,
            &opened.id,
            session,
            board,
            args.force.unwrap_or(false),
        )?
    };
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &[&opened.file]);
    Ok(state)
}

/// `publish_board`: writes the draft to `.unfold/boards/<name>.json` (never overwriting; a
/// taken name answers `name_taken` with a free one) and removes the draft.
pub fn publish_board(ctx: &Ctx<'_>, args: PublishBoardArgs) -> CommandResult<BoardWriteOutcome> {
    let opened = held(ctx, &args.id)?;
    if opened.file.place != Place::Draft {
        return Err(CommandError::invalid_args(format!(
            "`{}` is already published",
            opened.id
        )));
    }
    let store = BoardStore::new(&opened.workspace);
    let (outcome, published) = {
        let mut sessions = lock(&opened.extras.boards);
        let Some(session) = sessions.get(&opened.id) else {
            return Err(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()).into());
        };
        let graph = load_graph(&opened.workspace, &session.board)?;
        let (signed, checked) = prepare_save(
            session.board.clone(),
            Some((&session.board, &session.health)),
            &graph,
        )?;
        let name = args.name.unwrap_or_else(|| slug(&signed.title));
        let (file, revision) = match store.create_published(&name, &serialize(&signed)) {
            Ok(created) => created,
            Err(BoardError::NameTaken { path, suggestion }) => {
                return Ok(BoardWriteOutcome::NameTaken { path, suggestion });
            }
            Err(other) => return Err(other.into()),
        };
        store.remove(&opened.file)?;
        let id = board_id(&file);
        opened.workspace.claims().rename(&opened.id, &id);
        sessions.remove(&opened.id);
        let session = BoardSession {
            file: file.clone(),
            board: signed,
            revision,
            health: checked,
            reported: None,
        };
        let state = state_of(&id, &session, &graph);
        sessions.insert(id, session);
        (
            BoardWriteOutcome::Done {
                board: Box::new(state),
            },
            file,
        )
    };
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &[&opened.file, &published]);
    Ok(outcome)
}

/// `unpublish_board`: moves a published board back to drafts.
pub fn unpublish_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let opened = held(ctx, &args.id)?;
    if opened.file.place != Place::Published {
        return Err(CommandError::invalid_args(format!("`{}` is a draft", opened.id)));
    }
    let store = BoardStore::new(&opened.workspace);
    let (state, draft) = {
        let mut sessions = lock(&opened.extras.boards);
        let Some(mut session) = sessions.remove(&opened.id) else {
            return Err(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()).into());
        };
        let draft = store.free_draft(&opened.file.name)?;
        let written = store
            .save(&draft, &serialize(&session.board))
            .and_then(|revision| store.remove(&opened.file).map(|()| revision));
        let revision = match written {
            Ok(revision) => revision,
            Err(error) => {
                sessions.insert(opened.id.clone(), session);
                return Err(error);
            }
        };
        let id = board_id(&draft);
        opened.workspace.claims().rename(&opened.id, &id);
        session.file = draft.clone();
        session.revision = revision;
        session.reported = None;
        let graph = load_graph(&opened.workspace, &session.board)?;
        let state = state_of(&id, &session, &graph);
        sessions.insert(id, session);
        (state, draft)
    };
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &[&opened.file, &draft]);
    Ok(state)
}

/// `rename_board`: changes the title (saved like an edit) and, for a published board, the file
/// name (never overwriting another board).
pub fn rename_board(ctx: &Ctx<'_>, args: RenameBoardArgs) -> CommandResult<BoardWriteOutcome> {
    let opened = held(ctx, &args.id)?;
    if args.name.is_some() && opened.file.place != Place::Published {
        return Err(CommandError::invalid_args(
            "only a published board's file can be renamed; a draft's name is internal",
        ));
    }
    let mut touched = vec![opened.file.clone()];
    let outcome = {
        let mut sessions = lock(&opened.extras.boards);
        let Some(session) = sessions.get_mut(&opened.id) else {
            return Err(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()).into());
        };
        let mut state = None;
        if let Some(title) = &args.title {
            if title.trim().is_empty() {
                return Err(CommandError::invalid_args("the title is empty"));
            }
            let mut board = session.board.clone();
            board.title.clone_from(title);
            state = Some(save_session(
                &opened.workspace,
                &opened.id,
                session,
                board,
                false,
            )?);
        }
        if let Some(name) = &args.name {
            let store = BoardStore::new(&opened.workspace);
            let file = match store.rename_published(&opened.file, name) {
                Ok(file) => file,
                Err(BoardError::NameTaken { path, suggestion }) => {
                    return Ok(BoardWriteOutcome::NameTaken { path, suggestion });
                }
                Err(other) => return Err(other.into()),
            };
            let id = board_id(&file);
            if let Some(mut moved) = sessions.remove(&opened.id) {
                opened.workspace.claims().rename(&opened.id, &id);
                moved.file = file.clone();
                let graph = load_graph(&opened.workspace, &moved.board)?;
                state = Some(state_of(&id, &moved, &graph));
                sessions.insert(id, moved);
            }
            touched.push(file);
        }
        match state {
            Some(board) => BoardWriteOutcome::Done {
                board: Box::new(board),
            },
            None => {
                let session = sessions
                    .get(&opened.id)
                    .ok_or_else(|| CommandError::internal("the board's session disappeared"))?;
                let graph = load_graph(&opened.workspace, &session.board)?;
                BoardWriteOutcome::Done {
                    board: Box::new(state_of(&opened.id, session, &graph)),
                }
            }
        }
    };
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &touched.iter().collect::<Vec<_>>());
    Ok(outcome)
}

/// `delete_board`: deletes the board's file (the UI asks first) and closes it.
pub fn delete_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let opened = held(ctx, &args.id)?;
    BoardStore::new(&opened.workspace).remove(&opened.file)?;
    opened.workspace.claims().release(&opened.id, ctx.window);
    lock(&opened.extras.boards).remove(&opened.id);
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &[&opened.file]);
    Ok(())
}

/// `board_health`: the board's health and outline against the current graph.
pub fn board_health(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardHealthView> {
    let opened = held(ctx, &args.id)?;
    let view = {
        let mut sessions = lock(&opened.extras.boards);
        let session = sessions.get_mut(&opened.id).ok_or_else(|| {
            CommandError::from(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()))
        })?;
        let graph = load_graph(&opened.workspace, &session.board)?;
        session.health = health(&session.board, &graph);
        BoardHealthView {
            outline: outline(&session.board, &graph),
            label: health_label(&session.health),
            health: session.health.clone(),
        }
    };
    publish_problems(&opened.workspace, &opened.extras);
    Ok(view)
}

/// `repair_board`: applies one repair a health item offers and saves the board.
pub fn repair_board(ctx: &Ctx<'_>, args: RepairBoardArgs) -> CommandResult<BoardState> {
    let opened = held(ctx, &args.id)?;
    let state = {
        let mut sessions = lock(&opened.extras.boards);
        let session = sessions.get_mut(&opened.id).ok_or_else(|| {
            CommandError::from(crate::error::WorkspaceError::BoardNotHeld(opened.id.clone()))
        })?;
        let mut board = session.board.clone();
        let graph = load_graph(&opened.workspace, &board)?;
        apply_repair(&mut board, &graph, &args.repair.into())?;
        save_session(&opened.workspace, &opened.id, session, board, false)?
    };
    publish_problems(&opened.workspace, &opened.extras);
    announce(&opened.workspace, &[&opened.file]);
    Ok(state)
}

/// The authored Flow labels of a board the calling window holds, for its cards.
pub(crate) fn labels_for(
    ctx: &Ctx<'_>,
    workspace: &Workspace,
    id: &str,
) -> CommandResult<BTreeMap<String, String>> {
    workspace.claims().check(id, ctx.window)?;
    let extras = ctx.extras(workspace);
    let sessions = lock(&extras.boards);
    let session = sessions
        .get(id)
        .ok_or_else(|| CommandError::from(crate::error::WorkspaceError::BoardNotHeld(id.to_owned())))?;
    Ok(session
        .board
        .labels
        .keys()
        .filter_map(|key| {
            session
                .board
                .label(key)
                .map(|text| (key.clone(), text.to_owned()))
        })
        .collect())
}

/// The last saved version of a board the calling window holds.
pub(crate) fn held_board(ctx: &Ctx<'_>, id: &str) -> CommandResult<(Arc<Workspace>, Board)> {
    let opened = held(ctx, id)?;
    let board = lock(&opened.extras.boards)
        .get(id)
        .map(|session| session.board.clone())
        .ok_or_else(|| CommandError::from(crate::error::WorkspaceError::BoardNotHeld(id.to_owned())))?;
    Ok((opened.workspace, board))
}

/// Forgets the sessions of the boards `window` holds (the window is closing or leaving the
/// folder; their claims go with it).
pub(crate) fn release_window(host: &HostState, workspace: &Workspace, window: WindowId) {
    let boards = workspace.claims().boards_of(window);
    if boards.is_empty() {
        return;
    }
    let extras = host.extras(workspace.folder());
    {
        let mut sessions = lock(&extras.boards);
        for board in &boards {
            sessions.remove(board);
        }
    }
    publish_problems(workspace, &extras);
}
