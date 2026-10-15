//! Board commands (docs/ipc.md#boards).
//!
//! A board belongs to a project and is addressed as `{project, id}`, its id being its file's
//! path ([`files`]): every board is a file in `.unfold/boards/`, saved as it is edited and
//! shared through git. `open_board` and `new_board` open it (the project's
//! [`crate::open_boards::OpenBoards`]), and `close_board`, `delete_board`, and a closing window
//! close it. Every other command about one board needs it open (`board_not_open`), except
//! `read_board`, which reads a board's file without opening it, `board_health`, which judges the
//! file of a board no window has open (opening nothing), and the folder commands: a board's
//! folder is metadata Rust owns (board folder metadata), never part of what a window saves, so
//! they work on any board.
//!
//! The project keeps a **session** per open board ([`session`]): the version last loaded or
//! saved. A window autosaves its edits (`save_board`, a [`BoardEdit`]): Rust checks them against
//! the project's graph ([`graphs`]: no new missing reference, broken combined step, held
//! reference, or broken link, and the overlay rules), signs new descriptions, names, and
//! combined steps, and writes the board over its file. Nothing writes over what the window
//! hasn't seen: a save or repair names the revision its version builds on, and is refused
//! (`board_changed`) when the file changed since, or isn't a valid board, unless the person
//! chose to keep their version (`overwrite`). A deleted file is written again. When the file
//! changes outside the app (another program, a `git pull`), the window that opened the board
//! gets `board_reloaded` ([`health::check_disk`]): a valid board reloads the session, keeping
//! the version it replaced (the person may keep the version they edited on: [`session`]); a
//! file that isn't a valid board leaves the session as it was.
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

use unfold_reader::board::files::{BoardFile, Change, Loaded, Revision};
use unfold_reader::board::{Anchor, Board, Health, Repair, apply_repair, parse_edit, prepare_save};

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

/// Reads board `file` into a new session, with its health (cached).
fn load_session(
    files: &Files<'_>,
    extras: &BoardExtras,
    source: &Source,
    id: &str,
    file: &BoardFile,
) -> CommandResult<Session> {
    new_session(extras, source, id, files.load(file)?)
}

/// A new session on board `id` as read from disk (`loaded`), with its health (cached).
fn new_session(extras: &BoardExtras, source: &Source, id: &str, loaded: Loaded) -> CommandResult<Session> {
    let graph = source.board(&loaded.board)?;
    let health = unfold_reader::board::board_health(&loaded.board, &graph);
    extras.cache(source, id, &loaded.board, loaded.revision, &health);
    Ok(Session::new(loaded, health))
}

/// Board `id` of `project` (its file `file`) as the window shows it.
fn board_state(project: &ProjectId, id: &str, file: &BoardFile, session: &Session) -> BoardState {
    BoardState {
        project: project.clone(),
        id: id.to_owned(),
        name: file.name().to_owned(),
        document: BoardDocument::new(session.board.clone()),
        revision: session.revision.0.to_string(),
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

    /// The board's session, read from disk when it has none.
    fn session<'s>(
        &self,
        sessions: &'s mut BTreeMap<String, Session>,
        source: &Source,
    ) -> CommandResult<&'s mut Session> {
        if !sessions.contains_key(&self.id) {
            let session = load_session(&self.files(), self.boards(), source, &self.id, &self.file)?;
            sessions.insert(self.id.clone(), session);
        }
        session_mut(sessions, &self.id)
    }

    /// The board as the window shows it.
    fn state(&self, session: &Session) -> BoardState {
        board_state(self.project.id(), &self.id, &self.file, session)
    }

    /// Refuses (`board_changed`) a write of a version that builds on the file at `base` when
    /// that isn't what the file holds: the window's version must build on the session's, and
    /// the file must still hold the session's version. A deleted file is written again.
    fn check_base(&self, session: &Session, base: Revision) -> CommandResult<()> {
        let current = match self.files().dirs.change(&self.file, session.revision) {
            Ok(Change::Missing) => true,
            Ok(Change::Unchanged) => session.builds_on(base),
            // Changed, or unreadable (larger than a board file can be): not what the window saw.
            Ok(Change::Changed(_)) | Err(_) => false,
        };
        if current {
            Ok(())
        } else {
            Err(CommandError::new(
                "board_changed",
                format!(
                    "`{}` changed on disk since this version of the board was loaded or saved, or isn't a \
                     valid board: the person chooses which version to keep",
                    self.id
                ),
            ))
        }
    }

    /// Validates, signs, and writes `board` over the board's file as the session's new version.
    ///
    /// `board` is checked against the session's version, and when that refuses it, against each
    /// version an outside change replaced since the last save, newest first: a window with edits
    /// of its own didn't take those reloads, so the version the person keeps may build on one of
    /// them, and a problem that version already had isn't new. The first check that passes wins.
    fn save(&self, session: &mut Session, source: &Source, board: Board) -> CommandResult<BoardState> {
        let graph = source.board(&board)?;
        let check = |previous: (&Board, &Health)| prepare_save(board.clone(), Some(previous), &graph);
        let (signed, health) = check((&session.board, &session.health)).or_else(|refused| {
            session
                .replaced
                .iter()
                .rev()
                .find_map(|(old, old_health)| check((old, old_health)).ok())
                .ok_or(refused)
        })?;
        let revision = self.files().save(&self.file, &signed)?;
        self.boards().cache(source, &self.id, &signed, revision, &health);
        *session = Session::new(
            Loaded {
                board: signed,
                revision,
            },
            health,
        );
        Ok(self.state(session))
    }

    /// Opens the board in its session (read from disk when it has none) and returns it.
    fn open(&self) -> CommandResult<BoardState> {
        let source = self.source()?;
        let mut sessions = self.boards().sessions();
        let session = self.session(&mut sessions, &source)?;
        Ok(self.state(session))
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
    let newly_opened = project.open_board(&args.id, ctx.window)?;
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
/// returns it. A window that closed meanwhile gets `window_closed`, and the file is deleted
/// again: nobody holds the board, and a retry would make a second one.
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
    let extras = project.extras();
    // An empty board is healthy.
    let session = Session::new(Loaded { board, revision }, Health::default());
    let state = board_state(project.id(), &id, &file, &session);
    extras.boards.sessions().insert(id.clone(), session);
    if let Err(error) = project.open_board(&id, ctx.window) {
        extras.boards.forget_sessions(std::slice::from_ref(&id));
        files.remove(&file)?;
        return Err(error.into());
    }
    extras
        .boards
        .cache(&source, &id, &state.document.board, revision, &Health::default());
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
        let session = load_session(
            &opened.files(),
            opened.boards(),
            &source,
            &opened.id,
            &opened.file,
        )?;
        let state = opened.state(&session);
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

/// `save_board`: validates, signs, and writes the edited board over its file, and returns it as
/// saved. Refused (`board_changed`) when the file changed since the version it builds on, or
/// isn't a valid board, unless the person chose their version (`overwrite`).
pub fn save_board(ctx: &Ctx<'_>, args: SaveBoardArgs) -> CommandResult<BoardState> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let source = opened.source()?;
    let state = {
        let mut sessions = opened.boards().sessions();
        let session = opened.session(&mut sessions, &source)?;
        if args.overwrite != Some(true) {
            opened.check_base(session, args.base)?;
        }
        let board = parse_edit(&args.board.0.to_string(), session.board.folder.clone())?;
        opened.save(session, &source, board)?
    };
    publish_problems(&opened.workspace, opened.boards());
    opened.announce(vec![opened.id.clone()], false);
    Ok(state)
}

/// `board_health`: the board's health against the graph now: its session's while a window has
/// it open, else its file's, which opens nothing (an agent's `check` of a board no window holds).
pub fn board_health(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardHealthView> {
    let handle = ctx.project(&args.project)?;
    if handle.open_boards().check(&args.id).is_err() {
        return file_health(&handle, &args.id);
    }
    let opened = opened(ctx, &args.project, &args.id)?;
    let source = opened.source()?;
    let view = {
        let mut sessions = opened.boards().sessions();
        let session = opened.session(&mut sessions, &source)?;
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
            label: session.health.label(),
            health: session.health.clone(),
        }
    };
    publish_problems(&opened.workspace, opened.boards());
    Ok(view)
}

/// The health of board `id` as its file holds it, cached as an open board's is.
fn file_health(handle: &ProjectHandle<'_>, id: &str) -> CommandResult<BoardHealthView> {
    let file = parse_id(id)?;
    let workspace = handle.wake()?;
    let extras = handle.extras();
    let source = Source::live(&workspace)?;
    let loaded = Files::live(&workspace).load(&file)?;
    let health = unfold_reader::board::board_health(&loaded.board, &source.board(&loaded.board)?);
    extras
        .boards
        .cache(&source, id, &loaded.board, loaded.revision, &health);
    publish_problems(&workspace, &extras.boards);
    Ok(BoardHealthView {
        label: health.label(),
        health,
    })
}

/// `repair_board`: applies one repair a health item offers, on its page, and saves the board.
/// Refused (`board_changed`) as `save_board` is.
pub fn repair_board(ctx: &Ctx<'_>, args: RepairBoardArgs) -> CommandResult<BoardState> {
    let opened = opened(ctx, &args.project, &args.id)?;
    let page_id = args.repair.page;
    let repair = args.repair.repair.into_repair()?;
    let source = opened.source()?;
    let state = {
        let mut sessions = opened.boards().sessions();
        let session = opened.session(&mut sessions, &source)?;
        opened.check_base(session, args.base)?;
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
