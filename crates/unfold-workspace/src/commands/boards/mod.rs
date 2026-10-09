//! Board commands (contract §3.5, §4.6, §6; redesign R-5, R-6, R-7, R-10, R-14).
//!
//! A board belongs to a project and is addressed as `{project, id}`, its id being its public
//! file's path ([`files`]). It is open in one window at a time: `open_board` and `new_board`
//! claim it for the calling window (a board another window holds answers `held` with that
//! window, so the UI can focus it), `move_board` hands the claim over, and `close_board`,
//! `delete_board`, and a closing window release it. Every other command about one board checks
//! that the calling window holds it, except the folder commands: a board's folder is metadata
//! Rust owns (decision D-12), never part of what a window saves, so they work on any board.
//!
//! The project keeps a **session** per open board ([`session`]): the working version last
//! loaded or saved. A window autosaves its edits (`save_board`, a [`BoardEdit`]): Rust checks
//! them against the graphs of every project the cards come from ([`graphs`]: no new missing
//! reference, broken group, held reference, or broken link), signs new descriptions, labels, and
//! link labels, and writes the board's **working file**: a draft; or, for a published board, its
//! changes copy, so teammates see nothing until Publish (R-6). A save is refused with
//! `board_conflict` when the working file changed on disk since the session's revision, unless
//! `force`; the holding window also gets a `board_conflict` event as soon as the watcher sees the
//! change ([`health::check_disk`]).
//!
//! | | Draft | Published, no copy | Published, with copy |
//! | --- | --- | --- | --- |
//! | `save_board` | Writes the draft | Writes a new copy (based on the published revision loaded) | Writes the copy; deletes it when equal to the published version |
//! | `publish_board` | Creates `boards/<name>.json` (never overwriting: `name_taken`), deletes the draft; new id | Nothing to do | Copy over the published file, unless it changed since the copy's base (`conflict`; `keep_mine` writes anyway) |
//! | `discard_changes` | `invalid_args` | Reloads the published file | Deletes the copy, reloads the published file |
//! | `unpublish_board` | `invalid_args` | The published version becomes a draft; new id | The copy's version becomes a draft; new id |
//! | `delete_board` | Deletes the draft | Deletes the published file | Deletes both |
//!
//! When a board's id changes (first Publish, Move back to drafts), its claim, session, view
//! state, health, and recent-board entry follow it. Every board's health is cached per board
//! ([`session::BoardExtras`]) and feeds the Problems list and Home's Health line; it is
//! refreshed after the graphs of the project or of a project its cards come from change.

mod files;
mod folders;
mod graphs;
mod health;
mod lists;
mod session;
mod types;

use std::collections::BTreeMap;
use std::sync::Arc;

use unfold_reader::board::files::{BoardFile, Change, Loaded, Place, slug};
use unfold_reader::board::{
    Board, BoardError, Health, ProjectName, Repair, apply_repair, outline, parse_edit, prepare_save,
    serialize,
};
use unfold_reader::mermaid;

pub use self::folders::{create_folder, delete_folder, rename_folder, set_board_folder};
pub(crate) use self::health::{check_disk, refresh};
pub use self::lists::{board_refs, board_thumbnails, drafts, list_boards};
pub use self::session::BoardExtras;
pub use self::types::{
    BoardDocument, BoardEdit, BoardHealthView, BoardIdArgs, BoardList, BoardRefHit, BoardRefs, BoardState,
    BoardSummary, CardKind, Drafts, LabelSpec, MoveBoardArgs, NewBoardArgs, OpenBoardResult,
    PublishBoardArgs, PublishOutcome, PublishedState, RenameFolderArgs, RenamedFolder, RepairBoardArgs,
    RepairKindSpec, RepairSpec, Resolve, SaveBoardArgs, SaveBoardViewArgs, SetBoardFolderArgs, TargetSpec,
    Thumbnail, ThumbnailCard, ThumbnailPage, Thumbnails, ThumbnailsArgs,
};

use self::files::{Files, Working, parse_id};
use self::folders::{folder_arg, folder_left, folder_used};
use self::graphs::{Graphs, Resolver};
use self::session::{Session, health_of, publish_problems};
use super::ProjectExtras;
use crate::error::WorkspaceError;
use crate::events::{WorkingFile, WorkspaceEvent};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{ProjectHandle, ProjectId};
use crate::workspace::Workspace;

/// A board the calling window holds, in its (woken) project.
struct Held<'c> {
    project: ProjectHandle<'c>,
    workspace: Arc<Workspace>,
    extras: Arc<ProjectExtras>,
    file: BoardFile,
    id: String,
}

/// Board `id` of `project`, which the calling window must hold; wakes the project.
fn held<'c>(ctx: &'c Ctx<'_>, project: &ProjectId, id: &str) -> CommandResult<Held<'c>> {
    let handle = ctx.project(project)?;
    let file = parse_id(id)?;
    handle.claims().check(id, ctx.window)?;
    let workspace = handle.wake()?;
    Ok(Held {
        extras: handle.extras(),
        workspace,
        project: handle,
        file,
        id: id.to_owned(),
    })
}

/// A session the board must have (a claim without one is a broken invariant).
fn session_mut<'s>(sessions: &'s mut BTreeMap<String, Session>, id: &str) -> CommandResult<&'s mut Session> {
    sessions
        .get_mut(id)
        .ok_or_else(|| CommandError::internal(format!("board `{id}` has no session")))
}

/// Reads board `file`'s working version into a new session, with its health (cached) and the
/// graphs it was checked against.
fn load_session(
    files: &Files<'_>,
    extras: &BoardExtras,
    resolver: &mut Resolver<'_>,
    id: &str,
    file: &BoardFile,
) -> CommandResult<(Session, Graphs)> {
    let working = files.working(file)?;
    let board = &working.loaded.board;
    let graphs = resolver.board_graphs(board)?;
    let health = board_health_of(board, &graphs);
    extras.cache(resolver, id, board, working.loaded.revision, &health);
    let published = files.published_state(file, board, working.kind, working.loaded.base);
    Ok((Session::new(file.clone(), working, health, published), graphs))
}

impl Held<'_> {
    fn files(&self) -> Files<'_> {
        Files::live(&self.workspace)
    }

    fn boards(&self) -> &BoardExtras {
        &self.extras.boards
    }

    fn resolver<'p>(&self, ctx: &'p Ctx<'_>) -> CommandResult<Resolver<'p>> {
        Resolver::live(ctx.projects(), &self.workspace)
    }

    /// The board's session, read from disk when it has none (with the graphs it was read with).
    fn session<'s>(
        &self,
        sessions: &'s mut BTreeMap<String, Session>,
        resolver: &mut Resolver<'_>,
    ) -> CommandResult<(&'s mut Session, Option<Graphs>)> {
        let mut graphs = None;
        if !sessions.contains_key(&self.id) {
            let (session, read) = load_session(&self.files(), self.boards(), resolver, &self.id, &self.file)?;
            sessions.insert(self.id.clone(), session);
            graphs = Some(read);
        }
        Ok((session_mut(sessions, &self.id)?, graphs))
    }

    /// Board `id` as the window shows it.
    fn state(&self, id: &str, session: &Session, graphs: &Graphs) -> BoardState {
        let graphs = graphs.page();
        BoardState {
            project: self.project.id().clone(),
            id: id.to_owned(),
            place: session.file.place,
            name: session.file.name.clone(),
            document: BoardDocument::new(session.board.clone()),
            revision: session.revision.0.to_string(),
            published: session.published.clone(),
            outlines: session
                .board
                .pages
                .iter()
                .map(|page| outline(page, &graphs))
                .collect(),
            label: session.health.label(),
            health: session.health.clone(),
            view: self.workspace.local_state().board_view(id).cloned(),
        }
    }

    /// Keeps the projects the board's cards come from live while it is open.
    fn reference(&self, id: &str, resolver: &mut Resolver<'_>, board: &Board) {
        self.project
            .claims()
            .set_references(id, resolver.references(board));
    }

    /// Validates, signs, and writes `board` as the session's working version, refusing outside
    /// changes to the working file unless `force`.
    fn save(
        &self,
        session: &mut Session,
        resolver: &mut Resolver<'_>,
        board: Board,
        force: bool,
    ) -> CommandResult<BoardState> {
        let files = self.files();
        let change = files.dirs.change(&session.working_file(), session.revision)?;
        if !force && change != Change::Unchanged {
            return Err(CommandError::new(
                "board_conflict",
                format!(
                    "`{}` changed on disk since it was loaded; reload it or save with force",
                    self.id
                ),
            ));
        }
        // Keeping mine over a published file that changed starts the copy from what is there now.
        let base = match (session.kind, change) {
            (WorkingFile::Published, Change::Changed(now)) => Some(now),
            _ => session.base(),
        };
        let graphs = resolver.board_graphs(&board)?;
        let (signed, health) = prepare_save(board, Some((&session.board, &session.health)), &graphs.page())?;
        let written = files.write(&self.file, signed, base)?;
        // Back on the published file: its own health (its guards may differ).
        let health = if written.kind == WorkingFile::Published {
            board_health_of(&written.board, &graphs)
        } else {
            health
        };
        self.boards()
            .cache(resolver, &self.id, &written.board, written.revision, &health);
        session.wrote(written, health);
        self.reference(&self.id, resolver, &session.board);
        Ok(self.state(&self.id, session, &graphs))
    }

    /// Opens the board in its session (read from disk when it has none) and returns it.
    fn open(&self, ctx: &Ctx<'_>) -> CommandResult<BoardState> {
        let mut resolver = self.resolver(ctx)?;
        let mut sessions = self.boards().sessions();
        let (session, graphs) = self.session(&mut sessions, &mut resolver)?;
        let graphs = match graphs {
            Some(graphs) => graphs,
            None => resolver.board_graphs(&session.board)?,
        };
        self.reference(&self.id, &mut resolver, &session.board);
        Ok(self.state(&self.id, session, &graphs))
    }

    /// Writes new draft `board` (empty, so healthy) and opens a session on it.
    fn create(&self, ctx: &Ctx<'_>, files: &Files<'_>, board: Board) -> CommandResult<BoardState> {
        let written = files.write(&self.file, board, None)?;
        let mut resolver = self.resolver(ctx)?;
        self.boards().cache(
            &mut resolver,
            &self.id,
            &written.board,
            written.revision,
            &Health::default(),
        );
        let working = Working {
            kind: WorkingFile::Draft,
            loaded: Loaded {
                board: written.board,
                revision: written.revision,
                base: None,
            },
        };
        let session = Session::new(self.file.clone(), working, Health::default(), None);
        let state = self.state(&self.id, &session, &Graphs::default());
        self.boards().sessions().insert(self.id.clone(), session);
        Ok(state)
    }

    /// Tells every window that boards of the project changed (the watcher doesn't report the
    /// app's own writes).
    fn announce(&self, mut boards: Vec<String>, folders: bool) {
        announce(&self.workspace, &mut boards, folders);
    }

    /// The board's id became `to` (first Publish, Move back to drafts): its claim, view state,
    /// and recent-board entry follow it.
    fn moved(&self, ctx: &Ctx<'_>, to: &str) {
        // Remembered state is a convenience: a failed write never undoes the move.
        let _ = self.workspace.update_local_state(|state| {
            state.move_board_view(&self.id, to);
        });
        let _ = ctx.projects().move_recent_board(self.project.id(), &self.id, to);
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

/// The health of every page of `board` against `graphs`.
fn board_health_of(board: &Board, graphs: &Graphs) -> Health {
    unfold_reader::board::board_health(board, &graphs.page())
}

/// `text`, or `fallback` when absent or blank.
fn nonblank(text: Option<String>, fallback: &str) -> String {
    text.filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

/// `open_board`: claims the board for the calling window and returns it, or answers `held`
/// with the window that has it. Records it as a recent board.
pub fn open_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<OpenBoardResult> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    // A board the window already holds stays held whatever happens here.
    let reopened = project.claims().holder(&args.id) == Some(ctx.window);
    match project.claims().claim(&args.id, ctx.window) {
        Ok(()) => {}
        Err(WorkspaceError::BoardHeld { window, .. }) => {
            return Ok(OpenBoardResult::Held { held_by: window });
        }
        Err(other) => return Err(other.into()),
    }
    let release = |project: &ProjectHandle<'_>| {
        if !reopened {
            project.claims().release(&args.id, ctx.window);
        }
    };
    let workspace = match project.wake() {
        Ok(workspace) => workspace,
        Err(error) => {
            release(&project);
            return Err(error.into());
        }
    };
    let held = Held {
        extras: project.extras(),
        workspace,
        project,
        file,
        id: args.id.clone(),
    };
    let board = match held.open(ctx) {
        Ok(board) => board,
        Err(error) => {
            release(&held.project);
            return Err(error);
        }
    };
    // Recents are a convenience: a failed write never fails the open.
    let _ = ctx
        .projects()
        .record_recent_board(held.project.id(), &held.id, &board.document.board.title);
    Ok(OpenBoardResult::Opened {
        board: Box::new(board),
    })
}

/// `new_board`: creates an empty draft with one page, opens it in the calling window, and
/// returns it. `not_compiled` before the project's first Compile (drafts live in `local/`).
pub fn new_board(ctx: &Ctx<'_>, args: NewBoardArgs) -> CommandResult<BoardState> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    if !workspace.layout().is_compiled() {
        return Err(WorkspaceError::NotCompiled.into());
    }
    let folder = args.folder.map(folder_arg).transpose()?;
    let mut board = Board::new(&nonblank(args.title, "Untitled board"));
    board.folder.clone_from(&folder);
    if let Some(page) = board.pages.first_mut() {
        page.title = nonblank(args.page_title, "Page 1");
    }
    let files = Files::live(&workspace);
    let file = files.new_draft()?;
    let id = file.relative();
    project.claims().claim(&id, ctx.window)?;
    let held = Held {
        extras: project.extras(),
        workspace: Arc::clone(&workspace),
        project,
        file,
        id,
    };
    let created = held.create(ctx, &files, board);
    if created.is_err() {
        held.project.claims().release(&held.id, ctx.window);
    }
    let state = created?;
    if let Some(folder) = &folder {
        folder_used(&workspace, folder);
    }
    held.announce(vec![held.id.clone()], folder.is_some());
    Ok(state)
}

/// `reload_board`: reads the board's working file again (after `board_conflict`, or "take
/// theirs") and returns it.
pub fn reload_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let held = held(ctx, &args.project, &args.id)?;
    let mut resolver = held.resolver(ctx)?;
    let state = {
        let mut sessions = held.boards().sessions();
        let (session, graphs) =
            load_session(&held.files(), held.boards(), &mut resolver, &held.id, &held.file)?;
        held.reference(&held.id, &mut resolver, &session.board);
        let state = held.state(&held.id, &session, &graphs);
        sessions.insert(held.id.clone(), session);
        state
    };
    publish_problems(&held.workspace, held.boards());
    Ok(state)
}

/// `close_board`: releases the calling window's claim (the window saves first).
pub fn close_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    parse_id(&args.id)?;
    project.claims().check(&args.id, ctx.window)?;
    project.claims().release(&args.id, ctx.window);
    project.extras().boards.forget_sessions(&[args.id]);
    Ok(())
}

/// `move_board`: hands the claim to another window (Open in New Window).
pub fn move_board(ctx: &Ctx<'_>, args: MoveBoardArgs) -> CommandResult<()> {
    let project = ctx.project(&args.project)?;
    parse_id(&args.id)?;
    Ok(project.claims().transfer(&args.id, ctx.window, args.to_window)?)
}

/// `save_board`: validates, signs, and writes the edited board as its working version (drafts
/// and unpublished changes autosave here), and returns it as saved.
pub fn save_board(ctx: &Ctx<'_>, args: SaveBoardArgs) -> CommandResult<BoardState> {
    let held = held(ctx, &args.project, &args.id)?;
    let mut resolver = held.resolver(ctx)?;
    let state = {
        let mut sessions = held.boards().sessions();
        let (session, _) = held.session(&mut sessions, &mut resolver)?;
        let board = parse_edit(&args.board.0.to_string(), session.board.folder.clone())?;
        if ProjectName::parse(held.project.name()).is_some_and(|own| board.projects.contains(&own)) {
            return Err(CommandError::invalid_args(format!(
                "a card names `{}`, this board's own project: omit project for the board's own project",
                held.project.name()
            )));
        }
        held.save(session, &mut resolver, board, args.force.unwrap_or(false))?
    };
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![held.id.clone()], false);
    Ok(state)
}

/// `save_board_view`: remembers the board's active page and each page's camera and lens in
/// `state.json`.
pub fn save_board_view(ctx: &Ctx<'_>, args: SaveBoardViewArgs) -> CommandResult<()> {
    let held = held(ctx, &args.project, &args.id)?;
    held.workspace
        .update_local_state(|state| state.put_board_view(&held.id, args.view))?;
    Ok(())
}

/// `board_health`: the board's health and outlines against the graphs now.
pub fn board_health(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardHealthView> {
    let held = held(ctx, &args.project, &args.id)?;
    let mut resolver = held.resolver(ctx)?;
    let view = {
        let mut sessions = held.boards().sessions();
        let (session, _) = held.session(&mut sessions, &mut resolver)?;
        let graphs = resolver.board_graphs(&session.board)?;
        session.health = board_health_of(&session.board, &graphs);
        held.boards().cache(
            &mut resolver,
            &held.id,
            &session.board,
            session.revision,
            &session.health,
        );
        let graphs = graphs.page();
        BoardHealthView {
            outlines: session
                .board
                .pages
                .iter()
                .map(|page| outline(page, &graphs))
                .collect(),
            label: session.health.label(),
            health: session.health.clone(),
        }
    };
    publish_problems(&held.workspace, held.boards());
    Ok(view)
}

/// `repair_board`: applies one repair a health item offers, on its page, and saves the board.
pub fn repair_board(ctx: &Ctx<'_>, args: RepairBoardArgs) -> CommandResult<BoardState> {
    let held = held(ctx, &args.project, &args.id)?;
    let page_id = args.repair.page;
    let repair = args.repair.repair.into_repair()?;
    let mut resolver = held.resolver(ctx)?;
    let state = {
        let mut sessions = held.boards().sessions();
        let (session, _) = held.session(&mut sessions, &mut resolver)?;
        let mut board = session.board.clone();
        // Reconnecting reads the new key from its project's graph too.
        let reconnect = match &repair {
            Repair::Reconnect { project, to, .. } => Some((project.clone(), to.as_str())),
            _ => None,
        };
        let graphs = resolver.graphs(&board.projects, |project| {
            let mut keys = board.references(project);
            if let Some((reconnected, to)) = &reconnect
                && reconnected.as_ref() == project
            {
                keys.push(to);
            }
            keys
        })?;
        let page = board
            .page_mut(&page_id)
            .ok_or_else(|| CommandError::invalid_args(format!("the board has no page `{page_id}`")))?;
        apply_repair(page, &graphs.page(), &repair)?;
        held.save(session, &mut resolver, board, false)?
    };
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![held.id.clone()], false);
    Ok(state)
}

/// `publish_board`: a draft's first Publish writes `.unfold/boards/<name>.json` (never
/// overwriting: `name_taken` with a free name) and deletes the draft (the board's id changes); a
/// published board's unpublished changes are written over its published file, unless that file
/// changed since the changes started from it (`conflict`, settled with `resolve: "keep_mine"`).
pub fn publish_board(ctx: &Ctx<'_>, args: PublishBoardArgs) -> CommandResult<PublishOutcome> {
    let held = held(ctx, &args.project, &args.id)?;
    if args.name.is_some() && held.file.place != Place::Draft {
        return Err(CommandError::invalid_args(
            "a file name is chosen only at a draft's first Publish",
        ));
    }
    let mut resolver = held.resolver(ctx)?;
    let files = held.files();
    let (outcome, moved) = {
        let mut sessions = held.boards().sessions();
        let (session, _) = held.session(&mut sessions, &mut resolver)?;
        if held.file.place == Place::Draft {
            let graphs = resolver.board_graphs(&session.board)?;
            let (signed, health) = prepare_save(
                session.board.clone(),
                Some((&session.board, &session.health)),
                &graphs.page(),
            )?;
            let name = args.name.unwrap_or_else(|| slug(&signed.title));
            let (file, revision) = match files.create_published(&name, &serialize(&signed)) {
                Ok(created) => created,
                Err(BoardError::NameTaken { path, suggestion }) => {
                    return Ok(PublishOutcome::NameTaken { path, suggestion });
                }
                Err(other) => return Err(other.into()),
            };
            files.remove(&held.file)?;
            let id = file.relative();
            held.project.claims().rename(&held.id, &id);
            let mut session = sessions
                .remove(&held.id)
                .ok_or_else(|| CommandError::internal("the published draft lost its session"))?;
            session.file = file;
            session.kind = WorkingFile::Published;
            session.board = signed;
            session.revision = revision;
            session.base = None;
            session.health = health;
            session.published = Some(PublishedState::current(revision));
            session.reported = None;
            held.boards().forget(&held.id);
            held.boards()
                .cache(&mut resolver, &id, &session.board, revision, &session.health);
            held.reference(&id, &mut resolver, &session.board);
            held.moved(ctx, &id);
            let state = held.state(&id, &session, &graphs);
            sessions.insert(id.clone(), session);
            (
                PublishOutcome::Done {
                    board: Box::new(state),
                },
                Some(id),
            )
        } else {
            if session.kind == WorkingFile::Changes {
                let base = session
                    .base
                    .ok_or_else(|| CommandError::internal("a changes copy without its base revision"))?;
                let theirs = files.dirs.change(&held.file, base)?;
                if theirs != Change::Unchanged && args.resolve != Some(Resolve::KeepMine) {
                    let theirs_revision = match theirs {
                        Change::Changed(revision) => Some(revision.0.to_string()),
                        Change::Missing | Change::Unchanged => None,
                    };
                    return Ok(PublishOutcome::Conflict { theirs_revision });
                }
                let revision = files.overwrite_published(&held.file, &serialize(&session.board))?;
                files.remove(&session.working_file())?;
                held.boards().rekey(&held.id, session.revision, revision);
                session.kind = WorkingFile::Published;
                session.revision = revision;
                session.base = None;
                session.published = Some(PublishedState::current(revision));
                session.reported = None;
            }
            let graphs = resolver.board_graphs(&session.board)?;
            (
                PublishOutcome::Done {
                    board: Box::new(held.state(&held.id, session, &graphs)),
                },
                None,
            )
        }
    };
    publish_problems(&held.workspace, held.boards());
    held.announce(std::iter::once(held.id.clone()).chain(moved).collect(), false);
    Ok(outcome)
}

/// `discard_changes`: deletes a published board's changes copy and reads its published file
/// again ("take theirs" after a conflict, too).
pub fn discard_changes(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let held = held(ctx, &args.project, &args.id)?;
    let Some(copy) = BoardFile::changes_of(&held.file) else {
        return Err(CommandError::invalid_args(format!(
            "`{}` is a draft: it has no unpublished changes",
            held.id
        )));
    };
    let mut resolver = held.resolver(ctx)?;
    let files = held.files();
    let state = {
        let mut sessions = held.boards().sessions();
        files.remove(&copy)?;
        let (session, graphs) = load_session(&files, held.boards(), &mut resolver, &held.id, &held.file)?;
        held.reference(&held.id, &mut resolver, &session.board);
        let state = held.state(&held.id, &session, &graphs);
        sessions.insert(held.id.clone(), session);
        state
    };
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![held.id.clone()], false);
    Ok(state)
}

/// `unpublish_board`: Move back to drafts. The working version becomes a draft; the published
/// file and its changes copy are deleted; the board's id changes.
pub fn unpublish_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardState> {
    let held = held(ctx, &args.project, &args.id)?;
    let Some(copy) = BoardFile::changes_of(&held.file) else {
        return Err(CommandError::invalid_args(format!("`{}` is a draft", held.id)));
    };
    let mut resolver = held.resolver(ctx)?;
    let files = held.files();
    let (state, id) = {
        let mut sessions = held.boards().sessions();
        let (session, _) = held.session(&mut sessions, &mut resolver)?;
        let draft = files.dirs.free_draft(&held.file.name)?;
        let written = files.write(&draft, session.board.clone(), None)?;
        files.remove(&held.file)?;
        files.remove(&copy)?;
        let id = draft.relative();
        held.project.claims().rename(&held.id, &id);
        let mut session = sessions
            .remove(&held.id)
            .ok_or_else(|| CommandError::internal("the unpublished board lost its session"))?;
        session.file = draft;
        let health = std::mem::take(&mut session.health);
        session.wrote(written, health);
        held.boards().forget(&held.id);
        held.boards().cache(
            &mut resolver,
            &id,
            &session.board,
            session.revision,
            &session.health,
        );
        held.reference(&id, &mut resolver, &session.board);
        held.moved(ctx, &id);
        let graphs = resolver.board_graphs(&session.board)?;
        let state = held.state(&id, &session, &graphs);
        sessions.insert(id.clone(), session);
        (state, id)
    };
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![held.id.clone(), id], false);
    Ok(state)
}

/// `delete_board`: deletes the board's files (the UI asks first), closes it, and forgets its
/// view state and recent-board entry. Its folder stays, empty, when no other board is in it.
pub fn delete_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<()> {
    let held = held(ctx, &args.project, &args.id)?;
    let files = held.files();
    let folder = {
        let mut sessions = held.boards().sessions();
        let folder = match sessions.get(&held.id) {
            Some(session) => session.board.folder.clone(),
            None => files
                .working(&held.file)
                .ok()
                .and_then(|working| working.loaded.board.folder),
        };
        if let Some(copy) = BoardFile::changes_of(&held.file) {
            files.remove(&copy)?;
        }
        files.remove(&held.file)?;
        held.project.claims().release(&held.id, ctx.window);
        sessions.remove(&held.id);
        held.boards().forget(&held.id);
        folder
    };
    let _ = held.workspace.update_local_state(|state| {
        state.remove_board_view(&held.id);
    });
    let _ = ctx.projects().forget_recent_board(held.project.id(), &held.id);
    if let Some(folder) = &folder {
        folder_left(&files, &held.workspace, folder);
    }
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![held.id.clone()], folder.is_some());
    Ok(())
}

/// `duplicate_board`: a new draft "<title> copy" in the same folder, from the board's working
/// version. The copy is not opened.
pub fn duplicate_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<BoardSummary> {
    let held = held(ctx, &args.project, &args.id)?;
    let mut resolver = held.resolver(ctx)?;
    let files = held.files();
    let mut board = {
        let mut sessions = held.boards().sessions();
        held.session(&mut sessions, &mut resolver)?.0.board.clone()
    };
    board.title = format!("{} copy", board.title);
    let draft = files.new_draft()?;
    let written = files.write(&draft, board, None)?;
    let id = draft.relative();
    let loaded = Loaded {
        board: written.board,
        revision: written.revision,
        base: None,
    };
    let label = health_of(held.boards(), &mut resolver, &id, &loaded)?.and_then(|health| health.label());
    let summary = lists::summary(
        held.project.id(),
        &files,
        None,
        &draft,
        Ok(Working {
            kind: WorkingFile::Draft,
            loaded,
        }),
        label,
    );
    publish_problems(&held.workspace, held.boards());
    held.announce(vec![id], false);
    Ok(summary)
}

/// The Mermaid text of page `page` of board `id`, which the calling window holds (its last
/// saved version).
pub(crate) fn page_mermaid(
    ctx: &Ctx<'_>,
    project: &ProjectId,
    id: &str,
    page: &str,
) -> CommandResult<String> {
    let held = held(ctx, project, id)?;
    let mut resolver = held.resolver(ctx)?;
    let mut sessions = held.boards().sessions();
    let (session, graphs) = held.session(&mut sessions, &mut resolver)?;
    let graphs = match graphs {
        Some(graphs) => graphs,
        None => resolver.board_graphs(&session.board)?,
    };
    mermaid::page(&session.board, page, &graphs.page())
        .ok_or_else(|| CommandError::not_found(format!("board `{id}` has no page `{page}`")))
}

impl BoardExtras {
    /// Forgets the sessions of `boards` (their claims were released).
    pub(crate) fn forget_sessions(&self, boards: &[String]) {
        let mut sessions = self.sessions();
        for board in boards {
            sessions.remove(board);
        }
    }
}
