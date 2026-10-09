//! `search`: definitions, entrances, and files by name across the workspace's projects
//! (contract §4.5, decisions U-30, D-9). It also serves Add's name search and quick open.
//!
//! One request is one fan-out in Rust: every project in "Look in" (all of them by default) is
//! searched in parallel, and the results are merged. In each project, definitions and entrances
//! come from the store's FTS5 index, which returns every item whose name, qualified name, or path
//! contains each word of the query; files come from the live project's file list under
//! discovery's ignore rules, or from a resting project's stored graph (its indexed files: no
//! disk walk without a watcher to keep it current). Every candidate is ranked with
//! `nucleo-matcher`'s fuzzy score (a match in the name counts most, then the qualified name, then
//! the path); merged results sort by score, then workspace order, then shorter name, then key, so
//! they are deterministic. Searching never wakes a project.

use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Instant;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_store::{CandidateKind, ReadPool, SearchCandidate};

use super::{FileSet, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::projects::{ProjectHandle, ProjectId};

/// Results per kind when the request doesn't say.
const DEFAULT_LIMIT: u32 = 50;
/// The most results per kind.
pub const MAX_LIMIT: u32 = 500;
/// How many matches per kind and project are counted (and, for definitions and entrances, how
/// many FTS5 candidates are ranked).
const CANDIDATES: usize = 2_000;

/// A result kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    /// Definitions (functions, methods, classes, …).
    Definitions,
    /// Entrances.
    Entrances,
    /// Files.
    Files,
}

/// The arguments of `search`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchArgs {
    /// What was typed. Words are matched separately.
    pub query: String,
    /// The kinds to search (all when absent).
    #[serde(default)]
    #[ts(optional)]
    pub kinds: Option<Vec<SearchKind>>,
    /// Results per kind across every project (50 when absent, at most 500).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// The projects to look in (every project when absent).
    #[serde(default)]
    #[ts(optional)]
    pub projects: Option<Vec<ProjectId>>,
}

/// A definition or entrance result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// The project it is in.
    pub project: ProjectId,
    /// The key (open its card with it).
    pub key: String,
    /// The name (a definition's label; an entrance's label, trigger, or first target).
    pub name: String,
    /// A definition's qualified name; an entrance's method, trigger, and targets.
    pub qualified_name: String,
    /// The file (an entrance's evidence file).
    pub path: String,
    /// The block kind or entrance kind.
    pub kind: String,
    /// A definition's first line.
    pub line: Option<u32>,
    /// The rank score (higher is better).
    pub score: u32,
    /// UTF-16 offsets of the matched characters in `name`, for highlighting.
    pub name_matches: Vec<u32>,
}

/// A file result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// The project it is in.
    pub project: ProjectId,
    /// The path relative to the project's folder.
    pub path: String,
    /// The file name.
    pub name: String,
    /// The rank score (higher is better).
    pub score: u32,
    /// UTF-16 offsets of the matched characters in `path`, for highlighting.
    pub matches: Vec<u32>,
}

/// One kind's results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchGroup<T> {
    /// The best results across the projects, best first.
    pub hits: Vec<T>,
    /// How many matched in all (counting at most 2,000 per project).
    pub total: u32,
    /// More matched than `hits` holds.
    pub truncated: bool,
}

impl<T> Default for SearchGroup<T> {
    fn default() -> Self {
        Self {
            hits: Vec::new(),
            total: 0,
            truncated: false,
        }
    }
}

/// One project's match counts (at most 2,000 per kind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMatches {
    /// The project.
    pub project: ProjectId,
    /// Definitions that matched.
    pub definitions: u32,
    /// Entrances that matched.
    pub entrances: u32,
    /// Files that matched.
    pub files: u32,
}

/// Search results by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    /// The query searched.
    pub query: String,
    /// Definitions.
    pub definitions: SearchGroup<SearchHit>,
    /// Entrances.
    pub entrances: SearchGroup<SearchHit>,
    /// Files.
    pub files: SearchGroup<FileHit>,
    /// Each searched project's match counts, in workspace order.
    pub projects: Vec<ProjectMatches>,
    /// How long the search took, in milliseconds.
    pub took_ms: u32,
}

/// One kind's results in one project: its best hits, and how many matched.
struct Found<T> {
    hits: Vec<T>,
    total: usize,
    capped: bool,
}

impl<T> Default for Found<T> {
    fn default() -> Self {
        Self {
            hits: Vec::new(),
            total: 0,
            capped: false,
        }
    }
}

/// One project's results.
#[derive(Default)]
struct ProjectFound {
    definitions: Found<SearchHit>,
    entrances: Found<SearchHit>,
    files: Found<FileHit>,
    readers: Option<ReadPool>,
}

/// `search`: ranked definitions, entrances, and files across the projects. Definitions and
/// entrances need a compiled project; a live project's files don't.
pub fn search(ctx: &Ctx<'_>, args: SearchArgs) -> CommandResult<SearchResults> {
    let started = Instant::now();
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let wants = |kind: SearchKind| args.kinds.as_ref().is_none_or(|kinds| kinds.contains(&kind));
    let wanted = (
        wants(SearchKind::Definitions),
        wants(SearchKind::Entrances),
        wants(SearchKind::Files),
    );
    let query = args.query.trim().to_owned();
    let projects = look_in(ctx, args.projects.as_deref())?;
    if query.is_empty() {
        return Ok(SearchResults {
            query,
            definitions: SearchGroup::default(),
            entrances: SearchGroup::default(),
            files: SearchGroup::default(),
            projects: Vec::new(),
            took_ms: 0,
        });
    }
    let found: Vec<ProjectFound> = projects
        .par_iter()
        .map(|project| search_project(ctx, project, &query, limit, wanted))
        .collect::<CommandResult<_>>()?;
    ctx.check_cancelled()?;
    let totals = projects
        .iter()
        .zip(&found)
        .map(|(project, found)| ProjectMatches {
            project: project.id().clone(),
            definitions: count(found.definitions.total),
            entrances: count(found.entrances.total),
            files: count(found.files.total),
        })
        .collect();
    let position = |id: &ProjectId| projects.iter().position(|project| project.id() == id);
    let order_hits = |a: &SearchHit, b: &SearchHit| {
        b.score
            .cmp(&a.score)
            .then_with(|| position(&a.project).cmp(&position(&b.project)))
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.key.cmp(&b.key))
    };
    let mut definitions = merge(&found, |found| &found.definitions, limit, order_hits);
    // Lines only for the hits returned, with one reader per project.
    for (project, found) in projects.iter().zip(&found) {
        let Some(pool) = &found.readers else {
            continue;
        };
        if !definitions.hits.iter().any(|hit| hit.project == *project.id()) {
            continue;
        }
        let reader = pool.get()?;
        for hit in definitions
            .hits
            .iter_mut()
            .filter(|hit| hit.project == *project.id())
        {
            hit.line = reader.block(&hit.key)?.map(|located| located.block.lines.start);
        }
    }
    let entrances = merge(&found, |found| &found.entrances, limit, order_hits);
    let files = merge(
        &found,
        |found| &found.files,
        limit,
        |a: &FileHit, b: &FileHit| {
            b.score
                .cmp(&a.score)
                .then_with(|| position(&a.project).cmp(&position(&b.project)))
                .then_with(|| a.path.len().cmp(&b.path.len()))
                .then_with(|| a.path.cmp(&b.path))
        },
    );
    Ok(SearchResults {
        query,
        definitions,
        entrances,
        files,
        projects: totals,
        took_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    })
}

/// The projects to look in, in workspace order: every project, or those `only` names (each must
/// exist).
pub(crate) fn look_in<'a>(
    ctx: &'a Ctx<'_>,
    only: Option<&[ProjectId]>,
) -> CommandResult<Vec<ProjectHandle<'a>>> {
    if let Some(only) = only {
        for id in only {
            ctx.project(id)?;
        }
    }
    let mut handles = Vec::new();
    for id in ctx.projects().ids() {
        if only.is_none_or(|only| only.contains(&id)) {
            // A project removed since the list was read is simply not searched.
            if let Ok(handle) = ctx.project(&id) {
                handles.push(handle);
            }
        }
    }
    Ok(handles)
}

/// Merges one kind's per-project results: the `limit` best by `order`, and the totals.
fn merge<T: Clone>(
    found: &[ProjectFound],
    kind: impl Fn(&ProjectFound) -> &Found<T>,
    limit: usize,
    order: impl Fn(&T, &T) -> Ordering,
) -> SearchGroup<T> {
    let mut hits: Vec<T> = found
        .iter()
        .flat_map(|found| kind(found).hits.iter().cloned())
        .collect();
    hits.sort_by(&order);
    hits.truncate(limit);
    let total: usize = found.iter().map(|found| kind(found).total).sum();
    let capped = found.iter().any(|found| kind(found).capped);
    SearchGroup {
        truncated: capped || hits.len() < total,
        total: count(total),
        hits,
    }
}

/// Searches one project: its stored graph for definitions and entrances, and its files.
fn search_project(
    ctx: &Ctx<'_>,
    project: &ProjectHandle<'_>,
    query: &str,
    limit: usize,
    (definitions, entrances, files): (bool, bool, bool),
) -> CommandResult<ProjectFound> {
    ctx.check_cancelled()?;
    let pattern = Pattern::new(query, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let readers = project.readers()?;
    let mut found = ProjectFound::default();
    if let Some(pool) = &readers
        && (definitions || entrances)
    {
        let reader = pool.get()?;
        for (wanted, kind, slot) in [
            (definitions, CandidateKind::Definition, &mut found.definitions),
            (entrances, CandidateKind::Entrance, &mut found.entrances),
        ] {
            if wanted {
                let candidates = reader.search(query, kind, CANDIDATES)?;
                slot.total = candidates.len();
                slot.capped = candidates.len() >= CANDIDATES;
                slot.hits = rank(project.id(), &pattern, &mut matcher, candidates, limit);
            }
            ctx.check_cancelled()?;
        }
    }
    if files {
        let paths = file_list(project, readers.as_ref())?;
        let mut path_matcher = Matcher::new(Config::DEFAULT.match_paths());
        found.files = rank_files(project.id(), &pattern, &mut path_matcher, &paths.paths, limit);
        // A folder with more files than the list holds wasn't searched whole.
        found.files.capped |= paths.truncated;
    }
    found.readers = readers;
    Ok(found)
}

/// The files search and text search look at: a live project's files under discovery's ignore
/// rules, or a resting project's indexed files from its stored graph (none without a graph).
pub(crate) fn file_list(
    project: &ProjectHandle<'_>,
    readers: Option<&ReadPool>,
) -> CommandResult<Arc<FileSet>> {
    if let Some(workspace) = project.live() {
        return project
            .extras()
            .file_set(workspace.root(), &workspace.options().engine.discover);
    }
    let paths = match readers {
        Some(pool) => pool
            .get()?
            .files()?
            .into_iter()
            .map(|row| row.path.as_str().into())
            .collect(),
        None => Vec::new(),
    };
    Ok(Arc::new(FileSet {
        paths,
        truncated: false,
    }))
}

/// The matched characters of `haystack` as UTF-16 offsets, with the score. `chars` is a
/// reusable buffer.
fn score_with_matches(
    pattern: &Pattern,
    matcher: &mut Matcher,
    haystack: &str,
    chars: &mut Vec<char>,
) -> Option<(u32, Vec<u32>)> {
    let mut indices = Vec::new();
    let score = pattern.indices(Utf32Str::new(haystack, chars), matcher, &mut indices)?;
    indices.sort_unstable();
    indices.dedup();
    // Indices count characters; convert them to UTF-16 offsets.
    let matches = if haystack.is_ascii() {
        indices
    } else {
        let mut offsets = Vec::with_capacity(indices.len());
        let mut wanted = indices.iter().peekable();
        let mut unit = 0u32;
        for (index, character) in haystack.chars().enumerate() {
            if wanted.peek().is_some_and(|&&next| next as usize == index) {
                offsets.push(unit);
                wanted.next();
            }
            unit += count(character.len_utf16());
        }
        offsets
    };
    Some((score, matches))
}

/// Ranks store candidates: a name match counts four times, a qualified-name match twice, a path
/// match once. Every candidate is scored, but only the `limit` best are turned into hits (with
/// their match positions and owned strings).
fn rank(
    project: &ProjectId,
    pattern: &Pattern,
    matcher: &mut Matcher,
    candidates: Vec<SearchCandidate>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut chars = Vec::new();
    let mut score = |text: &str| pattern.score(Utf32Str::new(text, &mut chars), matcher);
    let mut ranked: Vec<(u32, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let named = score(&candidate.name).map_or(0, |score| score * 4);
            let qualified = score(&candidate.qualified_name).map_or(0, |score| score * 2);
            let path = score(&candidate.path).unwrap_or(0);
            (named.max(qualified).max(path), index)
        })
        .collect();
    ranked.sort_by(|&(a_score, a), &(b_score, b)| {
        let (a, b) = (&candidates[a], &candidates[b]);
        b_score
            .cmp(&a_score)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.key.as_str().cmp(b.key.as_str()))
    });
    ranked.truncate(limit);
    let mut candidates: Vec<Option<SearchCandidate>> = candidates.into_iter().map(Some).collect();
    ranked
        .into_iter()
        .filter_map(|(score, index)| {
            let candidate = candidates[index].take()?;
            let name_matches = score_with_matches(pattern, matcher, &candidate.name, &mut chars)
                .map_or_else(Vec::new, |(_, matches)| matches);
            Some(SearchHit {
                project: project.clone(),
                key: candidate.key.as_str().to_owned(),
                name: candidate.name.to_string(),
                qualified_name: candidate.qualified_name.to_string(),
                path: candidate.path.to_string(),
                kind: candidate.item_kind.to_string(),
                line: None,
                score,
                name_matches,
            })
        })
        .collect()
}

/// Ranks file paths by fuzzy score, then shorter path, then path: the `limit` best, and how
/// many matched (counting at most [`CANDIDATES`]).
fn rank_files(
    project: &ProjectId,
    pattern: &Pattern,
    matcher: &mut Matcher,
    paths: &[Box<str>],
    limit: usize,
) -> Found<FileHit> {
    let mut chars = Vec::new();
    let mut scored: Vec<(u32, &str)> = paths
        .iter()
        .filter_map(|path| {
            pattern
                .score(Utf32Str::new(path, &mut chars), matcher)
                .map(|score| (score, path.as_ref()))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.len().cmp(&b.1.len()))
            .then_with(|| a.1.cmp(b.1))
    });
    let matched = scored.len();
    scored.truncate(limit);
    let hits = scored
        .into_iter()
        .map(|(score, path)| FileHit {
            project: project.clone(),
            path: path.to_owned(),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            score,
            matches: score_with_matches(pattern, matcher, path, &mut chars)
                .map_or_else(Vec::new, |(_, matches)| matches),
        })
        .collect();
    Found {
        hits,
        total: matched.min(CANDIDATES),
        capped: matched > CANDIDATES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_rank_by_fuzzy_score_then_length() {
        let pattern = Pattern::new(
            "ordsvc",
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let paths: Vec<Box<str>> = ["src/services/order-service.ts", "README.md", "src/orders/svc.ts"]
            .into_iter()
            .map(Into::into)
            .collect();
        let found = rank_files(
            &ProjectId::new_unchecked("p-aaaaaaaaaa"),
            &pattern,
            &mut matcher,
            &paths,
            10,
        );
        assert_eq!(found.total, 2);
        let hits = found.hits;
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|hit| !hit.matches.is_empty()));
        assert!(hits.iter().all(|hit| hit.path != "README.md"));
    }

    #[test]
    fn match_offsets_count_utf16_units() {
        let pattern = Pattern::new("b", CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let (_, matches) = score_with_matches(&pattern, &mut matcher, "😀b", &mut Vec::new()).expect("match");
        assert_eq!(matches, [2]);
    }
}
