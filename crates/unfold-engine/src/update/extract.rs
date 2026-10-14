//! The extract stage: per-file facts for changed files, from the cache when it has these exact
//! bytes and settings, otherwise from the language front end, in parallel.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use rayon::prelude::*;
use unfold_core::{RepoPath, Snapshot};

use super::cache::{CacheEntry, CacheStatus, decode_facts, decode_failure, encode_failure};
use super::read::Counter;
use crate::cancel::{CancelToken, Cancelled};
use crate::facts::FileFacts;
use crate::language::{ExtractionOptions, Language, SourceFailure};
use crate::settings::{Settings, SettingsHash};

/// One file to extract.
pub(crate) struct Job<'a> {
    pub(crate) path: &'a RepoPath,
    pub(crate) language: &'a dyn Language,
    pub(crate) snapshot: &'a Snapshot,
    /// The settings hash the facts are cached under.
    pub(crate) settings: SettingsHash,
    /// The file's extraction options (whose hash is `settings`).
    pub(crate) options: &'a ExtractionOptions,
}

/// What extraction produced for one file.
pub(crate) enum Extracted {
    /// The bytes parsed.
    Facts(Arc<FileFacts>),
    /// They didn't.
    Failure(SourceFailure),
}

/// One file's result, with the cache entry to put when the front end produced it.
pub(crate) struct JobResult {
    pub(crate) extracted: Extracted,
    /// `Some` when the front end ran and its result can be cached.
    pub(crate) fresh: Option<(CacheStatus, Vec<u8>)>,
    /// Whether the cache supplied the result.
    pub(crate) from_cache: bool,
}

/// Extracts `jobs` in parallel, with `cached[i]` the cache's entry for `jobs[i]`. Results are in
/// input order.
pub(crate) fn extract_all(
    jobs: &[Job<'_>],
    cached: Vec<Option<CacheEntry>>,
    settings: &Settings,
    cancel: &CancelToken,
    counter: &Counter<'_>,
) -> Result<Vec<JobResult>, Cancelled> {
    // Largest files first, one file per task: a single big file (TypeScript's `checker.ts` is
    // a sixth of its corpus) then overlaps the many small ones instead of starting last.
    // Results are put back in input order.
    let mut work: Vec<(usize, &Job<'_>, Option<CacheEntry>)> = jobs
        .iter()
        .zip(cached)
        .enumerate()
        .map(|(index, (job, cached))| (index, job, cached))
        .collect();
    work.sort_by_key(|(index, job, _)| (std::cmp::Reverse(job.snapshot.len()), *index));
    let mut results: Vec<(usize, JobResult)> = work
        .into_par_iter()
        .with_max_len(1)
        .map(|(index, job, cached)| {
            cancel.check()?;
            let result = extract_one(job, cached, settings);
            counter.tick();
            Ok((index, result))
        })
        .collect::<Result<_, Cancelled>>()?;
    results.sort_unstable_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

fn extract_one(job: &Job<'_>, cached: Option<CacheEntry>, settings: &Settings) -> JobResult {
    // An entry that doesn't decode, or whose payload the language can't read (written by
    // another build, or corrupted), is a miss.
    if let Some(entry) = cached {
        let extracted = match entry.status {
            CacheStatus::Parsed => decode_facts(&entry.bytes, job.language)
                .ok()
                .map(|facts| Extracted::Facts(Arc::new(facts))),
            CacheStatus::Failed => decode_failure(&entry.bytes).ok().map(Extracted::Failure),
        };
        if let Some(extracted) = extracted {
            return JobResult {
                extracted,
                fresh: None,
                from_cache: true,
            };
        }
    }
    // Front ends must never panic; if one does anyway, the file fails instead of the update,
    // and the failure is not cached so a fixed front end retries it.
    let result = catch_unwind(AssertUnwindSafe(|| {
        job.language.extract_with(job.snapshot, settings, job.options)
    }));
    let (extracted, fresh) = match result {
        Ok(Ok(facts)) => {
            let fresh = facts.to_bytes().ok().map(|bytes| (CacheStatus::Parsed, bytes));
            (Extracted::Facts(Arc::new(facts)), fresh)
        }
        Ok(Err(failure)) => {
            let fresh = encode_failure(&failure)
                .ok()
                .map(|bytes| (CacheStatus::Failed, bytes));
            (Extracted::Failure(failure), fresh)
        }
        Err(panic) => {
            let detail = panic
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown cause".to_owned());
            let failure = SourceFailure {
                message: format!("internal error in {}: {detail}", job.language.producer().name).into(),
                offset: None,
            };
            (Extracted::Failure(failure), None)
        }
    };
    JobResult {
        extracted,
        fresh,
        from_cache: false,
    }
}
