//! What lists and reads boards without opening them: a project's boards and empty folders, the
//! drafts of every project, board thumbnails, where a key is on boards, and one board's working
//! version. All of them read from disk and serve resting projects without waking them; only a
//! live project's boards carry a health label (health needs a current graph).

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use unfold_core::vocab::BlockKind;
use unfold_reader::board::files::{BoardFile, Place};
use unfold_reader::board::{CardRef, ProjectName, card_links};

use super::files::{Files, Working, parse_id, working_file};
use super::graphs::Resolver;
use super::session::{BoardExtras, health_of};
use super::types::{
    BoardDocument, BoardIdArgs, BoardList, BoardRefHit, BoardRefs, BoardSummary, CardKind, Drafts,
    ReadBoardResult, Thumbnail, ThumbnailCard, ThumbnailPage, Thumbnails, ThumbnailsArgs,
};
use crate::commands::{NoArgs, ProjectArgs, ProjectKeyArgs, count, key_arg};
use crate::events::{WindowId, WorkingFile};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::{ProjectHandle, ProjectId, Projects};
use crate::state::{LocalState, read_json};

/// The most boards `list_boards` lists.
const MAX_BOARDS: usize = 2_000;
/// The most boards `drafts` lists.
const MAX_DRAFTS: usize = 500;
/// The most boards one `board_thumbnails` call draws.
const MAX_THUMBNAILS: usize = 60;
/// The most cards a thumbnail lists.
const MAX_THUMBNAIL_CARDS: usize = 200;
/// The most pages `board_refs` lists.
const MAX_REFS: usize = 50;

/// Board `file` of project `project` as lists show it, from its working version (or why it
/// can't be read), with health label `label`.
pub(super) fn summary(
    project: &ProjectId,
    files: &Files<'_>,
    held_by: Option<WindowId>,
    file: &BoardFile,
    working: CommandResult<Working>,
    label: Option<String>,
) -> BoardSummary {
    let mut summary = BoardSummary {
        project: project.clone(),
        id: file.relative(),
        place: file.place,
        name: file.name.clone(),
        title: None,
        folder: None,
        pages: 0,
        cards: 0,
        projects: Vec::new(),
        changes: 0,
        updated_at_ms: files.modified_ms(file),
        held_by,
        label,
        error: None,
    };
    let working = match working {
        Ok(working) => working,
        Err(error) => {
            summary.error = Some(error.message);
            return summary;
        }
    };
    if working.kind == WorkingFile::Changes {
        summary.updated_at_ms = files.modified_ms(&working_file(file, working.kind));
        summary.changes = files
            .published_state(file, &working.loaded.board, working.kind, working.loaded.base)
            .map_or(0, |published| published.changes);
    }
    let board = working.loaded.board;
    summary.pages = count(board.pages.len());
    summary.cards = count(board.pages.iter().map(|page| page.cards.len()).sum());
    summary.title = Some(board.title);
    summary.folder = board.folder;
    summary.projects = board.projects;
    summary
}

/// Lists one project's boards, live or resting.
struct Lister<'p> {
    project: ProjectHandle<'p>,
    files: Files<'p>,
    /// A live project's health cache and graphs (labels); `None` while it rests.
    health: Option<(std::sync::Arc<crate::commands::ProjectExtras>, Resolver<'p>)>,
}

impl<'p> Lister<'p> {
    fn new(projects: &'p Projects, project: ProjectHandle<'p>) -> CommandResult<Self> {
        let files = Files::reading(project.root(), &project.layout());
        let health = match project.live() {
            Some(workspace) => Some((project.extras(), Resolver::live(projects, &workspace)?)),
            None => None,
        };
        Ok(Self {
            project,
            files,
            health,
        })
    }

    fn summary(&mut self, file: &BoardFile) -> BoardSummary {
        let id = file.relative();
        let working = self.files.working(file);
        let label = match (&working, &mut self.health) {
            (Ok(working), Some((extras, resolver))) => {
                let extras: &BoardExtras = &extras.boards;
                health_of(extras, resolver, &id, &working.loaded)
                    .ok()
                    .flatten()
                    .and_then(|health| health.label())
            }
            _ => None,
        };
        summary(
            self.project.id(),
            &self.files,
            self.project.claims().holder(&id),
            file,
            working,
            label,
        )
    }

    /// The project's drafts and published boards with unpublished changes.
    fn drafts(&mut self) -> Vec<BoardSummary> {
        let mut files = self.files.dirs.list(Place::Draft).unwrap_or_default();
        files.extend(self.files.dirs.changed().unwrap_or_default());
        files
            .iter()
            .map(|file| self.summary(file))
            .filter(|summary| summary.place == Place::Draft || summary.changes > 0)
            .collect()
    }
}

/// `list_boards`: a project's boards (published, then drafts, each by name) and its empty
/// folders, for the sidebar tree and Home's Boards.
pub fn list_boards(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<BoardList> {
    let project = ctx.project(&args.project)?;
    let empty_folders = match project.live() {
        Some(workspace) => workspace.local_state().empty_folders,
        None => read_json::<LocalState>(&project.layout().state_file()).empty_folders,
    };
    let mut lister = Lister::new(ctx.projects(), project)?;
    let mut files = lister.files.boards()?;
    files.truncate(MAX_BOARDS);
    let mut boards = Vec::with_capacity(files.len());
    for file in &files {
        ctx.check_cancelled()?;
        boards.push(lister.summary(file));
    }
    Ok(BoardList {
        project: args.project,
        boards,
        empty_folders,
    })
}

/// `read_board`: board `id`'s working version as its file holds it, with the file's revision,
/// its publish state, and the window holding it. Neither claims the board nor wakes its project.
pub fn read_board(ctx: &Ctx<'_>, args: BoardIdArgs) -> CommandResult<ReadBoardResult> {
    let project = ctx.project(&args.project)?;
    let file = parse_id(&args.id)?;
    let files = Files::reading(project.root(), &project.layout());
    let working = files.working(&file)?;
    let published = files.published_state(&file, &working.loaded.board, working.kind, working.loaded.base);
    let id = file.relative();
    Ok(ReadBoardResult {
        held_by: project.claims().holder(&id),
        project: args.project,
        place: file.place,
        name: file.name,
        document: BoardDocument::new(working.loaded.board),
        revision: working.loaded.revision.0.to_string(),
        published,
        id,
    })
}

/// `drafts`: every project's drafts and published boards with unpublished changes, most
/// recently written first.
pub fn drafts(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<Drafts> {
    let projects = ctx.projects();
    let ids = projects.ids();
    let mut drafts: Vec<BoardSummary> = ids
        .par_iter()
        .map(|id| {
            // A project that left the workspace meanwhile has no drafts to list.
            projects
                .handle(id)
                .ok()
                .and_then(|project| Lister::new(projects, project).ok())
                .map(|mut lister| lister.drafts())
                .unwrap_or_default()
        })
        .flatten()
        .collect();
    drafts.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.project.cmp(&b.project))
            .then_with(|| a.id.cmp(&b.id))
    });
    drafts.truncate(MAX_DRAFTS);
    Ok(Drafts { drafts })
}

/// `board_thumbnails`: what the thumbnails of a project's boards draw (their first pages).
pub fn board_thumbnails(ctx: &Ctx<'_>, args: ThumbnailsArgs) -> CommandResult<Thumbnails> {
    if args.boards.len() > MAX_THUMBNAILS {
        return Err(CommandError::invalid_args(format!(
            "at most {MAX_THUMBNAILS} boards per call"
        )));
    }
    let files = args
        .boards
        .iter()
        .map(|id| parse_id(id))
        .collect::<CommandResult<Vec<_>>>()?;
    let project = ctx.project(&args.project)?;
    let reading = Files::reading(project.root(), &project.layout());
    let mut resolver = Resolver::of(ctx.projects(), &project)?;
    let mut thumbnails = Vec::with_capacity(files.len());
    for (id, file) in args.boards.into_iter().zip(files) {
        ctx.check_cancelled()?;
        let drawn = reading
            .working(&file)
            .and_then(|working| thumbnail(&mut resolver, &id, &working));
        thumbnails.push(drawn.unwrap_or_else(|error| Thumbnail {
            board: id,
            page: None,
            pages: 0,
            cards: Vec::new(),
            edges: Vec::new(),
            links: Vec::new(),
            truncated: false,
            error: Some(error.message),
        }));
    }
    Ok(Thumbnails { thumbnails })
}

/// The thumbnail of board `id`, whose working version is `working`.
fn thumbnail(resolver: &mut Resolver<'_>, id: &str, working: &Working) -> CommandResult<Thumbnail> {
    let board = &working.loaded.board;
    let mut thumbnail = Thumbnail {
        board: id.to_owned(),
        page: None,
        pages: count(board.pages.len()),
        cards: Vec::new(),
        edges: Vec::new(),
        links: Vec::new(),
        truncated: false,
        error: None,
    };
    let Some(page) = board.pages.first() else {
        return Ok(thumbnail);
    };
    let graphs = resolver.graphs(&board.projects, |project| page.references(project))?;
    let graphs = graphs.page();
    let shown = &page.cards[..page.cards.len().min(MAX_THUMBNAIL_CARDS)];
    let ids: BTreeSet<&str> = shown.iter().map(|card| card.id.as_str()).collect();
    let both = |from: &str, to: &str| ids.contains(from) && ids.contains(to);
    thumbnail.page = Some(ThumbnailPage {
        id: page.id.clone(),
        title: page.title.clone(),
    });
    thumbnail.truncated = shown.len() < page.cards.len();
    thumbnail.cards = shown
        .iter()
        .map(|card| {
            let project = card.reference.project();
            let block = card
                .reference
                .block_key()
                .and_then(|key| graphs.get(project)?.block(key.as_str()));
            let lines = block.map(|block| {
                let lines = block.block().lines;
                lines.end.saturating_sub(lines.start) + 1
            });
            let kind = match (&card.reference, block.map(|block| block.block().kind)) {
                (CardRef::Entrance { .. }, _) => CardKind::Entrance,
                (_, Some(BlockKind::Interface | BlockKind::TypeAlias | BlockKind::Enum)) => CardKind::Type,
                (_, Some(BlockKind::Class)) => CardKind::Class,
                _ => CardKind::Block,
            };
            let fields = block
                .filter(|_| matches!(kind, CardKind::Type | CardKind::Class))
                .map(|block| {
                    let members = block
                        .file()
                        .types
                        .members
                        .iter()
                        .filter(|member| member.block == block.local())
                        .count();
                    // An alias with no members lists its aliased type.
                    count(members.max(usize::from(block.block().kind == BlockKind::TypeAlias)))
                });
            ThumbnailCard {
                id: card.id.clone(),
                at: card.at,
                kind,
                view: page.view_of(card),
                folded: card.folded,
                lines,
                fields,
                foreign: project.is_some(),
            }
        })
        .collect();
    let mut seen = BTreeSet::new();
    thumbnail.edges = card_links(page, &graphs)
        .into_iter()
        .filter(|link| both(&link.source, &link.target))
        .map(|link| [link.source, link.target])
        .filter(|edge| seen.insert(edge.clone()))
        .collect();
    thumbnail.links = page
        .links
        .iter()
        .filter(|link| both(&link.from, &link.to))
        .map(|link| [link.from.clone(), link.to.clone()])
        .collect();
    Ok(thumbnail)
}

/// `board_refs`: the board pages of every project where key `key` of project `project` is a
/// card ("Already on …").
pub fn board_refs(ctx: &Ctx<'_>, args: ProjectKeyArgs) -> CommandResult<BoardRefs> {
    let target = ctx.project(&args.project)?.id().clone();
    let key = key_arg(&args.key)?;
    let projects = ctx.projects();
    let mut resolved: BTreeMap<ProjectName, Option<ProjectId>> = BTreeMap::new();
    let mut refs = Vec::new();
    for id in projects.ids() {
        let Ok(project) = projects.handle(&id) else {
            continue;
        };
        let files = Files::reading(project.root(), &project.layout());
        for file in files.boards().unwrap_or_default() {
            ctx.check_cancelled()?;
            let Ok(working) = files.working(&file) else {
                continue;
            };
            let board = &working.loaded.board;
            for page in &board.pages {
                let on_page = page.cards.iter().any(|card| {
                    card.reference.key().as_str() == key
                        && match card.reference.project() {
                            None => id == target,
                            Some(name) => {
                                resolved
                                    .entry(name.clone())
                                    .or_insert_with(|| projects.resolve_name(name.as_str()))
                                    .as_ref()
                                    == Some(&target)
                            }
                        }
                });
                if !on_page {
                    continue;
                }
                if refs.len() == MAX_REFS {
                    return Ok(BoardRefs {
                        refs,
                        truncated: true,
                    });
                }
                refs.push(BoardRefHit {
                    project: id.clone(),
                    board: file.relative(),
                    title: board.title.clone(),
                    page: page.id.clone(),
                    page_title: page.title.clone(),
                });
            }
        }
    }
    Ok(BoardRefs {
        refs,
        truncated: false,
    })
}
