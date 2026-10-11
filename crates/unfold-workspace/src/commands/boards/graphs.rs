//! The graphs a board reads: its own project's, and one per other project its
//! cards name, resolved by shared name (decision U-40) and read from that project's stored graph
//! without waking it (the live pool, or a resting project's read-only pool). A name that
//! resolves to no project, or to one with no usable graph, is **not connected**: its cards are
//! placeholders.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use unfold_reader::board::files::Revision;
use unfold_reader::board::{Board, BoardGraph, PageGraphs, ProjectName, References};
use unfold_store::ReadPool;

use crate::ipc::error::CommandResult;
use crate::projects::{ProjectHandle, ProjectId, Projects};
use crate::workspace::Workspace;

/// Which commit of a project's graph something was computed from: the commit time the store
/// records with every commit (`None`: no graph).
pub(super) type Stamp = Option<i64>;

/// What a board's health depends on: its working file's revision, its own project's graph, and
/// the projects its cards' names resolve to with their graphs. A cached health is current while
/// its key is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HealthKey {
    /// The working file's revision.
    pub(super) revision: Revision,
    own: Stamp,
    /// By name: the project it resolves to and its graph's stamp, or `None` when it resolves
    /// to no project.
    others: BTreeMap<ProjectName, Option<(ProjectId, Stamp)>>,
}

impl HealthKey {
    /// Whether the health depends on project `project`'s graph.
    pub(super) fn references(&self, project: &ProjectId) -> bool {
        self.others
            .values()
            .any(|resolved| resolved.as_ref().is_some_and(|(id, _)| id == project))
    }
}

/// One project's stored graph.
#[derive(Debug, Clone)]
struct Source {
    id: ProjectId,
    root: PathBuf,
    pool: Option<ReadPool>,
    stamp: Stamp,
}

impl Source {
    fn new(id: ProjectId, root: PathBuf, pool: Option<ReadPool>) -> CommandResult<Self> {
        let stamp = match &pool {
            Some(pool) => pool.get()?.meta()?.graph.map(|graph| graph.compiled_at_ms),
            None => None,
        };
        Ok(Self {
            id,
            root,
            pool,
            stamp,
        })
    }

    /// The part of the graph `references` need (empty without a graph).
    fn load(&self, references: &References<'_>) -> CommandResult<BoardGraph> {
        match &self.pool {
            Some(pool) => Ok(BoardGraph::load(&pool.get()?, &self.root, references)?),
            None => Ok(BoardGraph::new()),
        }
    }
}

/// The graphs of one page or board: its own project's and its connected projects'.
#[derive(Debug, Default)]
pub(super) struct Graphs {
    own: BoardGraph,
    others: BTreeMap<ProjectName, BoardGraph>,
}

impl Graphs {
    /// The graphs as the reader takes them.
    pub(super) fn page(&self) -> PageGraphs<'_> {
        PageGraphs::new(&self.own, &self.others)
    }
}

/// Resolves the projects of one project's boards, once per name per command.
pub(super) struct Resolver<'a> {
    projects: &'a Projects,
    own: Source,
    others: BTreeMap<ProjectName, Option<Source>>,
}

impl<'a> Resolver<'a> {
    /// For the boards of `project` (live or resting).
    pub(super) fn of(projects: &'a Projects, project: &ProjectHandle<'_>) -> CommandResult<Self> {
        let own = Source::new(
            project.id().clone(),
            project.root().to_path_buf(),
            project.readers()?,
        )?;
        Ok(Self::with(projects, own))
    }

    /// For the boards of the live project `workspace`.
    pub(super) fn live(projects: &'a Projects, workspace: &Workspace) -> CommandResult<Self> {
        let own = Source::new(
            workspace.project().clone(),
            workspace.root().to_path_buf(),
            workspace.readers().ok(),
        )?;
        Ok(Self::with(projects, own))
    }

    fn with(projects: &'a Projects, own: Source) -> Self {
        Self {
            projects,
            own,
            others: BTreeMap::new(),
        }
    }

    /// The project `name` resolves to, with its graph; `None` when it resolves to none.
    fn other(&mut self, name: &ProjectName) -> Option<&Source> {
        if !self.others.contains_key(name) {
            let source = self.resolve(name);
            self.others.insert(name.clone(), source);
        }
        self.others.get(name).and_then(Option::as_ref)
    }

    fn resolve(&self, name: &ProjectName) -> Option<Source> {
        let id = self.projects.resolve_name(name.as_str())?;
        if id == self.own.id {
            return Some(self.own.clone());
        }
        let handle = self.projects.handle(&id).ok()?;
        let root = handle.root().to_path_buf();
        // A graph that can't be read leaves the project's cards placeholders.
        let pool = handle.readers().ok().flatten();
        Source::new(id.clone(), root.clone(), pool)
            .ok()
            .or_else(|| Source::new(id, root, None).ok())
    }

    /// The graphs for what `references` lists per project (`None`: the board's own), for a
    /// board or page whose items name the projects `names`.
    pub(super) fn graphs<'k>(
        &mut self,
        names: &[ProjectName],
        references: impl Fn(Option<&ProjectName>) -> References<'k>,
    ) -> CommandResult<Graphs> {
        let mut graphs = Graphs {
            own: self.own.load(&references(None))?,
            others: BTreeMap::new(),
        };
        for name in names {
            let referenced = references(Some(name));
            if referenced == References::default() {
                continue;
            }
            let Some(source) = self.other(name).filter(|source| source.pool.is_some()) else {
                continue;
            };
            // Another project's graph that can't be read is not connected, never an error.
            if let Ok(graph) = source.load(&referenced) {
                graphs.others.insert(name.clone(), graph);
            }
        }
        Ok(graphs)
    }

    /// The graphs of every page of `board`.
    pub(super) fn board_graphs(&mut self, board: &Board) -> CommandResult<Graphs> {
        self.graphs(&board.projects, |project| board.references(project))
    }

    /// The key of `board`'s health, read from a file with `revision`; `None` while the board's
    /// own project has no graph (health needs a graph: before the first Compile every card would
    /// read as missing).
    pub(super) fn key(&mut self, board: &Board, revision: Revision) -> Option<HealthKey> {
        self.own.pool.as_ref()?;
        let others = board
            .projects
            .iter()
            .map(|name| {
                let resolved = self.other(name).map(|source| (source.id.clone(), source.stamp));
                (name.clone(), resolved)
            })
            .collect();
        Some(HealthKey {
            revision,
            own: self.own.stamp,
            others,
        })
    }

    /// The other projects `board`'s cards resolve to: an open board keeps them live.
    pub(super) fn references(&mut self, board: &Board) -> BTreeSet<ProjectId> {
        let own = self.own.id.clone();
        board
            .projects
            .iter()
            .filter_map(|name| self.other(name).map(|source| source.id.clone()))
            .filter(|id| *id != own)
            .collect()
    }
}
