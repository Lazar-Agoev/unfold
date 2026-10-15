//! The code panel's model (docs/reader.md#the-code-panels-model): what the panel beside the canvas
//! shows for one selection.
//!
//! | Part | What it is |
//! | --- | --- |
//! | `file`, `state` | The file and whether its lines can be trusted: current, changed since the last compile, doesn't parse, not analyzed, or not found |
//! | `span` | The view's first and last line |
//! | `lines` | A window of the view's lines with their real numbers, dedented to the view, highlighted, each with its block and its calls' names for hover |
//! | `mark` | What the selection marks inside the view: a tint on its lines, and a call's expression |
//! | `path` | File › definitions › control blocks › parts › the block, each with its label and key, and the step that is showing |
//! | `definitions` | The first line of each definition on the window's lines, for `⊕` |
//!
//! A request names the **selected** target and, when the path widens the view, the target to
//! **show** instead (a step of the path, or the file); the selection stays marked inside it.
//! Which lines a block shows is the table of "What each block shows": a definition from its
//! first decorator, a part inside its whole control block, a call inside its statement. An
//! answer holds a **window** of those lines, at most [`MAX_WINDOW`]: the ones the request asks
//! for, or a page that starts a little above what the selection marks.
//!
//! Lines are cropped and marked only from source verified against the facts' hash, kept with
//! its indexes and highlighting in the [`SourceCache`]. A file that changed since the last
//! compile, or stopped parsing, is shown whole as it is now, with nothing marked and no blocks:
//! its stored ranges may have moved. A block whose file left the graph shows that file whole, not
//! analyzed. Every column is a UTF-16 offset into its line's shown (dedented) text.

mod lines;
mod select;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::vocab::{FileState, InputRole};
use unfold_core::{App, ByteRange, Input, Key, LineIndex, RepoPath, Snapshot, Utf16Index};

use crate::contracts::{FactSource, Facts};
use crate::error::Result;
use crate::source::{Source, SourceCache, SourceState, highlight};

use lines::{Render, Text};
use select::{MarkSpec, Origin, Shows, Span};

pub(crate) use select::label;

/// The most lines one answer holds.
pub const MAX_WINDOW: u32 = 400;

/// What the panel opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A block: a module, definition, control block, part, call, or exit.
    Block(Key),
    /// Lines of a file: a combined step, a row of other calls, or a gap tick, and the lines a
    /// resource or connection cites (which can be lines of an evidence file).
    Lines {
        /// The file.
        path: RepoPath,
        /// The first and last line, one-based and inclusive.
        lines: [u32; 2],
        /// The lines to mark, for a citation.
        mark: Option<[u32; 2]>,
        /// A combined step's title, which labels it in the path after `⊞`; otherwise the path
        /// says which lines (`line 24`).
        title: Option<String>,
    },
    /// A written member of a class or type card, by the card's key and the member's index.
    Field {
        /// The class or type.
        key: Key,
        /// The member's index among its written members.
        index: u32,
    },
    /// A whole file: a source file, an evidence file, or any other file in the folder.
    File(RepoPath),
    /// An app or an entrance, by its key, with the byte span of its table in `unfold.toml`
    /// (the settings' `AppEntry` or `EntranceEntry` span: the reader doesn't read settings).
    Start {
        /// The app's or entrance's key.
        key: Key,
        /// Its declaration in `unfold.toml`.
        span: ByteRange,
    },
}

/// One request for the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRequest {
    /// The selected target: it heads the path, and it stays marked.
    pub selected: Target,
    /// What to show instead of the selection's own lines, when the path widened the view: a
    /// block on the selection's path, the file, or an app holding a selected entrance.
    pub show: Option<Target>,
    /// The view's lines to answer with, first and last, one-based: at most [`MAX_WINDOW`] are.
    /// `None` answers with a page from a little above the first line the selection marks.
    pub window: Option<[u32; 2]>,
}

/// Where the panel's files and facts come from.
#[derive(Clone, Copy)]
pub struct CodeSources<'a> {
    /// The stored facts.
    pub facts: &'a dyn FactSource,
    /// The folder the files live in.
    pub root: &'a Path,
    /// The inputs of the last compile: `unfold.toml` and the evidence files, with their hashes.
    pub inputs: &'a [Input],
    /// The apps of `unfold.toml`, with their entrances.
    pub apps: &'a [App],
    /// Whether `unfold.toml` parses, as the engine last read it (the reader doesn't read
    /// settings): one that doesn't is shown whole as it is now.
    pub settings_parse: bool,
    /// One highlighter per language.
    pub highlighters: &'a [&'a dyn Highlighter],
    /// The sources verified before, kept with their indexes and highlighting.
    pub kept: &'a SourceCache,
}

/// Whether the shown lines can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum CodeState {
    /// The file is the one the facts came from: cropped and marked.
    Current,
    /// "Changed since the last compile · showing the whole file": the whole file as it is now.
    Changed,
    /// "Doesn't parse · showing the whole file": the whole file as it is now.
    DoesNotParse,
    /// "Not analyzed": the file is outside the graph, shown whole.
    NotAnalyzed,
    /// "Not found in the code": the target no longer resolves, or its file is gone.
    NotFound,
}

/// The code panel's model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeView {
    /// The file; absent when the target no longer resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub file: Option<RepoPath>,
    /// Whether the lines can be trusted.
    pub state: CodeState,
    /// The view's first and last line; absent when nothing is shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub span: Option<[u32; 2]>,
    /// The window's lines, in order: those of the view the request asked for. Empty when not
    /// found, or when the window is past the view.
    pub lines: Vec<CodeLine>,
    /// What the selection marks inside the view.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub mark: Option<Mark>,
    /// The file, then each block above the selection, then the selection; empty when not
    /// found.
    pub path: Vec<PathStep>,
    /// Each definition whose first line is one of the window's, in written order, for `⊕` (the
    /// UI decides which have no card on the page).
    pub definitions: Vec<DefinitionLine>,
}

impl CodeView {
    /// The model of a target that no longer resolves: nothing shown.
    pub fn not_found() -> Self {
        Self {
            file: None,
            state: CodeState::NotFound,
            span: None,
            lines: Vec::new(),
            mark: None,
            path: Vec::new(),
            definitions: Vec::new(),
        }
    }
}

/// One shown line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeLine {
    /// The one-based line number in the file.
    pub number: u32,
    /// The line without its terminator, less the indentation the view shares.
    pub text: String,
    /// Highlighted spans, sorted and non-overlapping; text outside them is plain.
    pub tokens: Vec<Token>,
    /// The block hovering the line outlines: the first block of the statement it is in, or the
    /// block it opens (a definition's header, a control block's header, a part's opening line).
    /// Absent for a line no block covers, which lights up its gap tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub block: Option<Key>,
    /// The written names of the calls on the line; hovering one outlines that call.
    pub calls: Vec<CallName>,
    /// The line goes on past `text`: it is too long to show whole (written only when true).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub cut: bool,
}

/// One highlighted span of a line: `[start, end, class]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Token(pub u32, pub u32, pub TokenClass);

/// A call's written name on one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallName {
    /// Where the name starts.
    pub start: u32,
    /// Where it ends.
    pub end: u32,
    /// The call.
    pub key: Key,
}

/// A range of one line's shown text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    /// The one-based line number.
    pub line: u32,
    /// Where the range starts.
    pub start: u32,
    /// Where it ends.
    pub end: u32,
}

/// What the selection marks inside the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    /// The first and last tinted line: a part's opening line and body, cited lines, or the
    /// selection's lines once the view widened.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub lines: Option<[u32; 2]>,
    /// A call's own expression, one range per line of the window it spans; empty for anything
    /// else.
    pub expression: Vec<LineRange>,
}

/// One step of the path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PathStep {
    /// What the step says: a file's path, a definition's name, a control block's header, the
    /// text that opens a part, a call's written callee, an exit's keyword, an app's or
    /// entrance's name, a combined step's title after `⊞`, a field's name, or the lines
    /// (`line 24`).
    pub label: String,
    /// The block, app, or entrance the step widens the view to. Absent for the file (which
    /// widens to the whole file) and for a step that is only lines or a field.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<Key>,
    /// Whether the view shows this step (bold).
    pub showing: bool,
}

/// Where a definition starts, for `⊕`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DefinitionLine {
    /// Its first line: its first decorator's, or its own.
    pub line: u32,
    /// The definition.
    pub key: Key,
}

/// The code panel's model for `request`.
pub fn code_view(sources: &CodeSources<'_>, request: &CodeRequest) -> Result<CodeView> {
    let mut facts = Facts::new(sources.facts, sources.root);
    let Some(selected) = select::resolve(&mut facts, sources, &request.selected)? else {
        return Ok(CodeView::not_found());
    };
    let shown = match &request.show {
        Some(show) => select::resolve(&mut facts, sources, show)?
            .filter(|show| show.origin.path() == selected.origin.path()),
        None => None,
    };
    let (state, body) = load(sources, &selected.origin);
    let verified = body.as_ref().filter(|_| state == CodeState::Current);
    let mut path = select::path(&selected, verified.map(Body::text));
    let Some(body) = body else {
        return Ok(CodeView {
            file: Some(selected.origin.path().clone()),
            state,
            span: None,
            lines: Vec::new(),
            mark: None,
            path,
            definitions: Vec::new(),
        });
    };
    // A file that changed or stopped parsing is shown whole, with nothing marked: its stored
    // ranges may have moved.
    let current = state == CodeState::Current;
    let (view, mark, shows) = match (&shown, current) {
        (_, false) => (Span::Whole, MarkSpec::default(), Shows::File),
        (Some(shown), true) => (shown.view, selected.region, shown.shows.clone()),
        (None, true) => (selected.view, selected.mark, selected.shows.clone()),
    };
    mark_showing(&mut path, &shows);
    let tree = match &selected.origin {
        Origin::Source(tree) if current => Some(&**tree),
        Origin::Source(_) | Origin::Input(_) | Origin::Outside(_) => None,
    };
    let render = Render::new(
        body.indexed(),
        selected.origin.path(),
        view,
        &mark,
        request.window,
    )?;
    let highlights = highlighter(sources, &selected.origin).map(|highlighter| body.highlights(highlighter));
    Ok(CodeView {
        file: Some(selected.origin.path().clone()),
        state,
        span: Some(render.span()),
        lines: render.lines(tree, highlights.as_deref())?,
        mark: render.mark(&mark)?,
        path,
        definitions: tree.map_or_else(Vec::new, |tree| render.definitions(tree)),
    })
}

/// Marks the step of `path` that `shows` names as showing.
fn mark_showing(path: &mut [PathStep], shows: &Shows) {
    let index = match shows {
        Shows::File => Some(0),
        Shows::Key(key) => path.iter().position(|step| step.key.as_ref() == Some(key)),
        Shows::Last => path.len().checked_sub(1),
    };
    if let Some(step) = index.and_then(|index| path.get_mut(index)) {
        step.showing = true;
    }
}

/// The highlighter for a source file's language. Inputs and files outside the graph are plain
/// text.
fn highlighter<'a>(sources: &CodeSources<'a>, origin: &Origin) -> Option<&'a dyn Highlighter> {
    let Origin::Source(tree) = origin else {
        return None;
    };
    sources
        .highlighters
        .iter()
        .copied()
        .find(|highlighter| highlighter.language() == tree.language())
}

/// A file's text with its indexes: verified against its hash (kept, with its highlighting), or
/// read as it is now (indexed for this answer).
enum Body {
    Verified(Arc<Source>),
    Now {
        snapshot: Snapshot,
        lines: LineIndex,
        utf16: Utf16Index,
    },
}

impl Body {
    fn text(&self) -> &str {
        match self {
            Self::Verified(source) => source.text(),
            Self::Now { snapshot, .. } => snapshot.text(),
        }
    }

    fn indexed(&self) -> Text<'_> {
        match self {
            Self::Verified(source) => Text::new(source.text(), source.lines(), source.utf16()),
            Self::Now {
                snapshot,
                lines,
                utf16,
            } => Text::new(snapshot.text(), lines, utf16),
        }
    }

    /// The text's highlighting by `highlighter`, its language's.
    fn highlights(&self, highlighter: &dyn Highlighter) -> Cow<'_, [HighlightSpan]> {
        match self {
            Self::Verified(source) => Cow::Borrowed(source.highlights(highlighter)),
            Self::Now { snapshot, .. } => Cow::Owned(highlight(snapshot.text(), highlighter)),
        }
    }
}

/// The state of `origin`'s file and its text: verified when current, as it is now when it
/// changed, stopped parsing, or is outside the graph, and none when it is gone.
fn load(sources: &CodeSources<'_>, origin: &Origin) -> (CodeState, Option<Body>) {
    let expected = match origin {
        Origin::Source(tree) if tree.file().state != FileState::Current => {
            return now(sources, origin, CodeState::DoesNotParse);
        }
        Origin::Source(tree) => tree.file().hash,
        Origin::Input(input) if input.role == InputRole::UnfoldConfig && !sources.settings_parse => {
            return now(sources, origin, CodeState::DoesNotParse);
        }
        Origin::Input(input) => Some(input.hash),
        Origin::Outside(_) => return now(sources, origin, CodeState::NotAnalyzed),
    };
    let checked = sources
        .kept
        .check(sources.root, origin.path(), expected, origin.line_breaks());
    match (checked.state, checked.source) {
        (SourceState::Current, Some(source)) => (CodeState::Current, Some(Body::Verified(source))),
        (SourceState::Current | SourceState::Changed, _) => now(sources, origin, CodeState::Changed),
        (SourceState::Missing, _) => (CodeState::NotFound, None),
    }
}

/// `origin`'s file as it is now, in `state`; not found when it can't be read as text.
fn now(sources: &CodeSources<'_>, origin: &Origin, state: CodeState) -> (CodeState, Option<Body>) {
    let path = origin.path();
    std::fs::read(sources.root.join(path.as_str()))
        .ok()
        .and_then(|bytes| Snapshot::new(path.clone(), bytes).ok())
        .map_or((CodeState::NotFound, None), |snapshot| {
            let breaks = origin.line_breaks();
            let body = Body::Now {
                lines: snapshot.line_index(breaks),
                utf16: snapshot.utf16_index(),
                snapshot,
            };
            (state, Some(body))
        })
}

/// The input of `unfold.toml`, where apps and entrances are declared.
fn settings_input(inputs: &[Input]) -> Option<&Input> {
    inputs.iter().find(|input| input.role == InputRole::UnfoldConfig)
}
