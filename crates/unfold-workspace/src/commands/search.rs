//! `search`: definitions, apps and entrances, and files by name across the workspace's projects
//! (names only). It serves Add's name search, Home's Choose code…, and the agents' `search`.
//! Definitions are the catalog's: classes, functions (inline ones by their labels, such as
//! "callback to ordersRouter.post"), types, and modules, each hit naming its node kind;
//! `definitionKinds` narrows them.
//!
//! One request is one fan-out in Rust: every project asked for (all of them by default) is
//! searched in parallel, and the results are merged. In each project, everything comes from the
//! store's FTS5 index over names, which returns every item whose name, qualified name, or path
//! contains each word of the query: so only a compiled project has results. Every candidate is
//! ranked with `nucleo-matcher`'s fuzzy score (a match in the name counts most, then the
//! qualified name, then the path); merged results sort by score, then workspace order, then
//! shorter name, then key, so they are deterministic. Only the returned hits get details from the
//! stored graph (definitions' lines, entrances' apps, files' definition counts), with one reader
//! per project. Searching never wakes a project.

use std::cmp::Ordering;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::NodeKind;
use unfold_core::{Key, RepoPath};
use unfold_reader::catalog;
use unfold_store::{CandidateKind, ReadPool, Reader, SearchCandidate};

use super::count;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::projects::{ProjectHandle, ProjectId};

/// Results per kind when the request doesn't say.
const DEFAULT_LIMIT: u32 = 50;
/// The most results per kind.
pub const MAX_LIMIT: u32 = 500;
/// How many matches per kind and project are counted: how many FTS5 candidates are ranked.
const CANDIDATES: usize = 2_000;

/// A result kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    /// Definitions: classes, functions, types, and modules.
    Definitions,
    /// Apps and entrances.
    Entrances,
    /// Analyzed files.
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
    /// Only definitions of these kinds: `module`, `class`, `function`, `type` (all but `module`
    /// when absent: `files` finds modules by path).
    #[serde(default)]
    #[ts(optional)]
    pub definition_kinds: Option<Vec<NodeKind>>,
    /// Results per kind across every project (50 when absent, at most 500).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// The projects to look in (every project when absent).
    #[serde(default)]
    #[ts(optional)]
    pub projects: Option<Vec<ProjectId>>,
}

/// A definition, app, or entrance result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// The project it is in.
    pub project: ProjectId,
    /// The key (open its card with it).
    pub key: Key,
    /// The name: a definition's label, an app's or entrance's name.
    pub name: String,
    /// A definition's qualified name; an app's `run`; an entrance's app name and `on`.
    pub qualified_name: String,
    /// A definition's file; empty for an app or entrance.
    pub path: String,
    /// A definition's node kind (`module`, `class`, `function`, `type`), `app`, or `entrance`.
    pub kind: String,
    /// A definition's first line.
    pub line: Option<u32>,
    /// An entrance's app, by name.
    pub app: Option<String>,
    /// The rank score (higher is better).
    pub score: u32,
}

/// A file result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// The project it is in.
    pub project: ProjectId,
    /// The path relative to the project's folder.
    pub path: RepoPath,
    /// The file name.
    pub name: String,
    /// Its module's key.
    pub key: Key,
    /// How many definitions the stored graph holds in it.
    pub definitions: Option<u32>,
    /// The rank score (higher is better).
    pub score: u32,
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

/// Search results by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    /// The query searched.
    pub query: String,
    /// Definitions.
    pub definitions: SearchGroup<SearchHit>,
    /// Apps and entrances.
    pub entrances: SearchGroup<SearchHit>,
    /// Files.
    pub files: SearchGroup<FileHit>,
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

/// `search`: ranked definitions, apps and entrances, and files across the compiled projects.
pub fn search(ctx: &Ctx<'_>, args: SearchArgs) -> CommandResult<SearchResults> {
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let wants = |kind: SearchKind| args.kinds.as_ref().is_none_or(|kinds| kinds.contains(&kind));
    let wanted = (
        wants(SearchKind::Definitions),
        wants(SearchKind::Entrances),
        wants(SearchKind::Files),
    );
    let query = args.query.trim().to_owned();
    let projects = look_in(ctx, args.projects.as_deref())?;
    let definition_kinds: Vec<&str> = match &args.definition_kinds {
        Some(kinds) => kinds.iter().map(|kind| kind.as_str()).collect(),
        None => NodeKind::ALL
            .iter()
            .filter(|kind| kind.is_definition() && **kind != NodeKind::Module)
            .map(|kind| kind.as_str())
            .collect(),
    };
    if query.is_empty() {
        return Ok(SearchResults {
            query,
            definitions: SearchGroup::default(),
            entrances: SearchGroup::default(),
            files: SearchGroup::default(),
        });
    }
    let found: Vec<ProjectFound> = projects
        .par_iter()
        .map(|project| search_project(ctx, project, &query, &definition_kinds, limit, wanted))
        .collect::<CommandResult<_>>()?;
    ctx.check_cancelled()?;
    let position = |id: &ProjectId| projects.iter().position(|project| project.id() == id);
    let order_hits = |a: &SearchHit, b: &SearchHit| {
        b.score
            .cmp(&a.score)
            .then_with(|| position(&a.project).cmp(&position(&b.project)))
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.key.cmp(&b.key))
    };
    let mut definitions = merge(&found, |found| &found.definitions, limit, order_hits);
    let mut entrances = merge(&found, |found| &found.entrances, limit, order_hits);
    let mut files = merge(
        &found,
        |found| &found.files,
        limit,
        |a: &FileHit, b: &FileHit| {
            b.score
                .cmp(&a.score)
                .then_with(|| position(&a.project).cmp(&position(&b.project)))
                .then_with(|| a.path.as_str().len().cmp(&b.path.as_str().len()))
                .then_with(|| a.path.cmp(&b.path))
        },
    );
    // Details only for the hits returned, with one reader per project.
    for (project, found) in projects.iter().zip(&found) {
        if let Some(pool) = &found.readers {
            detail(
                project.id(),
                pool,
                &mut definitions.hits,
                &mut entrances.hits,
                &mut files.hits,
            )?;
        }
    }
    Ok(SearchResults {
        query,
        definitions,
        entrances,
        files,
    })
}

/// Fills in one project's returned hits from its stored graph: definitions' lines, entrances'
/// apps, and files' definition counts (one grouped query).
fn detail(
    project: &ProjectId,
    pool: &ReadPool,
    definitions: &mut [SearchHit],
    entrances: &mut [SearchHit],
    files: &mut [FileHit],
) -> CommandResult<()> {
    let ours = |hit_project: &ProjectId| hit_project == project;
    if !definitions.iter().any(|hit| ours(&hit.project))
        && !entrances.iter().any(|hit| ours(&hit.project))
        && !files.iter().any(|hit| ours(&hit.project))
    {
        return Ok(());
    }
    let reader: Reader = pool.get()?;
    for hit in definitions.iter_mut().filter(|hit| ours(&hit.project)) {
        hit.line = reader
            .node(hit.key.as_str())?
            .map(|located| located.node.lines.start);
    }
    if entrances.iter().any(|hit| ours(&hit.project)) {
        let apps = reader.apps()?;
        for hit in entrances.iter_mut().filter(|hit| ours(&hit.project)) {
            hit.app = catalog::start(&apps, hit.key.as_str())
                .filter(|start| start.key() != &start.app().key)
                .map(|start| start.app().name.to_string());
        }
    }
    let paths: Vec<&str> = files
        .iter()
        .filter(|hit| ours(&hit.project))
        .map(|hit| hit.path.as_str())
        .collect();
    let counts: FxHashMap<RepoPath, u32> = reader.definition_counts(&paths)?.into_iter().collect();
    for hit in files.iter_mut().filter(|hit| ours(&hit.project)) {
        hit.definitions = counts.get(&hit.path).copied();
    }
    Ok(())
}

/// The projects to look in, in workspace order: every project, or those `only` names (each must
/// exist).
fn look_in<'a>(ctx: &'a Ctx<'_>, only: Option<&[ProjectId]>) -> CommandResult<Vec<ProjectHandle<'a>>> {
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

/// Searches one project's stored graph: definitions (of `definition_kinds`), apps and
/// entrances, and files.
fn search_project(
    ctx: &Ctx<'_>,
    project: &ProjectHandle<'_>,
    query: &str,
    definition_kinds: &[&str],
    limit: usize,
    (definitions, entrances, files): (bool, bool, bool),
) -> CommandResult<ProjectFound> {
    ctx.check_cancelled()?;
    let pattern = Pattern::new(query, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let readers = project.readers()?;
    let mut found = ProjectFound::default();
    if let Some(pool) = &readers {
        let reader = pool.get()?;
        if definitions {
            let candidates = reader.search(query, CandidateKind::Definition, definition_kinds, CANDIDATES)?;
            found.definitions = rank(project.id(), &pattern, &mut matcher, candidates, limit);
            ctx.check_cancelled()?;
        }
        if entrances {
            let mut candidates = reader.search(query, CandidateKind::App, &[], CANDIDATES)?;
            candidates.extend(reader.search(query, CandidateKind::Entrance, &[], CANDIDATES)?);
            found.entrances = rank(project.id(), &pattern, &mut matcher, candidates, limit);
            ctx.check_cancelled()?;
        }
        if files {
            let candidates = reader.search(query, CandidateKind::File, &[], CANDIDATES)?;
            let mut path_matcher = Matcher::new(Config::DEFAULT.match_paths());
            found.files = rank_files(project.id(), &pattern, &mut path_matcher, candidates, limit);
        }
    }
    found.readers = readers;
    Ok(found)
}

/// Ranks store candidates: a name match counts four times, a qualified-name match twice, a path
/// match once. Every candidate is scored, but only the `limit` best are turned into hits (with
/// owned strings).
fn rank(
    project: &ProjectId,
    pattern: &Pattern,
    matcher: &mut Matcher,
    candidates: Vec<SearchCandidate>,
    limit: usize,
) -> Found<SearchHit> {
    let total = candidates.len();
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
            .then_with(|| a.key.cmp(&b.key))
    });
    ranked.truncate(limit);
    let mut candidates: Vec<Option<SearchCandidate>> = candidates.into_iter().map(Some).collect();
    let hits = ranked
        .into_iter()
        .filter_map(|(score, index)| {
            let candidate = candidates[index].take()?;
            Some(SearchHit {
                project: project.clone(),
                key: candidate.key,
                name: candidate.name.into(),
                qualified_name: candidate.qualified_name.into(),
                path: candidate.path.into(),
                kind: candidate.item_kind.into(),
                line: None,
                app: None,
                score,
            })
        })
        .collect();
    Found {
        hits,
        total,
        capped: total >= CANDIDATES,
    }
}

/// Ranks file candidates by fuzzy score on their paths, then shorter path, then path: the
/// `limit` best, and how many matched.
fn rank_files(
    project: &ProjectId,
    pattern: &Pattern,
    matcher: &mut Matcher,
    candidates: Vec<SearchCandidate>,
    limit: usize,
) -> Found<FileHit> {
    let total = candidates.len();
    let mut chars = Vec::new();
    let mut scored: Vec<(u32, SearchCandidate)> = candidates
        .into_iter()
        .map(|candidate| {
            let score = pattern
                .score(Utf32Str::new(&candidate.path, &mut chars), matcher)
                .unwrap_or(0);
            (score, candidate)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.path.len().cmp(&b.1.path.len()))
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    scored.truncate(limit);
    let hits = scored
        .into_iter()
        .filter_map(|(score, candidate)| {
            Some(FileHit {
                project: project.clone(),
                path: RepoPath::new(&candidate.path).ok()?,
                name: candidate.name.into(),
                key: candidate.key,
                definitions: None,
                score,
            })
        })
        .collect();
    Found {
        hits,
        total,
        capped: total >= CANDIDATES,
    }
}
