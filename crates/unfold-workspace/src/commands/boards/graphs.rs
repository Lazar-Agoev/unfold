//! The graph a board reads: its project's stored graph, read without waking the project (the
//! live pool, or a resting project's read-only pool), for what the board references.

use std::path::PathBuf;

use unfold_reader::board::files::Revision;
use unfold_reader::board::{Board, BoardGraph, References};
use unfold_store::ReadPool;

use crate::ipc::error::CommandResult;
use crate::workspace::Workspace;

/// What a board's health depends on: its file's revision and the commit of the project's graph
/// it was computed from (the commit time the store records with every commit). A cached health
/// is current while its key is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HealthKey {
    /// The board file's revision.
    pub(super) revision: Revision,
    stamp: i64,
}

/// A project's stored graph.
#[derive(Debug, Clone)]
pub(super) struct Source {
    root: PathBuf,
    pool: Option<ReadPool>,
    /// The graph's commit time; `None` without a graph.
    stamp: Option<i64>,
}

impl Source {
    /// The stored graph of the live project `workspace`.
    pub(super) fn live(workspace: &Workspace) -> CommandResult<Self> {
        let pool = workspace.readers().ok();
        let stamp = match &pool {
            Some(pool) => pool.get()?.meta()?.graph.map(|graph| graph.compiled_at_ms),
            None => None,
        };
        Ok(Self {
            root: workspace.root().to_path_buf(),
            pool,
            stamp,
        })
    }

    /// The part of the graph `references` need (empty without a graph).
    pub(super) fn load(&self, references: &References<'_>) -> CommandResult<BoardGraph> {
        match &self.pool {
            Some(pool) => Ok(BoardGraph::load(&pool.get()?, &self.root, references)?),
            None => Ok(BoardGraph::new()),
        }
    }

    /// The part of the graph every page of `board` needs.
    pub(super) fn board(&self, board: &Board) -> CommandResult<BoardGraph> {
        self.load(&board.references())
    }

    /// The key of the health of a board read from a file with `revision`; `None` until the
    /// project's first Compile commits a graph (health needs a graph: before it every card would
    /// read as missing).
    pub(super) fn key(&self, revision: Revision) -> Option<HealthKey> {
        Some(HealthKey {
            revision,
            stamp: self.stamp?,
        })
    }
}
