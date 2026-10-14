//! Home's project summary: the state line ("Graph current · 214 files · Python · updated just
//! now") and the Health line ("1 board needs review · All files parse · 1 entrance's code
//! not found").

use serde::Serialize;
use ts_rs::TS;
use unfold_core::vocab::{Language, TargetState};

use super::{ProjectArgs, board_hooks, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::problems::ProblemCategory;
use crate::projects::ProjectId;
use crate::status::{GraphStatus, ProjectState};

/// A board whose health has items (Home's Health line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardHealthCount {
    /// The board's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// How many health items it has.
    pub items: u32,
}

/// The most boards the Health line lists.
pub const MAX_HEALTH_BOARDS: usize = 20;

/// Home's Health line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealth {
    /// Files that don't parse or can't be read.
    pub doesnt_parse: u32,
    /// Boards whose health has items.
    pub boards_needing_review: u32,
    /// The boards with health items, most items first (at most 20).
    pub boards: Vec<BoardHealthCount>,
    /// The `code` keys of apps and entrances whose code isn't there: `missing`, `not_analyzed`,
    /// or `not_a_start`.
    pub entrances_not_found: u32,
}

/// Home's header, state line, and Health line for one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// The project.
    pub project: ProjectId,
    /// Its folder's name.
    pub name: String,
    /// The canonical path of its folder.
    pub path: String,
    /// Its row state.
    pub state: ProjectState,
    /// Its graph's status: the state line's state, and how the folder is watched.
    pub status: GraphStatus,
    /// When the graph was last committed, in milliseconds since the Unix epoch.
    pub updated_at_ms: Option<i64>,
    /// Indexed files.
    pub files: u32,
    /// The languages graphed, sorted.
    pub languages: Vec<Language>,
    /// The Health line.
    pub health: ProjectHealth,
}

/// `project_summary`: Home's header, state line, and Health line.
pub fn project_summary(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<ProjectSummary> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let (files, _, languages) = workspace.graph_counts();
    let (updated_at_ms, entrances_not_found) = match workspace.readers() {
        Ok(pool) => pool.get()?.snapshot(|reader| {
            let updated = reader.meta()?.graph.map(|graph| graph.compiled_at_ms);
            let apps = reader.apps()?;
            let not_found = apps
                .iter()
                .flat_map(|app| {
                    app.code
                        .iter()
                        .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
                })
                // A stale target still shows its last good block, and its file is counted
                // as one that doesn't parse.
                .filter(|target| {
                    matches!(
                        target.state,
                        TargetState::Missing | TargetState::NotAnalyzed | TargetState::NotAStart
                    )
                })
                .count();
            Ok((updated, not_found))
        })?,
        Err(_) => (None, 0),
    };
    let doesnt_parse = workspace
        .problems()
        .iter()
        .filter(|problem| {
            matches!(
                problem.category,
                ProblemCategory::DoesntParse | ProblemCategory::CantRead
            )
        })
        .count();
    let mut boards = board_hooks::health_rows(&project.extras());
    let boards_needing_review = count(boards.len());
    boards.truncate(MAX_HEALTH_BOARDS);
    Ok(ProjectSummary {
        project: args.project,
        name: project.name(),
        path: project.root().to_string_lossy().into_owned(),
        state: workspace.project_state(),
        status: workspace.status(),
        updated_at_ms,
        files,
        languages,
        health: ProjectHealth {
            doesnt_parse: count(doesnt_parse),
            boards_needing_review,
            boards,
            entrances_not_found: count(entrances_not_found),
        },
    })
}
