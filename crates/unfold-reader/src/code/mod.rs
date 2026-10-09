//! The Code render model (architecture §10.3): what a Code card or a source tab renders for one
//! block or a whole file.
//!
//! The model crosses IPC as JSON, so it is compact and every offset is a UTF-16 code-unit offset
//! relative to the start of the shown text (decision U-12). It is built only from bytes verified
//! against the facts' hash ([`crate::source`]); a changed or missing file yields a state with no
//! text, never old ranges applied to new bytes.
//!
//! | Part | What it is |
//! | --- | --- |
//! | `text` | The exact verified slice, line endings and all |
//! | `lines` | Each line's number, UTF-16 start and end (terminator excluded), and indentation |
//! | `spans` | Syntax colors from the injected [`Highlighter`], as `[start, end, class]` |
//! | `outline` | The statement outline: compound blocks with their folds, and the rows the gutter groups |
//! | `links` | Every call, construction, and import in the slice, at its written callee range |

mod lines;
mod links;
mod outline;
mod spans;

use std::path::Path;

use serde::Serialize;
use unfold_core::highlight::{Highlighter, TokenClass};
use unfold_core::vocab::{BlockKind, EdgeKind, Language, Reason, Resolution, Role};
use unfold_core::{ByteRange, FileGraph, Key, ModuleLabel, RepoPath, Utf16Index};

use crate::error::{ReaderError, Result};
use crate::source::{Verified, verify_file};

pub use outline::group_for_lines;

/// Which folds start closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoldPolicy {
    /// A card: in a card longer than [`LONG_CARD_LINES`] lines, callbacks and `else if` chains
    /// of three or more checks start folded.
    #[default]
    Card,
    /// A source tab: nothing starts folded.
    SourceTab,
}

/// A card with more lines than this starts with its callbacks and long `else if` chains folded.
pub const LONG_CARD_LINES: usize = 40;

/// The render model of one block (or a file, through its root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeView {
    /// The shown block.
    pub key: Key,
    /// Its file.
    pub path: RepoPath,
    /// The file's language.
    pub language: Language,
    /// The block's kind.
    pub kind: BlockKind,
    /// The first line (one-based), from the stored facts.
    pub start_line: u32,
    /// The last line, from the stored facts.
    pub end_line: u32,
    /// The verified source, or why there is none.
    pub source: CodeSource,
}

/// The source part of a [`CodeView`]: present only when the bytes verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CodeSource {
    /// The bytes match the facts.
    Current(CodeText),
    /// The bytes changed since the facts were taken.
    Changed,
    /// The file is missing or unreadable.
    Missing,
}

/// Verified text with everything the Code view draws on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeText {
    /// The exact slice of the source.
    pub text: String,
    /// The indentation common to every nonblank line after the first, which the view may hide.
    pub dedent: u32,
    /// The lines of `text`.
    pub lines: Vec<CodeLine>,
    /// Highlighted spans, sorted and non-overlapping; text outside them is plain.
    pub spans: Vec<CodeSpan>,
    /// The statement outline in preorder.
    pub outline: Vec<OutlineItem>,
    /// Call, construction, and import links in source order.
    pub links: Vec<CallLink>,
}

/// One line of the shown text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CodeLine {
    /// The one-based line number in the file.
    pub number: u32,
    /// Where the line starts.
    pub start: u32,
    /// Where the line ends, before its terminator.
    pub end: u32,
    /// The number of leading spaces and tabs.
    pub indent: u32,
}

/// One highlighted span: `[start, end, class]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CodeSpan(pub u32, pub u32, pub TokenClass);

/// A statement or definition inside the shown block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    /// The block.
    pub key: Key,
    /// Its kind.
    pub kind: BlockKind,
    /// Its role under its saved parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub role: Option<Role>,
    /// The index of the enclosing outline item, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parent: Option<u32>,
    /// The first line.
    pub start_line: u32,
    /// The last line.
    pub end_line: u32,
    /// Where the block starts.
    pub start: u32,
    /// Where the block ends.
    pub end: u32,
    /// Whether the gutter may put it in a group: a statement (not a nested definition) in a
    /// `body`, `then`, `else`, `catch`, or `finally` role. A group is a run of consecutive
    /// groupable siblings with one parent and one role.
    pub groupable: bool,
    /// How it folds, for a compound block spanning several lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub fold: Option<Fold>,
}

/// How a compound block folds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Fold {
    /// Whether it starts folded ([`FoldPolicy`]).
    pub folded: bool,
    /// Where the hidden part begins: the end of the written header line for Python, otherwise
    /// the start of the body (or the end of the first line when there is no body slot).
    pub header_end: u32,
    /// Whether the folded row shows the hidden body as `{ … }` (TypeScript blocks with a body).
    pub braces: bool,
    /// The folded row's summary, such as `12 lines · returns`.
    pub summary: String,
    /// Whether the block contains a `return` outside nested callables.
    pub returns: bool,
    /// Whether the block contains a `throw` outside nested callables.
    pub throws: bool,
    /// The number of checks, for an `if` that heads an `else if` chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub chain: Option<u32>,
}

/// One occurrence (call, construction, or import) in the shown text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CallLink {
    /// The occurrence block; an arrow for this call leaves from its line.
    pub source: Key,
    /// The definition (or file) that owns the occurrence.
    pub owner: Key,
    /// The relationship.
    pub kind: EdgeKind,
    /// How it resolved.
    pub resolution: Resolution,
    /// The target, when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub target: Option<Key>,
    /// The written callee or specifier.
    pub text: Box<str>,
    /// The module or reason as display text.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Why it is unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reason: Option<Reason>,
    /// Where an external target comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub module: Option<ModuleLabel>,
    /// The underlined range: the written callee, after any nested call that begins it, without
    /// leading `.`, `?`, or whitespace. Absent when a shorter link already covers it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub span: Option<[u32; 2]>,
    /// The line the occurrence starts on.
    pub line: u32,
}

/// Converts byte offsets inside the shown slice to UTF-16 offsets relative to its start.
pub(crate) struct Offsets<'a> {
    path: &'a RepoPath,
    range: ByteRange,
    utf16: Utf16Index,
}

impl<'a> Offsets<'a> {
    fn new(path: &'a RepoPath, range: ByteRange, slice: &str) -> Self {
        Self {
            path,
            range,
            utf16: Utf16Index::new(slice),
        }
    }

    /// The UTF-16 offset of absolute byte `offset`, which must lie in the slice on a character
    /// boundary.
    pub(crate) fn at(&self, offset: u32) -> Result<u32> {
        offset
            .checked_sub(self.range.start)
            .and_then(|relative| self.utf16.to_utf16(relative).ok())
            .ok_or_else(|| {
                self.invalid(ByteRange {
                    start: offset,
                    end: offset,
                })
            })
    }

    /// Whether `range` lies inside the slice.
    pub(crate) fn contains(&self, range: ByteRange) -> bool {
        self.range.contains(range)
    }

    pub(crate) fn invalid(&self, range: ByteRange) -> ReaderError {
        ReaderError::InvalidRange {
            path: self.path.as_str().into(),
            start: range.start,
            end: range.end,
        }
    }
}

/// The render model of block `local_id` of `file`, reading and verifying its source under
/// `root`. Pass the file root (local ID 0) for a whole file.
pub fn code_view(
    root: &Path,
    file: &FileGraph,
    local_id: u32,
    highlighter: Option<&dyn Highlighter>,
    policy: FoldPolicy,
) -> Result<CodeView> {
    let block = file
        .blocks
        .get(local_id as usize)
        .ok_or_else(|| ReaderError::BlockNotFound {
            path: file.file.path.as_str().into(),
            local_id,
        })?;
    let source = match verify_file(root, &file.file.path, file.file.hash) {
        Verified::Current(snapshot) => {
            CodeSource::Current(render(file, local_id, snapshot.text(), highlighter, policy)?)
        }
        Verified::Changed => CodeSource::Changed,
        Verified::Missing => CodeSource::Missing,
    };
    Ok(CodeView {
        key: block.key.clone(),
        path: file.file.path.clone(),
        language: file.file.language,
        kind: block.kind,
        start_line: block.lines.start,
        end_line: block.lines.end,
        source,
    })
}

/// The render model's text part for block `local_id` of `file`, given the file's verified text.
/// Callers must have verified `text` against the facts' hash ([`verify_file`]).
pub fn render(
    file: &FileGraph,
    local_id: u32,
    text: &str,
    highlighter: Option<&dyn Highlighter>,
    policy: FoldPolicy,
) -> Result<CodeText> {
    let path = &file.file.path;
    let block = file
        .blocks
        .get(local_id as usize)
        .ok_or_else(|| ReaderError::BlockNotFound {
            path: path.as_str().into(),
            local_id,
        })?;
    let range = block.range;
    let slice = text
        .get(range.as_usize())
        .ok_or_else(|| ReaderError::InvalidRange {
            path: path.as_str().into(),
            start: range.start,
            end: range.end,
        })?;
    let offsets = Offsets::new(path, range, slice);
    // The stored first line counts lines exactly as the view does, and `text` is the verified
    // text it was counted in, so the whole file need not be scanned again.
    let (lines, dedent) = lines::lines(slice, block.lines.start, &offsets.utf16);
    let spans = highlighter.map_or_else(Vec::new, |highlighter| {
        spans::spans(text, range, &highlighter.highlight(text), &offsets)
    });
    let long = policy == FoldPolicy::Card && lines.len() > LONG_CARD_LINES;
    let outline = outline::outline(file, local_id, text, &offsets, long)?;
    let links = links::links(file, local_id, text, &offsets)?;
    Ok(CodeText {
        text: slice.to_owned(),
        dedent,
        lines,
        spans,
        outline,
        links,
    })
}

#[cfg(test)]
mod tests;
