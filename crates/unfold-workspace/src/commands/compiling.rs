//! Compile commands (docs/engine.md#updates): Compile with streamed progress, cancelling, and
//! Problems. A running compile keeps its project live.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use super::ProjectArgs;
use crate::compile::{CompileOutcome, CompileProgress};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::problems::ProblemItem;
use crate::shared::lock;

/// The most problems `problems` returns.
pub const MAX_PROBLEMS: usize = 5_000;

/// How often a running compile checks whether its request was cancelled.
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// `compile`: queues a Compile and streams its progress (`CompileProgress` messages) until it
/// commits, fails, or is cancelled (`cancel(requestId)` or `cancel_compile`). The first Compile
/// creates `.unfold/`. The project stays live until the compile settles.
pub fn compile(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<CompileOutcome> {
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
    let ticket = workspace.compile(Some(progress));
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

/// `problems`: the Problems list (problem reporting): files that don't parse or can't be read,
/// settings errors, discovery and input problems, a failed compile, and the health of every
/// board of the project (at most 5,000).
pub fn problems(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<Vec<ProblemItem>> {
    let mut problems = ctx.live(&args.project)?.problems();
    problems.truncate(MAX_PROBLEMS);
    Ok(problems)
}
