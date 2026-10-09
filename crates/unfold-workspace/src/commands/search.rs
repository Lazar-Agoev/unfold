//! `search`: definitions, entrances, and files by name (architecture §11.3 Search, decision
//! U-30). It also serves quick open: ⌘P asks for files, ⌘T for definitions.
//!
//! Definitions and entrances come from the store's FTS5 index, which returns every item whose
//! name, qualified name, or path contains each word of the query; files come from the folder's
//! file list under discovery's ignore rules, matched on their path. Every candidate is then
//! ranked with `nucleo-matcher`'s fuzzy score (a match in the name counts most, then the
//! qualified name, then the path), ties broken by shorter name and then key, so results are
//! deterministic.

use std::time::Instant;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_store::{CandidateKind, SearchCandidate};

use super::count;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;

/// Results per group when the request doesn't say.
const DEFAULT_LIMIT: u32 = 50;
/// The most results per group.
pub const MAX_LIMIT: u32 = 500;
/// How many FTS5 candidates per kind are ranked.
const CANDIDATES: usize = 2_000;

/// A result group.
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
    /// The groups to search (all when absent): `["files"]` for ⌘P, `["definitions"]` for ⌘T.
    #[serde(default)]
    #[ts(optional)]
    pub kinds: Option<Vec<SearchKind>>,
    /// Results per group (50 when absent, at most 500).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// A definition or entrance result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
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
    /// The folder-relative path.
    pub path: String,
    /// The file name.
    pub name: String,
    /// The rank score (higher is better).
    pub score: u32,
    /// UTF-16 offsets of the matched characters in `path`, for highlighting.
    pub matches: Vec<u32>,
}

/// Search results, grouped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    /// The query searched.
    pub query: String,
    /// Definitions, best first.
    pub definitions: Vec<SearchHit>,
    /// Entrances, best first.
    pub entrances: Vec<SearchHit>,
    /// Files, best first.
    pub files: Vec<FileHit>,
    /// The folder has more files than the file list holds; some weren't searched.
    pub files_truncated: bool,
    /// How long the search took, in milliseconds.
    pub took_ms: u32,
}

/// `search`: ranked definitions, entrances, and files. Definitions and entrances need a
/// compiled folder; files don't.
pub fn search(ctx: &Ctx<'_>, args: SearchArgs) -> CommandResult<SearchResults> {
    let started = Instant::now();
    let workspace = ctx.workspace()?;
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let wants = |kind: SearchKind| args.kinds.as_ref().is_none_or(|kinds| kinds.contains(&kind));
    let query = args.query.trim();
    let mut results = SearchResults {
        query: query.to_owned(),
        definitions: Vec::new(),
        entrances: Vec::new(),
        files: Vec::new(),
        files_truncated: false,
        took_ms: 0,
    };
    if query.is_empty() {
        return Ok(results);
    }
    let pattern = Pattern::new(query, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
    let mut matcher = Matcher::new(Config::DEFAULT);
    if (wants(SearchKind::Definitions) || wants(SearchKind::Entrances))
        && let Ok(pool) = workspace.readers()
    {
        let reader = pool.get()?;
        if wants(SearchKind::Definitions) {
            let candidates = reader.search(query, CandidateKind::Definition, CANDIDATES)?;
            results.definitions = rank(&pattern, &mut matcher, candidates, limit);
            for hit in &mut results.definitions {
                hit.line = reader.block(&hit.key)?.map(|located| located.block.lines.start);
            }
        }
        ctx.check_cancelled()?;
        if wants(SearchKind::Entrances) {
            let candidates = reader.search(query, CandidateKind::Entrance, CANDIDATES)?;
            results.entrances = rank(&pattern, &mut matcher, candidates, limit);
        }
    }
    ctx.check_cancelled()?;
    if wants(SearchKind::Files) {
        let files = ctx.extras(&workspace).file_set(&workspace)?;
        results.files_truncated = files.truncated;
        let mut path_matcher = Matcher::new(Config::DEFAULT.match_paths());
        results.files = rank_files(&pattern, &mut path_matcher, &files.paths, limit);
    }
    results.took_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    Ok(results)
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

/// Ranks file paths by fuzzy score, then shorter path, then path.
fn rank_files(pattern: &Pattern, matcher: &mut Matcher, paths: &[Box<str>], limit: usize) -> Vec<FileHit> {
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
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|(score, path)| FileHit {
            path: path.to_owned(),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            score,
            matches: score_with_matches(pattern, matcher, path, &mut chars)
                .map_or_else(Vec::new, |(_, matches)| matches),
        })
        .collect()
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
        let hits = rank_files(&pattern, &mut matcher, &paths, 10);
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
