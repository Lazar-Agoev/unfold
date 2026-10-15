//! The Compile action as callers see it: a progress callback, and a ticket that resolves when
//! the compile commits, fails, or is cancelled (docs/engine.md#updates `compile`).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use serde::Serialize;
use unfold_engine::CancelToken;

use crate::error::{Result, WorkspaceError};

/// A progress report while compiling: `done` of `total` items of `stage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CompileProgress {
    /// The pipeline stage (`snapshot`, `extract`, ...).
    pub stage: &'static str,
    /// Items finished.
    pub done: u64,
    /// Items in the stage.
    pub total: u64,
}

/// Receives progress for one compile, from engine threads (a command streams it).
pub type ProgressFn = Box<dyn Fn(&CompileProgress) + Send + Sync>;

/// What a finished compile did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CompileOutcome {
    /// Indexed files.
    pub files: u32,
    /// Files added or changed.
    pub changed_files: u32,
    /// Files no longer indexed.
    pub removed_files: u32,
    /// Whether the apps, their entrances, or the states of their code changed.
    pub entrances_changed: bool,
    /// Whether every discovered file is indexed.
    pub complete: bool,
    /// Problems after the compile that make the project's state: graph and settings problems,
    /// not board health.
    pub problems: u32,
    /// The update's wall time, in milliseconds.
    pub duration_ms: u64,
}

/// A queued Compile on the engine thread.
pub(crate) struct CompileJob {
    pub(crate) cancel: CancelToken,
    pub(crate) progress: Option<ProgressFn>,
    pub(crate) reply: Sender<Result<CompileOutcome>>,
}

/// A running or queued Compile. Dropping it doesn't cancel the compile.
#[derive(Debug)]
pub struct CompileTicket {
    cancel: CancelToken,
    result: Receiver<Result<CompileOutcome>>,
}

impl CompileTicket {
    pub(crate) fn new(cancel: CancelToken, result: Receiver<Result<CompileOutcome>>) -> Self {
        Self { cancel, result }
    }

    /// Asks the compile to stop; it then resolves with [`WorkspaceError::Cancelled`], leaving
    /// the previous graph, unless its commit had already finished.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Waits for the compile to finish.
    pub fn wait(self) -> Result<CompileOutcome> {
        self.result.recv().unwrap_or(Err(WorkspaceError::Closed))
    }

    /// Waits at most `timeout`; `None` if the compile is still running.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<Result<CompileOutcome>> {
        match self.result.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(WorkspaceError::Closed)),
        }
    }
}
