//! Compile commands (architecture §7.1, §11.11): Compile and Rebuild from scratch
//! with streamed progress, cancelling, the background-update switch, Problems, and the Index
//! log. A running compile keeps its project live.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ProjectArgs;
use crate::compile::{CompileMode, CompileOutcome, CompileProgress};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::problems::{IndexLogEntry, ProblemItem};
use crate::projects::ProjectId;
use crate::shared::lock;
use crate::status::GraphStatus;

/// The most problems `problems` returns.
pub const MAX_PROBLEMS: usize = 5_000;

/// The arguments of `compile`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CompileArgs {
    /// The project.
    pub project: ProjectId,
    /// `check` (Compile) or `from_scratch` (Rebuild from scratch).
    #[serde(default)]
    #[ts(optional)]
    pub mode: Option<CompileMode>,
}

/// How often a running compile checks whether its request was cancelled.
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// `compile`: queues a Compile and streams its progress (`CompileProgress` messages) until it
/// commits, fails, or is cancelled (`cancel(requestId)` or `cancel_compile`). The first Compile
/// creates `.unfold/`. The project stays live until the compile settles.
pub fn compile(ctx: &Ctx<'_>, args: CompileArgs) -> CommandResult<CompileOutcome> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let _pin = project.pin();
    let host = Arc::clone(ctx.host);
    let (window, request) = (ctx.window, ctx.request);
    let interval = workspace.options().progress_interval;
    // The engine reports every file; the stream sends a report per interval, plus every stage's
    // first and last, so a large compile doesn't flood the window.
    let last: Mutex<Option<(Instant, &'static str)>> = Mutex::new(None);
    let progress = Box::new(move |report: &CompileProgress| {
        let mut last = lock(&last);
        let due = report.done >= report.total
            || last.is_none_or(|(at, stage)| stage != report.stage || at.elapsed() >= interval);
        if due {
            *last = Some((Instant::now(), report.stage));
            drop(last);
            host.stream(window, request, report);
        }
    });
    let ticket = workspace.compile(args.mode.unwrap_or_default(), Some(progress));
    loop {
        if ctx.cancel.is_cancelled() {
            ticket.cancel();
        }
        if let Some(result) = ticket.wait_timeout(CANCEL_POLL) {
            return result.map_err(CommandError::from);
        }
    }
}

/// What `cancel_compile` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelCompileResult {
    /// Whether a Compile was running or queued.
    pub cancelled: bool,
}

/// `cancel_compile`: cancels the project's running Compile and any queued ones (their `compile`
/// requests reject with `cancelled`). Nothing to cancel while the project rests.
pub fn cancel_compile(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<CancelCompileResult> {
    Ok(CancelCompileResult {
        cancelled: ctx
            .project(&args.project)?
            .live()
            .is_some_and(|workspace| workspace.cancel_compile()),
    })
}

/// The arguments of `set_background_updates`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetBackgroundUpdatesArgs {
    /// The project.
    pub project: ProjectId,
    /// Whether the graph updates itself as files change.
    pub on: bool,
}

/// `set_background_updates`: turns background updates on or off for the project (remembered in
/// `state.json`) and returns the new status.
pub fn set_background_updates(ctx: &Ctx<'_>, args: SetBackgroundUpdatesArgs) -> CommandResult<GraphStatus> {
    let workspace = ctx.live(&args.project)?;
    workspace.set_background_updates(args.on)?;
    Ok(workspace.status())
}

/// `problems`: the Problems list (decision U-28): files that don't parse or can't be read,
/// settings errors, discovery and input problems, a failed compile, and the health of every
/// board of the project (at most 5,000).
pub fn problems(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<Vec<ProblemItem>> {
    let mut problems = ctx.live(&args.project)?.problems();
    problems.truncate(MAX_PROBLEMS);
    Ok(problems)
}

/// Counts of the committed graph, for the Index log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    /// Files whose facts come from the bytes on disk.
    pub files_current: u64,
    /// Files showing their last version that parsed.
    pub files_last_good: u64,
    /// Files that never parsed.
    pub files_failed: u64,
    /// Fingerprinted configuration inputs.
    pub inputs: u64,
    /// Blocks.
    pub blocks: u64,
    /// Resolved edges.
    pub edges_resolved: u64,
    /// External edges.
    pub edges_external: u64,
    /// Unresolved edges (facts about the code, not problems).
    pub edges_unresolved: u64,
    /// Entrances.
    pub entrances: u64,
    /// Definitions.
    pub definitions: u64,
    /// Applications.
    pub applications: u64,
    /// The graph database's size in bytes.
    pub database_bytes: u64,
}

/// The Index log panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IndexLog {
    /// One entry per committed update, oldest first.
    pub entries: Vec<IndexLogEntry>,
    /// Counts of the committed graph, once the project is compiled.
    pub stats: Option<GraphStats>,
}

/// `index_log`: the last updates' statistics and the graph's counts.
pub fn index_log(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<IndexLog> {
    let workspace = ctx.live(&args.project)?;
    let stats = match workspace.readers() {
        Ok(pool) => {
            let stats = pool.get()?.stats()?;
            Some(GraphStats {
                files_current: stats.files_current,
                files_last_good: stats.files_last_good,
                files_failed: stats.files_failed,
                inputs: stats.inputs,
                blocks: stats.blocks,
                edges_resolved: stats.edges_resolved,
                edges_external: stats.edges_external,
                edges_unresolved: stats.edges_unresolved,
                entrances: stats.entrances,
                definitions: stats.definitions,
                applications: stats.applications,
                database_bytes: stats.database_bytes,
            })
        }
        Err(_) => None,
    };
    Ok(IndexLog {
        entries: workspace.index_log(),
        stats,
    })
}
