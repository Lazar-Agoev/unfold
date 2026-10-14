//! The code panel's model (docs/reader.md#the-code-panels-model): what the panel beside the canvas
//! shows for one selection.
//!
//! | Part | What it is |
//! | --- | --- |
//! | `file`, `state` | The file and whether its lines can be trusted: current, changed since the last compile, doesn't parse, not analyzed, or not found |
//! | `lines` | The shown lines with their real numbers, dedented to the view, highlighted, each with its block and its calls' names for hover |
//! | `mark` | What the selection marks inside the view: a tint on its lines, and a call's expression |
//! | `path` | File › definitions › control blocks › parts › the block, each with its label and key, and the step that is showing |
//! | `definitions` | Each definition's first line, for `⊕` |
//!
//! A request names the **selected** target and, when the path widens the view, the target to
//! **show** instead (a step of the path, or the file); the selection stays marked inside it.
//! Which lines a block shows is the table of "What each block shows": a definition from its
//! first decorator, a part inside its whole control block, a call inside its statement.
//!
//! Lines are cropped and marked only from source verified against the facts' hash. A file that
//! changed since the last compile, or stopped parsing, is shown whole as it is now, with nothing
//! marked and no blocks: its stored ranges may have moved. Every column is a UTF-16 offset into
//! its line's shown (dedented) text.

mod lines;
mod select;

#[cfg(test)]
mod tests;

use std::path::Path;

use serde::Serialize;
use unfold_core::highlight::{Highlighter, TokenClass};
use unfold_core::vocab::{FileState, InputRole};
use unfold_core::{App, ByteRange, Input, Key, RepoPath, Snapshot};

use crate::contracts::{FactSource, Facts};
use crate::error::Result;
use crate::source::{Source, Verified, verify_file};

use lines::{Render, Text};
use select::{MarkSpec, Origin, Shows, Span};

pub(crate) use select::label;

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
    /// The shown lines, in order; empty when not found.
    pub lines: Vec<CodeLine>,
    /// What the selection marks inside the view.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub mark: Option<Mark>,
    /// The file, then each block above the selection, then the selection; empty when not
    /// found.
    pub path: Vec<PathStep>,
    /// Each definition whose first line is shown, in written order, for `⊕` (the UI decides
    /// which have no card on the page).
    pub definitions: Vec<DefinitionLine>,
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
    /// A call's own expression, one range per line it spans; empty for anything else.
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
        return Ok(CodeView {
            file: None,
            state: CodeState::NotFound,
            lines: Vec::new(),
            mark: None,
            path: Vec::new(),
            definitions: Vec::new(),
        });
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
    let text = Text::new(body.text());
    let render = Render::new(&text, selected.origin.path(), view)?;
    Ok(CodeView {
        file: Some(selected.origin.path().clone()),
        state,
        lines: render.lines(tree, highlighter(sources, &selected.origin))?,
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

/// A file's text: verified against its hash, or read as it is now.
enum Body {
    Verified(Source),
    Now(Snapshot),
}

impl Body {
    fn text(&self) -> &str {
        match self {
            Self::Verified(source) => source.text(),
            Self::Now(snapshot) => snapshot.text(),
        }
    }
}

/// The state of `origin`'s file and its text: verified when current, as it is now when it
/// changed, stopped parsing, or is outside the graph, and none when it is gone.
fn load(sources: &CodeSources<'_>, origin: &Origin) -> (CodeState, Option<Body>) {
    let expected = match origin {
        Origin::Source(tree) if tree.file().state != FileState::Current => {
            return now(sources, tree.path(), CodeState::DoesNotParse);
        }
        Origin::Source(tree) => tree.file().hash,
        Origin::Input(input) if input.role == InputRole::UnfoldConfig && !sources.settings_parse => {
            return now(sources, &input.path, CodeState::DoesNotParse);
        }
        Origin::Input(input) => Some(input.hash),
        Origin::Outside(path) => return now(sources, path, CodeState::NotAnalyzed),
    };
    match verify_file(sources.root, origin.path(), expected) {
        Verified::Current(source) => (CodeState::Current, Some(Body::Verified(source))),
        Verified::Changed => now(sources, origin.path(), CodeState::Changed),
        Verified::Missing => (CodeState::NotFound, None),
    }
}

/// `path` as it is now, in `state`; not found when it can't be read as text.
fn now(sources: &CodeSources<'_>, path: &RepoPath, state: CodeState) -> (CodeState, Option<Body>) {
    std::fs::read(sources.root.join(path.as_str()))
        .ok()
        .and_then(|bytes| Snapshot::new(path.clone(), bytes).ok())
        .map_or((CodeState::NotFound, None), |snapshot| {
            (state, Some(Body::Now(snapshot)))
        })
}

/// The input of `unfold.toml`, where apps and entrances are declared.
fn settings_input(inputs: &[Input]) -> Option<&Input> {
    inputs.iter().find(|input| input.role == InputRole::UnfoldConfig)
}
