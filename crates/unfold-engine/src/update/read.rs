//! The snapshot stage: reading and hashing files once, in parallel, and the progress counter
//! the parallel stages share.

use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use unfold_core::text::SnapshotError;
use unfold_core::{RepoPath, Snapshot};

use super::{Progress, Stage};
use crate::cancel::{CancelToken, Cancelled};

/// One file read.
#[derive(Debug, Clone)]
pub(crate) enum Read {
    /// Its bytes, hashed and decoded.
    Snapshot(Snapshot),
    /// It disappeared since discovery.
    Vanished,
    /// It can't be read or isn't text Unfold can index.
    Unreadable(Box<str>),
}

/// Reads one folder-relative file.
pub(crate) fn read_file(root: &Path, path: &RepoPath) -> Read {
    match std::fs::read(root.join(path.as_str())) {
        Ok(bytes) => match Snapshot::new(path.clone(), bytes) {
            Ok(snapshot) => Read::Snapshot(snapshot),
            Err(SnapshotError::InvalidUtf8 { offset, .. }) => {
                Read::Unreadable(format!("invalid UTF-8 at byte {offset}").into())
            }
            Err(SnapshotError::TooLarge { len, .. }) => {
                Read::Unreadable(format!("the file is {len} bytes, more than the 4 GiB limit").into())
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => Read::Vanished,
        Err(error) => Read::Unreadable(format!("cannot read the file: {error}").into()),
    }
}

/// Reads `paths` in parallel, results in input order.
pub(crate) fn read_all(
    root: &Path,
    paths: &[&RepoPath],
    cancel: &CancelToken,
    counter: &Counter<'_>,
) -> Result<Vec<Read>, Cancelled> {
    paths
        .par_iter()
        .map(|path| {
            cancel.check()?;
            let read = read_file(root, path);
            counter.tick();
            Ok(read)
        })
        .collect()
}

/// Counts finished items of one stage and reports progress about a hundred times per stage.
pub(crate) struct Counter<'a> {
    stage: Stage,
    total: usize,
    step: usize,
    done: AtomicUsize,
    progress: Option<&'a (dyn Fn(Progress) + Sync)>,
}

impl<'a> Counter<'a> {
    /// A counter for `total` items; reports the start immediately.
    pub(crate) fn start(stage: Stage, total: usize, progress: Option<&'a (dyn Fn(Progress) + Sync)>) -> Self {
        if let Some(report) = progress {
            report(Progress {
                stage,
                done: 0,
                total,
            });
        }
        Self {
            stage,
            total,
            step: (total / 100).max(1),
            done: AtomicUsize::new(0),
            progress,
        }
    }

    /// One more item finished.
    pub(crate) fn tick(&self) {
        let Some(report) = self.progress else {
            return;
        };
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        if done.is_multiple_of(self.step) || done == self.total {
            report(Progress {
                stage: self.stage,
                done,
                total: self.total,
            });
        }
    }
}
