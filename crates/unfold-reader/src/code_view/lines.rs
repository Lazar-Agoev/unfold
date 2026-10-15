//! The shown lines: cropping, dedenting, highlighting, each line's block and call names, and
//! the marked range, all in UTF-16 columns of each line's shown text.
//!
//! Lines end where the stored line numbers end them (core's `LineIndex`, with the file's
//! `LineBreaks`), so line `n` of the view is line `n` of the file. A terminator at the very end
//! of the file does not start an extra empty line. The view drops the indentation every
//! nonblank line of it shares: a method loses its class's indentation.
//!
//! An answer holds a **window** of the view: the lines asked for, at most [`MAX_WINDOW`], or by
//! default a page from a little above the first line the selection marks. Everything found per
//! line (its tokens, its block and calls, the marked expression, the definitions) is found for
//! the window's lines only, from the highlighting and the nodes that reach them, so an answer
//! costs about the same in a file of any size. A line longer than [`MAX_LINE_BYTES`] is cut
//! there.

use unfold_core::highlight::HighlightSpan;
use unfold_core::vocab::{Form, NodeKind};
use unfold_core::{ByteRange, LineIndex, RepoPath, Utf16Index};

use super::select::{MarkSpec, Span};
use super::{CallName, CodeLine, DefinitionLine, LineRange, MAX_WINDOW, Mark, Token};
use crate::error::{ReaderError, Result};
use crate::tree::{FileTree, is_expression, to_u32};

/// The lines an answer holds when it asks for none.
const PAGE: u32 = 200;
/// How many lines above the first marked line that page starts.
const ABOVE: u32 = PAGE / 4;
/// The most bytes of one line's shown text an answer holds.
const MAX_LINE_BYTES: u32 = 1_000;

/// A file's text with its line and UTF-16 indexes.
#[derive(Clone, Copy)]
pub(super) struct Text<'a> {
    text: &'a str,
    lines: &'a LineIndex,
    utf16: &'a Utf16Index,
}

impl<'a> Text<'a> {
    pub(super) fn new(text: &'a str, lines: &'a LineIndex, utf16: &'a Utf16Index) -> Self {
        Self { text, lines, utf16 }
    }

    fn len(&self) -> u32 {
        to_u32(self.text.len())
    }

    /// The last line that holds text: a terminator at the very end starts no line.
    fn last_line(&self) -> u32 {
        self.lines
            .span(ByteRange {
                start: 0,
                end: self.len(),
            })
            .end
    }

    /// Where line `number` starts, and where it ends before its terminator.
    fn line_bounds(&self, number: u32) -> (u32, u32) {
        let start = self.lines.line_start(number).unwrap_or_else(|| self.len());
        let Some(next) = self.lines.line_start(number + 1) else {
            return (start, self.len());
        };
        let line = &self.text[start as usize..next as usize];
        let terminator = ["\r\n", "\n", "\r", "\u{2028}", "\u{2029}"]
            .iter()
            .find(|terminator| line.ends_with(*terminator))
            .map_or(0, |terminator| terminator.len());
        (start, next - to_u32(terminator))
    }

    /// Line `number`'s indentation (its leading spaces and tabs), and whether it is blank.
    fn indent(&self, number: u32) -> (u32, bool) {
        let (start, end) = self.line_bounds(number);
        let line = &self.text[start as usize..end as usize];
        let rest = line.trim_start_matches([' ', '\t']);
        (to_u32(line.len() - rest.len()), rest.trim().is_empty())
    }

    /// The first line `mark` marks, if it marks any.
    fn marked_line(&self, mark: &MarkSpec) -> Option<u32> {
        match (mark.lines, mark.expression) {
            (Some(lines), _) => self.lines_of(lines).ok().map(|[start, _]| start),
            (None, Some(range)) => Some(self.lines.line_of(range.start)),
            (None, None) => None,
        }
    }

    /// The first and last line of `span`, inside the file; the range of a span that doesn't fit
    /// the text.
    fn lines_of(&self, span: Span) -> std::result::Result<[u32; 2], ByteRange> {
        let last = self.last_line();
        Ok(match span {
            Span::Whole => [1, last],
            Span::Bytes(range) => {
                if self.text.get(range.as_usize()).is_none() {
                    return Err(range);
                }
                let span = self.lines.span(range);
                [span.start, span.end]
            }
            Span::Lines([first, end]) => {
                let end = end.clamp(1, last);
                [first.clamp(1, end), end]
            }
        })
    }
}

/// One shown line's bounds.
struct Row {
    number: u32,
    /// Where the line starts in the file.
    start: u32,
    /// Where its shown text starts, after the shared indentation.
    shown: u32,
    /// Where its shown text ends: before its terminator, or where a long line is cut.
    end: u32,
    /// Whether the line is cut.
    cut: bool,
}

/// The window of one view of a file.
pub(super) struct Render<'a> {
    text: Text<'a>,
    path: &'a RepoPath,
    /// The view's first and last line.
    span: [u32; 2],
    /// The window's lines.
    rows: Vec<Row>,
}

impl<'a> Render<'a> {
    /// The lines `view` covers, and the rows of `window` among them: at most [`MAX_WINDOW`] of
    /// them, by default a page from [`ABOVE`] lines above the first line `mark` marks (the
    /// view's first line when it marks none), kept inside the view.
    pub(super) fn new(
        text: Text<'a>,
        path: &'a RepoPath,
        view: Span,
        mark: &MarkSpec,
        window: Option<[u32; 2]>,
    ) -> Result<Self> {
        let [first, last] = text.lines_of(view).map_err(|range| invalid(path, range))?;
        let [from, to] = match window {
            Some([from, to]) => {
                let from = from.max(first);
                [from, to.min(last).min(from.saturating_add(MAX_WINDOW - 1))]
            }
            None => {
                let anchor = text.marked_line(mark).unwrap_or(first).clamp(first, last);
                let from = anchor
                    .saturating_sub(ABOVE)
                    .min(last.saturating_sub(PAGE - 1))
                    .max(first);
                [from, last.min(from + PAGE - 1)]
            }
        };
        // The indentation every nonblank line of the view shares: none once one has none.
        let mut dedent: Option<u32> = None;
        for number in first..=last {
            let (indent, blank) = text.indent(number);
            if !blank {
                dedent = Some(dedent.map_or(indent, |shared| shared.min(indent)));
                if indent == 0 {
                    break;
                }
            }
        }
        let dedent = dedent.unwrap_or(0);
        let rows = (from..=to)
            .map(|number| {
                let (start, end) = text.line_bounds(number);
                let shown = start + dedent.min(text.indent(number).0);
                let cut = end - shown > MAX_LINE_BYTES;
                let end = if cut {
                    to_u32(text.text.floor_char_boundary((shown + MAX_LINE_BYTES) as usize))
                } else {
                    end
                };
                Row {
                    number,
                    start,
                    shown,
                    end,
                    cut,
                }
            })
            .collect();
        Ok(Self {
            text,
            path,
            span: [first, last],
            rows,
        })
    }

    /// The view's first and last line.
    pub(super) fn span(&self) -> [u32; 2] {
        self.span
    }

    /// The first and last line of `span`, inside the file.
    fn lines_of(&self, span: Span) -> Result<[u32; 2]> {
        self.text
            .lines_of(span)
            .map_err(|range| invalid(self.path, range))
    }

    /// The bytes the window's lines take, terminators included.
    fn bytes(&self) -> Option<(u32, u32)> {
        let (first, last) = (self.rows.first()?, self.rows.last()?);
        let end = self
            .text
            .lines
            .line_start(last.number + 1)
            .unwrap_or_else(|| self.text.len());
        Some((first.start, end))
    }

    /// The window's lines, highlighted by `highlights` (the whole text's), with their blocks
    /// and call names when the view is of `tree`'s verified source.
    pub(super) fn lines(
        &self,
        tree: Option<&FileTree>,
        highlights: Option<&[HighlightSpan]>,
    ) -> Result<Vec<CodeLine>> {
        let mut lines: Vec<CodeLine> = self
            .rows
            .iter()
            .map(|row| CodeLine {
                number: row.number,
                text: self.text.text[row.shown as usize..row.end as usize].to_owned(),
                tokens: Vec::new(),
                block: None,
                calls: Vec::new(),
                cut: row.cut,
            })
            .collect();
        if let Some(spans) = highlights {
            self.tokens(spans, &mut lines)?;
        }
        if let Some(tree) = tree {
            self.blocks(tree, &mut lines);
            self.calls(tree, &mut lines)?;
        }
        Ok(lines)
    }

    /// Clips `spans` (well formed, over the whole text) to each line.
    fn tokens(&self, spans: &[HighlightSpan], lines: &mut [CodeLine]) -> Result<()> {
        let mut next = 0;
        for (row, line) in self.rows.iter().zip(lines) {
            next += spans[next..].partition_point(|span| span.range.end <= row.shown);
            for span in spans[next..].iter().take_while(|span| span.range.start < row.end) {
                let (start, end) = (span.range.start.max(row.shown), span.range.end.min(row.end));
                if start < end {
                    line.tokens.push(Token(
                        self.column(row, start)?,
                        self.column(row, end)?,
                        span.class,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Each line's block: the innermost block that covers it, except that a block starting on
    /// the same line as the block holding it leaves the line to that block (the statement's
    /// first block). One sweep over the blocks that reach the window, in preorder: each takes
    /// the lines it covers from the block holding them.
    fn blocks(&self, tree: &FileTree, lines: &mut [CodeLine]) {
        let (Some((start, end)), Some(first), Some(last)) =
            (self.bytes(), self.rows.first(), self.rows.last())
        else {
            return;
        };
        let mut best: Vec<Option<(u32, u32)>> = vec![None; lines.len()];
        for id in 0..to_u32(tree.nodes().len()) {
            let Some(extent) =
                hover_extent(tree, id).filter(|extent| extent.start < end && extent.end >= start)
            else {
                continue;
            };
            let span = self.text.lines.span(extent);
            if span.end < first.number || span.start > last.number {
                continue;
            }
            let from = span.start.max(first.number) - first.number;
            let to = span.end.min(last.number) - first.number;
            for held in &mut best[from as usize..=to as usize] {
                if held
                    .is_none_or(|(outer, outer_start)| tree.contains(outer, id) && span.start > outer_start)
                {
                    *held = Some((id, span.start));
                }
            }
        }
        for (line, best) in lines.iter_mut().zip(best) {
            line.block = best.map(|(id, _)| tree.key(id).clone());
        }
    }

    /// Each call's written name on the window's lines it is written on.
    fn calls(&self, tree: &FileTree, lines: &mut [CodeLine]) -> Result<()> {
        let Some((start, end)) = self.bytes() else {
            return Ok(());
        };
        for id in 0..to_u32(tree.nodes().len()) {
            let reaches = |head: ByteRange| head.start < end && head.end > start;
            if !tree.node(id).head.is_some_and(reaches) {
                continue;
            }
            let Some(name) = call_name(tree, id, self.text.text) else {
                continue;
            };
            for range in self.ranges(name)? {
                let index = (range.line - self.rows[0].number) as usize;
                lines[index].calls.push(CallName {
                    start: range.start,
                    end: range.end,
                    key: tree.key(id).clone(),
                });
            }
        }
        for line in lines {
            line.calls.sort_by_key(|call| (call.start, call.end));
        }
        Ok(())
    }

    /// The first line of each definition that starts on the window's lines.
    pub(super) fn definitions(&self, tree: &FileTree) -> Vec<DefinitionLine> {
        let Some((start, end)) = self.bytes() else {
            return Vec::new();
        };
        (1..to_u32(tree.nodes().len()))
            .filter(|&id| tree.kind(id).is_definition())
            .filter_map(|id| {
                let first = tree.extent(id).start;
                (start..end).contains(&first).then(|| DefinitionLine {
                    line: self.text.lines.line_of(first),
                    key: tree.key(id).clone(),
                })
            })
            .collect()
    }

    /// `spec` inside the view: the marked lines (all of them the view shows), and the
    /// expression on the window's lines. `None` when nothing of it is shown.
    pub(super) fn mark(&self, spec: &MarkSpec) -> Result<Option<Mark>> {
        let [first, last] = self.span;
        let lines = match spec.lines {
            Some(span) => {
                let [start, end] = self.lines_of(span)?;
                let [start, end] = [start.max(first), end.min(last)];
                (start <= end).then_some([start, end])
            }
            None => None,
        };
        let expression = match spec.expression {
            Some(range) => self.ranges(range)?,
            None => Vec::new(),
        };
        Ok((lines.is_some() || !expression.is_empty()).then_some(Mark { lines, expression }))
    }

    /// The shown parts of `range`, one per line of the window it touches.
    fn ranges(&self, range: ByteRange) -> Result<Vec<LineRange>> {
        if self.text.text.get(range.as_usize()).is_none() {
            return Err(self.invalid(range));
        }
        let mut ranges = Vec::new();
        let first = self.rows.partition_point(|row| row.end < range.start);
        for row in &self.rows[first..] {
            if row.start > range.end {
                break;
            }
            let (start, end) = (range.start.max(row.shown), range.end.min(row.end));
            if start < end {
                ranges.push(LineRange {
                    line: row.number,
                    start: self.column(row, start)?,
                    end: self.column(row, end)?,
                });
            }
        }
        Ok(ranges)
    }

    /// The UTF-16 column of byte `offset` (inside `row`'s shown text) in that text.
    fn column(&self, row: &Row, offset: u32) -> Result<u32> {
        let utf16 = |offset: u32| {
            self.text.utf16.to_utf16(offset).map_err(|_| {
                self.invalid(ByteRange {
                    start: offset,
                    end: offset,
                })
            })
        };
        Ok(utf16(offset)? - utf16(row.shown)?)
    }

    fn invalid(&self, range: ByteRange) -> ReaderError {
        invalid(self.path, range)
    }
}

/// A stored range of `path` that doesn't fit its verified text.
fn invalid(path: &RepoPath, range: ByteRange) -> ReaderError {
    ReaderError::InvalidRange {
        path: path.as_str().into(),
        start: range.start,
        end: range.end,
    }
}

/// The lines hovering outlines block `id` for: a statement's first block (and an expression
/// control block that begins one) covers its statement; a control block or part covers the
/// lines that open it; a definition covers its decorators, its header, and its doc. The module,
/// `body` parts, and blocks inside a statement cover nothing of their own.
fn hover_extent(tree: &FileTree, id: u32) -> Option<ByteRange> {
    let node = tree.node(id);
    let opening = |start: u32| ByteRange {
        start,
        end: [node.head, node.doc]
            .into_iter()
            .flatten()
            .filter(|range| range.start >= start)
            .map(|range| range.end)
            .fold(start, u32::max),
    };
    match node.kind() {
        NodeKind::Module => None,
        NodeKind::Class | NodeKind::Function | NodeKind::Type => Some(opening(tree.extent(id).start)),
        NodeKind::Decision | NodeKind::Loop if is_expression(node.form) => node.stmt,
        NodeKind::Part if node.form == Form::Body => None,
        NodeKind::Decision | NodeKind::Loop | NodeKind::Try | NodeKind::With | NodeKind::Part => {
            Some(opening(node.range.start))
        }
        NodeKind::Call | NodeKind::Exit => node.stmt,
    }
}

/// Call `id`'s written name: its callee, after any call its callee begins with
/// (`res.status(200).json` is named at `json`), without the `.` or `?.` before it.
fn call_name(tree: &FileTree, id: u32, text: &str) -> Option<ByteRange> {
    let node = tree.node(id);
    if node.kind() != NodeKind::Call {
        return None;
    }
    let head = node.head?;
    let after = tree
        .children(id)
        .iter()
        .map(|&child| tree.node(child).range)
        .filter(|range| range.start == head.start && range.end <= head.end)
        .map(|range| range.end)
        .fold(head.start, u32::max);
    let rest = text.get(after as usize..head.end as usize)?;
    let start = after
        + to_u32(
            rest.len()
                - rest
                    .trim_start_matches(['.', '?', '!', ' ', '\t', '\n', '\r'])
                    .len(),
        );
    (start < head.end).then_some(ByteRange { start, end: head.end })
}
