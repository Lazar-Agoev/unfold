//! `file_view`: what the peek sheet renders (architecture §11.7, decision U-48).
//!
//! The peek sheet shows a whole file, read-only, through the same render model as Code cards:
//! syntax colors, the statement outline with its folds, and calls as links. Long files are
//! virtualized by line, so the sheet asks for pages: with `range`, only those lines' text, spans,
//! outline items, and links come back. Every offset in a page is a UTF-16 offset from the start
//! of the **file**; `textStart` says where the page's `text` begins.
//!
//! | `graph` | When | What is drawn |
//! | --- | --- | --- |
//! | `current` | The bytes on disk are the ones the graph was built from | Colors, outline, folds, links |
//! | `not_compiled` | The file changed since the graph was built (or the update is pending) | The current text with colors, labeled as not compiled yet, without graph links |
//! | `evidence` | The file is an evidence file (`[evidence] files`) and its bytes are the ones the graph recorded | The text with line numbers |
//! | `not_indexed` | The file isn't in the graph (another language, ignored, out of scope) | The text, with colors when the language is known |
//!
//! An evidence file whose bytes changed since the graph recorded them is `not_compiled`.
//!
//! A file that isn't UTF-8 text, or is larger than [`MAX_FILE_BYTES`], comes back without text.
//!
//! A virtualized sheet asks for many pages of one file, so the whole-file model is rendered once
//! and kept in a small per-project cache ([`RenderCache`]) keyed by the path, the hash of the bytes
//! read for this request, and the project's change generation (bumped by every graph update and
//! file change). Each request still reads and hashes the file, so a page is never cut from a model
//! of other bytes or another graph; a hit only skips highlighting and rendering. Pages are cut by
//! binary search over the model's sorted lines, spans, and links.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::highlight::Highlighter;
use unfold_core::vocab::{FileState, InputRole, Language};
use unfold_core::{ByteRange, ContentHash, FileGraph, RepoPath, Snapshot, Utf16Index};
use unfold_reader::code::{CallLink, CodeLine, CodeSpan, CodeText, FoldPolicy, OutlineItem, render};

use super::{LineRange, ProjectExtras, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::shared::lock;
use crate::workspace::Workspace;

/// The largest file the peek sheet shows.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// The arguments of `file_view`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileViewArgs {
    /// The project.
    pub project: ProjectId,
    /// The file, relative to the project's folder.
    pub path: String,
    /// Only these lines (a page of a virtualized tab); the whole file when absent.
    #[serde(default)]
    #[ts(optional)]
    pub range: Option<LineRange>,
}

/// How the shown text relates to the graph. See the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GraphMatch {
    /// The text is what the graph was built from.
    Current,
    /// The file changed since the graph was built; no graph links are drawn.
    NotCompiled,
    /// The file is an evidence file, and the text is what the graph recorded.
    Evidence,
    /// The file isn't in the graph.
    NotIndexed,
}

/// Why a file has no text to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Unshowable {
    /// It isn't UTF-8 text.
    Binary,
    /// It is larger than the tab shows.
    TooLarge,
}

/// The peek sheet's render model of a file, or one page of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    /// The file.
    pub path: String,
    /// The language whose colors are applied.
    pub language: Option<Language>,
    /// How the text relates to the graph.
    pub graph: GraphMatch,
    /// The file's state in the graph, when it is in the graph.
    pub file_state: Option<FileState>,
    /// Why there is no text, if there isn't.
    pub unshowable: Option<Unshowable>,
    /// The file's line count.
    pub line_count: u32,
    /// The first line returned.
    pub start_line: u32,
    /// The last line returned (`startLine - 1` when the page is empty).
    pub end_line: u32,
    /// Where `text` begins in the file, in UTF-16 code units.
    pub text_start: u32,
    /// The text of the returned lines, terminators included.
    pub text: String,
    /// The returned lines.
    pub lines: Vec<CodeLine>,
    /// Syntax colors overlapping the returned lines.
    pub spans: Vec<CodeSpan>,
    /// Statement outline items overlapping the returned lines (with folds); `current` only.
    pub outline: Vec<OutlineItem>,
    /// Calls, constructions, and imports on the returned lines; `current` only.
    pub links: Vec<CallLink>,
}

/// A rendered whole file, as the cache keeps it.
#[derive(Debug)]
pub(crate) struct RenderedFile {
    language: Option<Language>,
    graph: GraphMatch,
    file_state: Option<FileState>,
    code: CodeText,
    utf16: Utf16Index,
}

/// The most rendered files one project keeps.
const CACHED_FILES: usize = 8;
/// The most text the kept files may hold together, in bytes.
const CACHED_BYTES: usize = 32 * 1024 * 1024;

/// Recently rendered files of one project, most recently used last. See the
/// [module documentation](self).
#[derive(Debug, Default)]
pub(crate) struct RenderCache {
    entries: Vec<(CacheKey, Arc<RenderedFile>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKey {
    path: RepoPath,
    hash: ContentHash,
    generation: u64,
}

impl RenderCache {
    fn get(&mut self, key: &CacheKey) -> Option<Arc<RenderedFile>> {
        let index = self.entries.iter().position(|(found, _)| found == key)?;
        let entry = self.entries.remove(index);
        let rendered = Arc::clone(&entry.1);
        self.entries.push(entry);
        Some(rendered)
    }

    fn put(&mut self, key: CacheKey, rendered: Arc<RenderedFile>) {
        // A render from before a change that another request already saw is never asked for.
        if self
            .entries
            .iter()
            .any(|(found, _)| found.generation > key.generation)
        {
            return;
        }
        // Older generations and other versions of this file can never be asked for again.
        self.entries
            .retain(|(found, _)| found.generation == key.generation && found.path != key.path);
        self.entries.push((key, rendered));
        let mut bytes: usize = self.entries.iter().map(|(_, file)| file.code.text.len()).sum();
        while self.entries.len() > CACHED_FILES || (bytes > CACHED_BYTES && self.entries.len() > 1) {
            let (_, evicted) = self.entries.remove(0);
            bytes -= evicted.code.text.len();
        }
    }
}

/// `file_view`: the render model of a whole file, or of a range of its lines.
pub fn file_view(ctx: &Ctx<'_>, args: FileViewArgs) -> CommandResult<FileView> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    render_page(&workspace, &project.extras(), &args.path, args.range)
}

/// The render model of the live project's file at `path` (relative to its folder), or of a
/// range of its lines.
pub(crate) fn render_page(
    workspace: &Workspace,
    extras: &ProjectExtras,
    path: &str,
    range: Option<LineRange>,
) -> CommandResult<FileView> {
    let confined = workspace.confine(path)?;
    let shown = path;
    let path = confined
        .relative()
        .cloned()
        .ok_or_else(|| CommandError::invalid_args("`file_view` needs a file path"))?;
    let metadata = std::fs::metadata(confined.absolute())
        .map_err(|error| CommandError::not_found(format!("cannot read `{shown}`: {error}")))?;
    if !metadata.is_file() {
        return Err(CommandError::invalid_args(format!("`{shown}` is not a file")));
    }
    // Read before the graph: a graph update after this point bumps the generation, so a model
    // rendered from an older graph is never stored under a newer generation.
    let generation = extras.generation();
    let file_graph = || -> CommandResult<Option<FileGraph>> {
        Ok(match workspace.readers() {
            Ok(pool) => pool.get()?.file_graph(path.as_str())?,
            Err(_) => None,
        })
    };
    // The hash the graph recorded for the file as an evidence file, if it is one.
    let evidence = || -> CommandResult<Option<ContentHash>> {
        Ok(match workspace.readers() {
            Ok(pool) => pool
                .get()?
                .inputs()?
                .into_iter()
                .find(|input| input.path == path && input.role == InputRole::Evidence)
                .map(|input| input.hash),
            Err(_) => None,
        })
    };
    let unshowable = |reason| -> CommandResult<FileView> {
        let graph = file_graph()?;
        let recorded = if graph.is_some() { None } else { evidence()? };
        Ok(FileView {
            path: path.as_str().to_owned(),
            language: graph
                .as_ref()
                .map(|file| file.file.language)
                .or_else(|| language_of(&path)),
            graph: unverified(graph.is_some(), recorded, None),
            file_state: graph.as_ref().map(|file| file.file.state),
            unshowable: Some(reason),
            line_count: 0,
            start_line: 1,
            end_line: 0,
            text_start: 0,
            text: String::new(),
            lines: Vec::new(),
            spans: Vec::new(),
            outline: Vec::new(),
            links: Vec::new(),
        })
    };
    if metadata.len() > MAX_FILE_BYTES {
        return unshowable(Unshowable::TooLarge);
    }
    let bytes = std::fs::read(confined.absolute())
        .map_err(|error| CommandError::not_found(format!("cannot read `{shown}`: {error}")))?;
    let Ok(snapshot) = Snapshot::new(path.clone(), bytes) else {
        return unshowable(Unshowable::Binary);
    };
    let key = CacheKey {
        path: path.clone(),
        hash: snapshot.hash(),
        generation,
    };
    // A hit skips the graph read too: everything taken from the graph is in the cached model,
    // and the generation says the graph hasn't changed since.
    let cached = lock(&extras.rendered).get(&key);
    let rendered = match cached {
        Some(rendered) => rendered,
        None => {
            let graph = file_graph()?;
            let language = graph
                .as_ref()
                .map(|file| file.file.language)
                .or_else(|| language_of(&path));
            let highlighter = language.and_then(|language| workspace.highlighter(language));
            let verified = graph
                .as_ref()
                .filter(|file| file.file.hash == Some(snapshot.hash()));
            let (code, matched) = match verified {
                Some(file) => (
                    render(file, 0, snapshot.text(), highlighter, FoldPolicy::SourceTab)?,
                    GraphMatch::Current,
                ),
                None => {
                    let recorded = if graph.is_some() { None } else { evidence()? };
                    (
                        plain(snapshot.text(), highlighter),
                        unverified(graph.is_some(), recorded, Some(snapshot.hash())),
                    )
                }
            };
            let rendered = Arc::new(RenderedFile {
                language,
                graph: matched,
                file_state: graph.as_ref().map(|file| file.file.state),
                utf16: Utf16Index::new(&code.text),
                code,
            });
            lock(&extras.rendered).put(key, Arc::clone(&rendered));
            rendered
        }
    };
    Ok(page(&path, &rendered, range))
}

/// How text drawn without graph facts relates to the graph: a file the graph indexes changed
/// since; an evidence file (the graph `recorded` its hash) is as recorded unless its bytes
/// (`hash`, when read) differ; anything else isn't in the graph.
fn unverified(indexed: bool, recorded: Option<ContentHash>, hash: Option<ContentHash>) -> GraphMatch {
    match recorded {
        _ if indexed => GraphMatch::NotCompiled,
        Some(recorded) if hash.is_none_or(|hash| hash == recorded) => GraphMatch::Evidence,
        Some(_) => GraphMatch::NotCompiled,
        None => GraphMatch::NotIndexed,
    }
}

/// The language whose highlighter colors a file outside the graph, by extension. JavaScript is
/// colored by the TypeScript highlighter, which reads it.
fn language_of(path: &RepoPath) -> Option<Language> {
    match path.extension()? {
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" => Some(Language::TypeScript),
        "py" | "pyi" => Some(Language::Python),
        _ => None,
    }
}

/// The render model of text that has no graph facts: lines and colors only.
fn plain(text: &str, highlighter: Option<&dyn Highlighter>) -> CodeText {
    let utf16 = Utf16Index::new(text);
    let unit = |byte: usize| utf16.to_utf16(count(byte)).unwrap_or(utf16.len_utf16());
    let lines = split_lines(text)
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| CodeLine {
            number: count(index) + 1,
            start: unit(start),
            end: unit(end),
            indent: count(
                text.as_bytes()[start..end]
                    .iter()
                    .take_while(|&&byte| byte == b' ' || byte == b'\t')
                    .count(),
            ),
        })
        .collect();
    let mut spans = Vec::new();
    if let Some(highlighter) = highlighter {
        let mut previous_end = 0;
        for span in highlighter.highlight(text) {
            let ByteRange { start, end } = span.range;
            let well_formed = start >= previous_end
                && start < end
                && (end as usize) <= text.len()
                && text.is_char_boundary(start as usize)
                && text.is_char_boundary(end as usize);
            if well_formed {
                previous_end = end;
                spans.push(CodeSpan(unit(start as usize), unit(end as usize), span.class));
            }
        }
    }
    CodeText {
        text: text.to_owned(),
        dedent: 0,
        lines,
        spans,
        outline: Vec::new(),
        links: Vec::new(),
    }
}

/// Line byte ranges without terminators. Lines end at CR, LF, CRLF, U+2028, and U+2029, as the
/// graph's line numbers count them; a final terminator doesn't start an extra empty line.
fn split_lines(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut index) = (0usize, 0usize);
    while index < bytes.len() {
        let terminator = match bytes[index] {
            b'\n' => 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            0xE2 if bytes.get(index + 1) == Some(&0x80)
                && matches!(bytes.get(index + 2), Some(0xA8 | 0xA9)) =>
            {
                3
            }
            _ => 0,
        };
        if terminator == 0 {
            index += 1;
            continue;
        }
        lines.push((start, index));
        index += terminator;
        start = index;
    }
    if start < bytes.len() || start == 0 {
        lines.push((start, bytes.len()));
    }
    lines
}

/// Cuts the whole-file model down to the requested lines. Lines are numbered from 1 in order,
/// spans are sorted and disjoint, and links are in source order, so each part is found by
/// binary search; outline items (in preorder) are filtered.
fn page(path: &RepoPath, rendered: &RenderedFile, range: Option<LineRange>) -> FileView {
    let code = &rendered.code;
    let line_count = count(code.lines.len());
    let (first, last) = match range {
        Some(range) => (range.start_line.max(1), range.end_line.min(line_count)),
        None => (1, line_count),
    };
    let chosen: &[CodeLine] = if first <= last {
        &code.lines[first as usize - 1..last as usize]
    } else {
        &[]
    };
    let total = rendered.utf16.len_utf16();
    // The page runs from its first line's start to the next line's start (terminator included).
    let (start, end) = match (chosen.first(), chosen.last()) {
        (Some(head), Some(tail)) => {
            let end = code
                .lines
                .get(tail.number as usize)
                .map_or(total, |next| next.start);
            (head.start, end)
        }
        _ => (total, total),
    };
    let byte = |unit: u32| rendered.utf16.to_utf8(unit).unwrap_or(rendered.utf16.len_utf8()) as usize;
    let text = code.text.get(byte(start)..byte(end)).unwrap_or("").to_owned();
    let spans = &code.spans[code.spans.partition_point(|span| span.1 <= start)..];
    let spans = &spans[..spans.partition_point(|span| span.0 < end)];
    let links = &code.links[code.links.partition_point(|link| link.line < first)..];
    let links = &links[..links.partition_point(|link| link.line <= last)];
    FileView {
        path: path.as_str().to_owned(),
        language: rendered.language,
        graph: rendered.graph,
        file_state: rendered.file_state,
        unshowable: None,
        line_count,
        start_line: first,
        end_line: chosen.last().map_or(first.saturating_sub(1), |line| line.number),
        text_start: start,
        text,
        lines: chosen.to_vec(),
        spans: spans.to_vec(),
        outline: code
            .outline
            .iter()
            .filter(|item| item.end_line >= first && item.start_line <= last)
            .cloned()
            .collect(),
        links: links.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_like_the_graph() {
        assert_eq!(split_lines("a\nb\r\nc\rd"), [(0, 1), (2, 3), (5, 6), (7, 8)]);
        assert_eq!(split_lines("a\n"), [(0, 1)]);
        assert_eq!(split_lines(""), [(0, 0)]);
        assert_eq!(split_lines("a\u{2028}b").len(), 2);
    }

    fn rendered(text: &str) -> Arc<RenderedFile> {
        Arc::new(RenderedFile {
            language: None,
            graph: GraphMatch::NotIndexed,
            file_state: None,
            utf16: Utf16Index::new(text),
            code: plain(text, None),
        })
    }

    fn key(path: &str, text: &str, generation: u64) -> CacheKey {
        CacheKey {
            path: RepoPath::new(path).expect("path"),
            hash: ContentHash::of(text.as_bytes()),
            generation,
        }
    }

    #[test]
    fn the_render_cache_keeps_only_the_newest_generation() {
        let mut cache = RenderCache::default();
        cache.put(key("a.ts", "a", 1), rendered("a"));
        cache.put(key("b.ts", "b", 1), rendered("b"));
        assert!(cache.get(&key("a.ts", "a", 1)).is_some());
        assert!(cache.get(&key("a.ts", "a2", 1)).is_none(), "other bytes miss");
        cache.put(key("a.ts", "a2", 1), rendered("a2"));
        assert!(
            cache.get(&key("a.ts", "a", 1)).is_none(),
            "the older version is dropped"
        );
        cache.put(key("c.ts", "c", 0), rendered("c"));
        assert!(
            cache.get(&key("c.ts", "c", 0)).is_none(),
            "a stale render isn't kept"
        );
        cache.put(key("d.ts", "d", 2), rendered("d"));
        assert_eq!(cache.entries.len(), 1, "a newer generation drops the rest");
        for index in 0..20 {
            cache.put(key(&format!("{index}.ts"), "x", 2), rendered("x"));
        }
        assert_eq!(cache.entries.len(), CACHED_FILES);
    }

    #[test]
    fn evidence_files_are_shown_as_recorded_or_changed() {
        let recorded = ContentHash::of(b"create table orders;\n");
        let changed = ContentHash::of(b"create table customers;\n");
        assert_eq!(
            unverified(false, Some(recorded), Some(recorded)),
            GraphMatch::Evidence
        );
        assert_eq!(unverified(false, Some(recorded), None), GraphMatch::Evidence);
        assert_eq!(
            unverified(false, Some(recorded), Some(changed)),
            GraphMatch::NotCompiled
        );
        assert_eq!(unverified(true, None, Some(changed)), GraphMatch::NotCompiled);
        assert_eq!(unverified(false, None, Some(changed)), GraphMatch::NotIndexed);
    }

    #[test]
    fn plain_text_has_utf16_lines() {
        let code = plain("é\nab\n", None);
        assert_eq!(code.lines.len(), 2);
        assert_eq!((code.lines[1].start, code.lines[1].end), (2, 4));
    }
}
