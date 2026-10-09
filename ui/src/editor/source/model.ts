/**
 * The source tab's render model, as pure functions over `file_view` pages: which lines show (folds
 * hide ranges), which page holds a line, and how one line splits into colored and linked runs.
 * Offsets are UTF-16 offsets from the start of the file; `textStart` is where a page's text begins.
 */

import type { CallLink, CodeLine, CodeSpan, FileView, OutlineItem, TokenClass } from "../../ipc/generated";

/** Lines per `file_view` page. */
export const PAGE_LINES = 200;

/** The zero-based page that holds a one-based line. */
export function pageOf(line: number): number {
  return Math.floor((line - 1) / PAGE_LINES);
}

/** The one-based, inclusive line range of a page, clamped to the file. */
export function pageRange(page: number, lineCount: number): { startLine: number; endLine: number } {
  const startLine = page * PAGE_LINES + 1;
  return { startLine, endLine: Math.max(startLine, Math.min(lineCount, startLine + PAGE_LINES - 1)) };
}

/** A folded range: its first line stays visible, the lines after it up to `endLine` hide. */
export interface FoldedRange {
  readonly startLine: number;
  readonly endLine: number;
}

/**
 * The visible line numbers, in order. Folded ranges may nest or overlap; a line hides when any
 * folded range covers it below its first line.
 */
export function visibleLines(lineCount: number, folded: readonly FoldedRange[]): number[] {
  const ranges = [...folded].filter((range) => range.endLine > range.startLine).sort((a, b) => a.startLine - b.startLine);
  const lines: number[] = [];
  let hiddenUntil = 0;
  let next = 0;
  for (let line = 1; line <= lineCount; line += 1) {
    const visible = line > hiddenUntil;
    if (visible) {
      lines.push(line);
    }
    // A range folds what follows its first line, if that line itself shows.
    while (next < ranges.length && (ranges[next]?.startLine ?? Infinity) <= line) {
      const range = ranges[next];
      if (range && visible && range.startLine === line) {
        hiddenUntil = Math.max(hiddenUntil, range.endLine);
      }
      next += 1;
    }
  }
  return lines;
}

/** One run of a line: plain or colored text, maybe part of a call link. */
export interface Segment {
  readonly start: number;
  readonly text: string;
  readonly token: TokenClass | null;
  readonly link: CallLink | null;
}

/** The index of the first span that ends after `offset` (spans are sorted and don't overlap). */
function firstSpanEndingAfter(spans: readonly CodeSpan[], offset: number): number {
  let low = 0;
  let high = spans.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    const span = spans[middle];
    if (span && span[1] <= offset) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

/** The line's text, without its terminator. */
export function lineText(page: Pick<FileView, "text" | "textStart">, line: CodeLine): string {
  return page.text.slice(line.start - page.textStart, line.end - page.textStart);
}

/** Splits one line into runs at every color and link boundary. */
export function lineSegments(
  page: Pick<FileView, "text" | "textStart" | "spans">,
  line: CodeLine,
  links: readonly CallLink[] = [],
): Segment[] {
  const { start, end } = line;
  if (end <= start) {
    return [];
  }
  const cuts = new Set<number>([start, end]);
  const spans: CodeSpan[] = [];
  for (let index = firstSpanEndingAfter(page.spans, start); index < page.spans.length; index += 1) {
    const span = page.spans[index];
    if (!span || span[0] >= end) {
      break;
    }
    spans.push(span);
    cuts.add(Math.max(start, span[0]));
    cuts.add(Math.min(end, span[1]));
  }
  const linked = links.filter((link) => link.span !== undefined && link.span[0] < end && link.span[1] > start);
  for (const link of linked) {
    const [from, to] = link.span ?? [0, 0];
    cuts.add(Math.max(start, from));
    cuts.add(Math.min(end, to));
  }
  const points = [...cuts].sort((a, b) => a - b);
  const segments: Segment[] = [];
  let spanIndex = 0;
  for (let index = 0; index + 1 < points.length; index += 1) {
    const from = points[index] ?? start;
    const to = points[index + 1] ?? end;
    if (to <= from) {
      continue;
    }
    while (spanIndex < spans.length && (spans[spanIndex]?.[1] ?? Infinity) <= from) {
      spanIndex += 1;
    }
    const span = spans[spanIndex];
    const token = span && span[0] <= from && span[1] >= to ? span[2] : null;
    const link = linked.find((entry) => entry.span !== undefined && entry.span[0] <= from && entry.span[1] >= to) ?? null;
    segments.push({
      start: from,
      text: page.text.slice(from - page.textStart, to - page.textStart),
      token,
      link,
    });
  }
  return segments;
}

/** The foldable outline items that start on a line (outermost first). */
export function foldsStartingAt(outline: readonly OutlineItem[], lineNumber: number): OutlineItem[] {
  return outline
    .filter((item) => item.fold !== undefined && item.startLine === lineNumber && item.endLine > item.startLine)
    .sort((a, b) => b.endLine - a.endLine);
}
