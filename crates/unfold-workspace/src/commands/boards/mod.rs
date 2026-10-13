//! Board commands (docs/ipc.md › Boards; decisions board references, board folder metadata).
//!
//! A board belongs to a project and is addressed as `{project, id}`, its id being its file's
//! path ([`files`]): every board is a file in `.unfold/boards/`, saved as it is edited and
//! shared through git. `open_board` and `new_board` open it (the project's
//! [`crate::open_boards::OpenBoards`]), and `close_board`, `delete_board`, and a closing window
//! close it. Every other command about one board needs it open (`board_not_open`), except
//! `read_board`, which reads a board's file without opening it, and the folder commands: a
//! board's folder is metadata Rust owns (board folder metadata), never part of what a window
//! saves, so they work on any board.
//!
//! The project keeps a **session** per open board ([`session`]): the version last loaded or
//! saved. A window autosaves its edits (`save_board`, a [`BoardEdit`]): Rust checks them against
//! the project's graph ([`graphs`]: no new missing reference, broken group, held reference, or
//! broken link), signs new descriptions, labels, and link labels, and writes the board over its
//! file, whatever the file holds now. When the file changes outside the app (another program, a
//! `git pull`), the session is reloaded from it and the window that opened the board gets
//! `board_reloaded` ([`health::check_disk`]); while the window's save is failing, it ignores the
//! reload, and its Retry writes the person's version over the file.
//!
//! Every board's health is cached per board ([`session::BoardExtras`]) and feeds the Problems
//! list and Home's Health line; it is refreshed after the project's graph changes.

mod files;
mod folders;
mod graphs;
mod health;
mod lists;
mod session;
mod types;

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_reader::board::files::{BoardFile, Loaded};
use unfold_reader::board::{
    Anchor, Board, BoardGraph, Health, Repair, apply_repair, outline, parse_edit, prepare_save,
};
use unfold_reader::mermaid;

pub use self::folders::{create_folder, delete_folder, rename_folder, set_board_folder};
pub(crate) use self::health::{check_disk, refresh};
pub use self::lists::{board_refs, list_boards, read_board};
pub use self::session::BoardExtras;
pub use self::types::{
    AnchorSpec, BoardDocument, BoardEdit, BoardHealthView, BoardIdArgs, BoardList, BoardRefHit, BoardRefs,
    BoardState, BoardSummary, NewBoardArgs, ReadBoardResult, RenameFolderArgs, RenamedFolder,
    RepairBoardArgs, RepairKindSpec, RepairSpec, SaveBoardArgs, SetBoardFolderArgs, TargetSpec,
};

use self::files::{Files, parse_id};
use self::folders::{folder_arg, folder_left, folder_used};
use self::graphs::Source;
use self::session::{Session, publish_problems};
use super::ProjectExtras;
use crate::events::WorkspaceEvent;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{ProjectHandle, ProjectId};
use crate::workspace::Workspace;

/// An open board, in its (woken) project.
struct Opened<'c> {
    project: ProjectHandle<'c>,
    workspace: Arc<Workspace>,
    extras: Arc<ProjectExtras>,
    file: BoardFile,
    id: String,
}

/// Board `id` of `project`, which must be open; wakes the project.
fn opened<'c>(ctx: &'c Ctx<'_>, project: &ProjectId, id: &str) -> CommandResult<Opened<'c>> {
    let handle = ctx.project(project)?;
    let file = parse_id(id)?;
    handle.open_boards().check(id)?;
    let workspace = handle.wake()?;
    Ok(Opened {
        extras: handle.extras(),
        workspace,
        project: handle,
        file,
        id: id.to_owned(),
    })
}

/// A session the board must have (an open board without one is a broken invariant).
fn session_mut<'s>(sessions: &'s mut BTreeMap<String, Session>, id: &str) -> CommandResult<&'s mut Session> {
    sessions
        .get_mut(id)
        .ok_or_else(|| CommandError::internal(format!("board `{id}` has no session")))
}

/// Reads board `file` into a new session, with its health (cached) and the graph it was checked
/// against.
fn load_session(
    files: &Files<'_>,
    extras: &BoardExtras,
    source: &Source,
    id: &str,
    file: &BoardFile,
) -> CommandResult<(Session, BoardGraph)> {
    let loaded = files.load(file)?;
    let graph = source.board(&loaded.board)?;
    let health = unfold_reader::board::board_health(&loaded.board, &graph);
    extras.cache(source, id, &loaded.board, loaded.revision, &health);
    Ok((Session::new(loaded, health), graph))
}

/// Board `id` of `project` (its file `file`) as the window shows it.
fn board_state(
    project: &ProjectId,
    id: &str,
    file: &BoardFile,
    session: &Session,
    graph: &BoardGraph,
) -> BoardState {
    BoardState {
        project: project.clone(),
        id: id.to_owned(),
        name: file.name().to_owned(),
        document: BoardDocument::new(session.board.clone()),
        revision: session.revision.0.to_string(),
        outlines: session
            .board
            .pages
            .iter()
            .map(|page| outline(page, graph))
            .collect(),
        label: session.health.label(),
        health: session.health.clone(),
    }
}

impl Opened<'_> {
    fn files(&self) -> Files<'_> {
        Files::live(&self.workspace)
    }

    fn boards(&self) -> &BoardExtras {
        &self.extras.boards
    }

    fn source(&self) -> CommandResult<Source> {
        Source::live(&self.workspace)
    }

    /// The board's session, read from disk when it has none (with the graph it was read with).
    fn session<'s>(
        &self,
        sessions: &'s mut BTreeMap<String, Session>,
        source: &Source,
    ) -> CommandResult<(&'s mut Session, Option<BoardGraph>)> {
        let mut graph = None;
        if !sessions.contains_key(&self.id) {
            let (session, read) = load_session(&self.files(), self.boards(), source, &self.id, &self.file)?;
            sessions.insert(self.id.clone(), session);
            graph = Some(read);
        }
        Ok((session_mut(sessions, &self.id)?, graph))
    }

    /// The board as the window shows it.
    fn state(&self, session: &Session, graph: &BoardGraph) -> BoardState {
        board_state(self.project.id(), &self.id, &self.file, session, graph)
    }

    /// Validates, signs, and writes `board` over the board's file, whatever the file holds now,
    /// as the session's new version.
    fn save(&self, session: &mut Session, source: &Source, board: Board) -> CommandResult<BoardState> {
        let graph = source.board(&board)?;
        let (signed, health) = prepare_save(board, Some((&session.board, &session.health)), &graph)?;
        let revision = self.files().save(&self.file, &signed)?;
        self.boards().cache(source, &self.id, &signed, revision, &health);
        *session = Session::new(
            Loaded {
                board: signed,
                revision,
            },
            health,
        );
        Ok(self.state(session, &graph))
    }

    /// Opens the board in its session (read from disk when it has none) and returns it.
    fn open(&self) -> CommandResult<BoardState> {
        let source = self.source()?;
        let mut sessions = self.boards().sessions();
        let (session, graph) = self.session(&mut sessions, &source)?;
        let graph = match graph {
            Some(graph) => graph,
            None => source.board(&session.board)?,
        };
        Ok(self.state(session, &graph))
    }

    /// Tells every window that boards of the project changed (the watcher doesn't report the
    /// app's own writes).
    fn announce(&self, mut boards: Vec<String>, folders: bool) {
        announce(&self.workspace, &mut boards, folders);
    }
}

/// Sends `boards_changed` for `boards` of live project `workspace`.
fn announce(workspace: &Workspace, boards: &mut Vec<String>, folders: bool) {
    boards.sort();
    boards.dedup();
    workspace.emit(&WorkspaceEvent::BoardsChanged {
        project: workspace.project().clone(),
        boards: std::mem::take(boards),
        folders,
    });
}

/// `text`, or `fallback` when absent or blank.
fn nonblank(text: Option<String>, fallback: &str) -> String {
    text.filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

/// `open_board`: opens the board in the calling window (keeping its session when it is open
/// already) and returns it.
pub fn open_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    // A board open already stays open whatever happens here.
    let newly_opened = project.open_boards().open(&args.id, ctx.window);
    let release = |project: &ProjectHandle<'_>| {
        if newly_opened {
            project.open_boards().close(&args.id);
        }
    };
    let workspace = match project.wake() {
        Ok(workspace) => workspace,
        Err(error) => {
            release(&project);
            return Err(error.into());
        }
    };
    let opened = Opened {
        extras: project.extras(),
        workspace,
        project,
        file,
        id: args.id.clone(),
    };
    opened.open().inspect_err(|_| release(&opened.project))
}

/// `new_board`: creates an empty board with one page as a new file in `.unfold/boards/` (named
/// from its title, never overwriting another board), opens it in the calling window, and
/// returns it.
pub fn new_board(ctx: &Ctx<'_>, args: NewBoardArgs) -> CommandResult<BoardState> {
    let project = ctx.project(&args.project)?;
    let folder = args.folder.map(folder_arg).transpose()?;
    let workspace = project.wake()?;
    let mut board = Board::new(&nonblank(args.title, "Untitled board"));
    board.folder.clone_from(&folder);
    if let Some(page) = board.pages.first_mut() {
        page.title = nonblank(args.page_title, "Page 1");
    }
    let files = Files::live(&workspace);
    let source = Source::live(&workspace)?;
    let (file, revision) = files.create(&board)?;
    let id = file.relative();
    project.open_boards().open(&id, ctx.window);
    // An empty board is healthy.
    let extras = project.extras();
    extras
        .boards
        .cache(&source, &id, &board, revision, &Health::default());
    let session = Session::new(Loaded { board, revision }, Health::default());
    let state = board_state(project.id(), &id, &file, &session, &BoardGraph::new());
    extras.boards.sessions().insert(id.clone(), session);
    if let Some(folder) = &folder {
        folder_used(&workspace, folder);
    }
    announce(&workspace, &mut vec![id], folder.is_some());
    Ok(state)
}

/// `reload_board`: reads the board's file again and returns it.
pub fn reload_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let source = opened.source()?;
    let state = {
        let mut sessions = opened.boards().sessions();
        let (session, graph) = load_session(
            &opened.files(),
            opened.boards(),
            &source,
            &opened.id,
            &opened.file,
        )?;
        let state = opened.state(&session, &graph);
        sessions.insert(opened.id.clone(), session);
        state
    };
    publish_problems(&opened.workspace, opened.boards());
    Ok(state)
}

/// `close_board`: closes the board and drops its session (the window saves first).
pub fn close_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    parse_id(&args.id)?;
    project.open_boards().check(&args.id)?;
    project.open_boards().close(&args.id);
    project.extras().boards.forget_sessions(&[args.id]);
    Ok(())
}

/// `save_board`: validates, signs, and writes the edited board over its file (with no check of
/// what the file holds now: the window's version wins), and returns it as saved.
pub fn save_board(ctx: &Ctx<'_>, args: SaveBoardArgs) -> CommandResult<BoardState> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let source = opened.source()?;
    let state = {
        let mut sessions = opened.boards().sessions();
        let (session, _) = opened.session(&mut sessions, &source)?;
        let board = parse_edit(&args.board.0.to_string(), session.board.folder.clone())?;
        opened.save(session, &source, board)?
    };
    publish_problems(&opened.workspace, opened.boards());
    opened.announce(vec![opened.id.clone()], false);
    Ok(state)
}

/// `board_health`: the board's health and outlines against the graph now.
pub fn board_health(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardHealthView> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let source = opened.source()?;
    let view = {
        let mut sessions = opened.boards().sessions();
        let (session, _) = opened.session(&mut sessions, &source)?;
        let graph = source.board(&session.board)?;
        session.health = unfold_reader::board::board_health(&session.board, &graph);
        opened.boards().cache(
            &source,
            &opened.id,
            &session.board,
            session.revision,
            &session.health,
        );
        BoardHealthView {
            outlines: session
                .board
                .pages
                .iter()
                .map(|page| outline(page, &graph))
                .collect(),
            label: session.health.label(),
            health: session.health.clone(),
        }
    };
    publish_problems(&opened.workspace, opened.boards());
    Ok(view)
}

/// `repair_board`: applies one repair a health item offers, on its page, and saves the board.
pub fn repair_board(ctx: &Ctx<'_>, args: RepairBoardArgs) -> CommandResult<BoardState> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let page_id = args.repair.page;
    let repair = args.repair.repair.into_repair()?;
    let source = opened.source()?;
    let state = {
        let mut sessions = opened.boards().sessions();
        let (session, _) = opened.session(&mut sessions, &source)?;
        let mut board = session.board.clone();
        // Reconnecting reads the new key, or the new anchor, from the graph too.
        let graph = {
            let mut references = board.references();
            match &repair {
                Repair::Reconnect { to, .. } => references.keys.push(to),
                Repair::ReconnectAnchor {
                    to: Anchor::Key { key },
                    ..
                } => references.keys.push(key.as_str()),
                Repair::ReconnectAnchor {
                    to: Anchor::File { file, .. },
                    ..
                } => references.files.push(file),
                _ => {}
            }
            source.load(&references)?
        };
        let page = board
            .page_mut(&page_id)
            .ok_or_else(|| CommandError::invalid_args(format!("the board has no page `{page_id}`")))?;
        apply_repair(page, &graph, &repair)?;
        opened.save(session, &source, board)?
    };
    publish_problems(&opened.workspace, opened.boards());
    opened.announce(vec![opened.id.clone()], false);
    Ok(state)
}

/// `delete_board`: deletes the board's file (the UI asks first) and closes it. Its folder stays,
/// empty, when no other board is in it.
pub fn delete_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let files = opened.files();
    let folder = {
        let mut sessions = opened.boards().sessions();
        let folder = match sessions.get(&opened.id) {
            Some(session) => session.board.folder.clone(),
            None => files
                .load(&opened.file)
                .ok()
                .and_then(|loaded| loaded.board.folder),
        };
        files.remove(&opened.file)?;
        opened.project.open_boards().close(&opened.id);
        sessions.remove(&opened.id);
        opened.boards().forget(&opened.id);
        folder
    };
    if let Some(folder) = &folder {
        folder_left(&files, &opened.workspace, folder);
    }
    publish_problems(&opened.workspace, opened.boards());
    opened.announce(vec![opened.id.clone()], folder.is_some());
    Ok(())
}

/// The Mermaid text of page `page` of open board `id` (its last saved version).
pub(crate) fn page_mermaid(
    ctx: &Ctx<'_>,
    project: &ProjectId,
    id: &str,
    page: &str,
) -> CommandResult<String> {
    let opened = opened(ctx, project, id)?;
    let source = opened.source()?;
    let mut sessions = opened.boards().sessions();
    let (session, graph) = opened.session(&mut sessions, &source)?;
    let graph = match graph {
        Some(graph) => graph,
        None => source.board(&session.board)?,
    };
    mermaid::page(&session.board, page, &graph)
        .ok_or_else(|| CommandError::not_found(format!("board `{id}` has no page `{page}`")))
}
