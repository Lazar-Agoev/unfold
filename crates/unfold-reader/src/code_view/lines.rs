//! The shown lines: cropping, dedenting, highlighting, each line's block and call names, and
//! the marked range, all in UTF-16 columns of each line's shown text.
//!
//! Lines end where the stored line numbers end them (core's `LineIndex`, with the file's
//! `LineBreaks`), so line `n` of the view is line `n` of the file. A terminator at the very end
//! of the file does not start an extra empty line. The view drops the indentation every
//! nonblank shown line shares: a method loses its class's indentation.

use unfold_core::highlight::{HighlightSpan, Highlighter};
use unfold_core::vocab::{Form, NodeKind};
use unfold_core::{ByteRange, LineBreaks, LineIndex, RepoPath, Utf16Index};

use super::select::{MarkSpec, Span};
use super::{CallName, CodeLine, DefinitionLine, LineRange, Mark, Token};
use crate::error::{ReaderError, Result};
use crate::tree::{FileTree, is_expression, to_u32};

/// A file's text with its line and UTF-16 indexes.
pub(super) struct Text<'a> {
    text: &'a str,
    lines: LineIndex,
    utf16: Utf16Index,
}

impl<'a> Text<'a> {
    pub(super) fn new(text: &'a str, breaks: LineBreaks) -> Self {
        Self {
            text,
            lines: LineIndex::new(text, breaks),
            utf16: Utf16Index::new(text),
        }
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
}

/// One shown line's bounds.
struct Row {
    number: u32,
    /// Where the line starts in the file.
    start: u32,
    /// Where its shown text starts, after the shared indentation.
    shown: u32,
    /// Where it ends, before its terminator.
    end: u32,
}

/// The rows of one view of a file.
pub(super) struct Render<'a> {
    text: &'a Text<'a>,
    path: &'a RepoPath,
    rows: Vec<Row>,
}

impl<'a> Render<'a> {
    /// The lines `view` covers.
    pub(super) fn new(text: &'a Text<'a>, path: &'a RepoPath, view: Span) -> Result<Self> {
        let mut render = Self {
            text,
            path,
            rows: Vec::new(),
        };
        let [first, last] = render.lines_of(view)?;
        let bounds: Vec<(u32, u32, u32)> = (first..=last)
            .map(|number| {
                let (start, end) = text.line_bounds(number);
                (number, start, end)
            })
            .collect();
        let indent = |start: u32, end: u32| {
            let line = &text.text[start as usize..end as usize];
            to_u32(line.len() - line.trim_start_matches([' ', '\t']).len())
        };
        let blank = |start: u32, end: u32| text.text[start as usize..end as usize].trim().is_empty();
        let dedent = bounds
            .iter()
            .filter(|&&(_, start, end)| !blank(start, end))
            .map(|&(_, start, end)| indent(start, end))
            .min()
            .unwrap_or(0);
        render.rows = bounds
            .into_iter()
            .map(|(number, start, end)| Row {
                number,
                start,
                shown: start + dedent.min(indent(start, end)),
                end,
            })
            .collect();
        Ok(render)
    }

    /// The first and last line of `span`, inside the file.
    fn lines_of(&self, span: Span) -> Result<[u32; 2]> {
        let last = self.text.last_line();
        Ok(match span {
            Span::Whole => [1, last],
            Span::Bytes(range) => {
                if self.text.text.get(range.as_usize()).is_none() {
                    return Err(self.invalid(range));
                }
                let span = self.text.lines.span(range);
                [span.start, span.end]
            }
            Span::Lines([first, end]) => {
                let end = end.clamp(1, last);
                [first.clamp(1, end), end]
            }
        })
    }

    /// The shown lines, highlighted by `highlighter`, with their blocks and call names when the
    /// view is of `tree`'s verified source.
    pub(super) fn lines(
        &self,
        tree: Option<&FileTree>,
        highlighter: Option<&dyn Highlighter>,
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
            })
            .collect();
        if let Some(highlighter) = highlighter {
            self.tokens(&highlighter.highlight(self.text.text), &mut lines)?;
        }
        if let Some(tree) = tree {
            self.blocks(tree, &mut lines);
            self.calls(tree, &mut lines)?;
        }
        Ok(lines)
    }

    /// Clips `spans` (over the whole text) to each line. Spans that are not well formed are
    /// dropped rather than trusted.
    fn tokens(&self, spans: &[HighlightSpan], lines: &mut [CodeLine]) -> Result<()> {
        let text = self.text.text;
        let mut previous_end = 0u32;
        let spans: Vec<&HighlightSpan> = spans
            .iter()
            .filter(|span| {
                let ByteRange { start, end } = span.range;
                let fits = start >= previous_end
                    && start < end
                    && (end as usize) <= text.len()
                    && text.is_char_boundary(start as usize)
                    && text.is_char_boundary(end as usize);
                if fits {
                    previous_end = end;
                }
                fits
            })
            .collect();
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
    /// first block).
    fn blocks(&self, tree: &FileTree, lines: &mut [CodeLine]) {
        let (Some(first), Some(last)) = (self.rows.first(), self.rows.last()) else {
            return;
        };
        let candidates: Vec<(u32, u32, u32)> = (0..to_u32(tree.nodes().len()))
            .filter_map(|id| {
                let extent = hover_extent(tree, id)?;
                let span = self.text.lines.span(extent);
                (span.start <= last.number && first.number <= span.end).then_some((id, span.start, span.end))
            })
            .collect();
        for line in lines {
            let mut best: Option<(u32, u32)> = None;
            for &(id, start, end) in &candidates {
                if !(start..=end).contains(&line.number) {
                    continue;
                }
                let inner =
                    best.is_none_or(|(held, held_start)| tree.contains(held, id) && start > held_start);
                if inner {
                    best = Some((id, start));
                }
            }
            line.block = best.map(|(id, _)| tree.key(id).clone());
        }
    }

    /// Each call's written name on the lines it is written on.
    fn calls(&self, tree: &FileTree, lines: &mut [CodeLine]) -> Result<()> {
        for id in 0..to_u32(tree.nodes().len()) {
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

    /// The first line of each definition the view shows.
    pub(super) fn definitions(&self, tree: &FileTree) -> Vec<DefinitionLine> {
        let (Some(first), Some(last)) = (self.rows.first(), self.rows.last()) else {
            return Vec::new();
        };
        (1..to_u32(tree.nodes().len()))
            .filter(|&id| tree.kind(id).is_definition())
            .filter_map(|id| {
                let line = self.text.lines.line_of(tree.extent(id).start);
                (first.number..=last.number)
                    .contains(&line)
                    .then(|| DefinitionLine {
                        line,
                        key: tree.key(id).clone(),
                    })
            })
            .collect()
    }

    /// `spec` inside the view; `None` when nothing of it is shown.
    pub(super) fn mark(&self, spec: &MarkSpec) -> Result<Option<Mark>> {
        let (Some(first), Some(last)) = (self.rows.first(), self.rows.last()) else {
            return Ok(None);
        };
        let lines = match spec.lines {
            Some(span) => {
                let [start, end] = self.lines_of(span)?;
                let [start, end] = [start.max(first.number), end.min(last.number)];
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

    /// The shown parts of `range`, one per line it touches.
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
        ReaderError::InvalidRange {
            path: self.path.as_str().into(),
            start: range.start,
            end: range.end,
        }
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
