//! `search_text`: text matches across the workspace's projects with ripgrep's libraries
//! (`grep-searcher`, `grep-regex`), streamed file by file (contract §4.5, decisions U-30, D-9).
//!
//! It searches each project's file list: a live project's files under discovery's ignore rules
//! (never `.unfold/`), or a resting project's indexed files from its stored graph. It skips
//! binary files and files larger than [`MAX_FILE_BYTES`]. Projects are searched in workspace
//! order; within one, files are searched in parallel in chunks and streamed in path order, so
//! the stream is deterministic. It stops after [`MAX_MATCHES`] matches in all (the result says
//! there were more) and between files or lines when the request is cancelled (the UI cancels on
//! each new query). Searching never wakes a project.

use std::sync::atomic::{AtomicBool, Ordering};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::count;
use super::search::{file_list, look_in};
use crate::confine::confine;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// The most matches one search returns, across every project.
pub const MAX_MATCHES: usize = 2_000;
/// Files larger than this are not searched.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// A match line longer than this (in bytes) is shown as a window around its first match.
const MAX_PREVIEW_BYTES: usize = 400;
/// How much of the line before the first match a window keeps.
const PREVIEW_LEAD_BYTES: usize = 80;
/// Files searched in parallel before their results are streamed.
const CHUNK: usize = 64;

/// The arguments of `search_text`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextSearchArgs {
    /// The text, or a regular expression when `regex` is set.
    pub query: String,
    /// Treat `query` as a regular expression (Rust `regex` syntax).
    #[serde(default)]
    #[ts(optional)]
    pub regex: Option<bool>,
    /// Match case exactly.
    #[serde(default)]
    #[ts(optional)]
    pub case_sensitive: Option<bool>,
    /// Match whole words only.
    #[serde(default)]
    #[ts(optional)]
    pub whole_word: Option<bool>,
    /// The projects to look in (every project when absent).
    #[serde(default)]
    #[ts(optional)]
    pub projects: Option<Vec<ProjectId>>,
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TextMatch {
    /// The one-based line number.
    pub line: u32,
    /// The line's text (without its terminator), or a window of it for a very long line.
    pub text: String,
    /// Where `text` starts in the line, in UTF-16 code units (0 unless it is a window).
    pub column: u32,
    /// The matches in `text`, as UTF-16 `[start, end)` offsets into `text`.
    pub ranges: Vec<[u32; 2]>,
}

/// One stream message: a file's matching lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchFile {
    /// The file's project.
    pub project: ProjectId,
    /// The file, relative to the project's folder.
    pub path: String,
    /// Its matching lines, in order.
    pub matches: Vec<TextMatch>,
}

/// What a finished text search found in one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTextMatches {
    /// The project.
    pub project: ProjectId,
    /// Its files with matches.
    pub files: u32,
    /// Its matching lines streamed.
    pub matches: u32,
}

/// What a finished text search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchSummary {
    /// Files with matches.
    pub files: u32,
    /// Matching lines streamed.
    pub matches: u32,
    /// Files searched.
    pub searched: u32,
    /// The search stopped at the match limit; there are more.
    pub truncated: bool,
    /// A project has more files than its file list holds; some weren't searched.
    pub files_truncated: bool,
    /// Each searched project's counts, in workspace order.
    pub projects: Vec<ProjectTextMatches>,
}

/// `search_text`: streams `TextSearchFile` messages and resolves with a summary.
pub fn search_text(ctx: &Ctx<'_>, args: TextSearchArgs) -> CommandResult<TextSearchSummary> {
    if args.query.is_empty() {
        return Err(CommandError::invalid_args("the search text is empty"));
    }
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!args.case_sensitive.unwrap_or(false))
        .fixed_strings(!args.regex.unwrap_or(false))
        .word(args.whole_word.unwrap_or(false))
        .build(&args.query)
        .map_err(|error| CommandError::invalid_args(format!("invalid search: {error}")))?;
    let projects = look_in(ctx, args.projects.as_deref())?;
    let mut summary = TextSearchSummary {
        files: 0,
        matches: 0,
        searched: 0,
        truncated: false,
        files_truncated: false,
        projects: Vec::with_capacity(projects.len()),
    };
    let stop = AtomicBool::new(false);
    for project in &projects {
        let mut counts = ProjectTextMatches {
            project: project.id().clone(),
            files: 0,
            matches: 0,
        };
        if !summary.truncated {
            ctx.check_cancelled()?;
            let files = file_list(project, project.readers()?.as_ref())?;
            summary.files_truncated |= files.truncated;
            let root = project.root();
            for chunk in files.paths.chunks(CHUNK) {
                ctx.check_cancelled()?;
                let found: Vec<Option<TextSearchFile>> = chunk
                    .par_iter()
                    .map(|path| {
                        if stop.load(Ordering::Relaxed) || ctx.cancel.is_cancelled() {
                            return None;
                        }
                        // Stored paths may predate a link swapped in since: read only what
                        // stays inside the project's folder.
                        let confined = confine(root, path).ok()?;
                        search_file(&matcher, confined.absolute(), project.id(), path, &ctx.cancel)
                    })
                    .collect();
                summary.searched += count(chunk.len());
                for mut file in found.into_iter().flatten() {
                    let room = MAX_MATCHES - summary.matches as usize;
                    if room == 0 {
                        summary.truncated = true;
                        break;
                    }
                    if file.matches.len() > room {
                        file.matches.truncate(room);
                        summary.truncated = true;
                    }
                    summary.files += 1;
                    summary.matches += count(file.matches.len());
                    counts.files += 1;
                    counts.matches += count(file.matches.len());
                    ctx.stream(&file);
                    if summary.truncated {
                        break;
                    }
                }
                if summary.truncated {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        summary.projects.push(counts);
    }
    ctx.check_cancelled()?;
    Ok(summary)
}

/// The matching lines of one file (at most [`MAX_MATCHES`] + 1, so the caller sees there are
/// more), or `None` for no match, a binary or oversized file, or an unreadable one.
fn search_file(
    matcher: &RegexMatcher,
    absolute: &std::path::Path,
    project: &ProjectId,
    path: &str,
    cancel: &unfold_engine::CancelToken,
) -> Option<TextSearchFile> {
    if std::fs::metadata(absolute).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut searcher: Searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();
    let mut sink = Lines {
        matcher,
        cancel,
        matches: Vec::new(),
    };
    searcher.search_path(matcher, absolute, &mut sink).ok()?;
    (!sink.matches.is_empty()).then(|| TextSearchFile {
        project: project.clone(),
        path: path.to_owned(),
        matches: sink.matches,
    })
}

/// Collects matching lines.
struct Lines<'a> {
    matcher: &'a RegexMatcher,
    cancel: &'a unfold_engine::CancelToken,
    matches: Vec<TextMatch>,
}

impl Sink for Lines<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, found: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let line = String::from_utf8_lossy(found.bytes());
        let line = line.trim_end_matches(['\n', '\r']);
        let mut ranges = Vec::new();
        // Matching the decoded line again gives offsets into exactly the text shown.
        let _ = self.matcher.find_iter(line.as_bytes(), |found| {
            if found.start() < found.end() {
                ranges.push((found.start(), found.end()));
            }
            true
        });
        let number = found
            .line_number()
            .map_or(0, |number| u32::try_from(number).unwrap_or(u32::MAX));
        self.matches.push(preview(line, number, &ranges));
        Ok(self.matches.len() <= MAX_MATCHES && !self.cancel.is_cancelled())
    }
}

/// A line's text and its matches in UTF-16, windowed around the first match when the line is
/// long.
fn preview(line: &str, number: u32, ranges: &[(usize, usize)]) -> TextMatch {
    let (start, end) = if line.len() <= MAX_PREVIEW_BYTES {
        (0, line.len())
    } else {
        let first = ranges.first().map_or(0, |range| range.0);
        let start = floor_boundary(line, first.saturating_sub(PREVIEW_LEAD_BYTES));
        let end = floor_boundary(line, (start + MAX_PREVIEW_BYTES).min(line.len()));
        (start, end)
    };
    let text = &line[start..end];
    let units = |from: usize, to: usize| count(line[from..to].encode_utf16().count());
    TextMatch {
        line: number,
        text: text.to_owned(),
        column: units(0, start),
        ranges: ranges
            .iter()
            .filter(|(from, to)| *from < end && *to > start)
            .map(|&(from, to)| {
                let from = floor_boundary(line, from.max(start));
                let to = floor_boundary(line, to.min(end));
                [units(start, from), units(start, to)]
            })
            .collect(),
    }
}

/// The nearest character boundary at or before `index`.
fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_use_utf16_ranges() {
        let shown = preview("é = place(x)", 3, &[(5, 10)]);
        assert_eq!(shown.text, "é = place(x)");
        assert_eq!(shown.ranges, [[4, 9]]);
        assert_eq!(shown.column, 0);
    }

    #[test]
    fn long_lines_are_windowed_around_the_first_match() {
        let line = format!("{}needle{}", "a".repeat(1000), "b".repeat(1000));
        let shown = preview(&line, 1, &[(1000, 1006)]);
        assert_eq!(shown.column, 920);
        assert_eq!(shown.text.len(), MAX_PREVIEW_BYTES);
        assert_eq!(shown.ranges, [[80, 86]]);
    }
}
