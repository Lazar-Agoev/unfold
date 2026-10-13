/**
 * The peek sheet's render model, as pure functions over `file_view` pages: which page holds a line,
 * which lines show (folds hide ranges), which outline items are definitions (they get `⊕` and a
 * fold), and View code's focus (its rows, its exact expression on each row, and whether the file may
 * show it). Offsets are
 * UTF-16 offsets from the start of the file; a page's text begins at `textStart`.
 */

import type { BlockKind, GraphMatch, OutlineItem } from "../ipc/generated";
import type { PeekFocus } from "../state/overlays";

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

/** The block kinds that are definitions (the catalog's): only they get `⊕` and a fold. */
const DEFINITION_KINDS: ReadonlySet<BlockKind> = new Set(["function", "class", "method", "constructor", "getter", "setter", "interface", "type_alias", "enum"]);

function isDefinition(item: Pick<OutlineItem, "kind">): boolean {
  return DEFINITION_KINDS.has(item.kind);
}

/** The definition that starts on a line, outermost first (a line can open a class and its first method). */
export function definitionAt(outline: readonly OutlineItem[], lineNumber: number): OutlineItem | undefined {
  return outline
    .filter((item) => isDefinition(item) && item.startLine === lineNumber)
    .sort((a, b) => b.endLine - a.endLine)[0];
}

/** Whether a definition can fold: it spans more than its first line and the engine gives it a fold. */
export function canFold(item: OutlineItem): boolean {
  return item.fold !== undefined && item.endLine > item.startLine;
}

/** View code's expression on a line, relative to the line's start. */
export interface LineMark {
  readonly start: number;
  readonly end: number;
}

/** A run of a line's text, inside a mark or not. */
interface MarkedRun {
  readonly start: number;
  readonly end: number;
  readonly marked: boolean;
}

/** Cuts `[start, end)` (offsets in the line) at mark boundaries. */
export function markRuns(start: number, end: number, marks: readonly LineMark[]): MarkedRun[] {
  const runs: MarkedRun[] = [];
  let at = start;
  for (const mark of marks) {
    const from = Math.max(at, mark.start);
    const to = Math.min(end, mark.end);
    if (to <= from) {
      continue;
    }
    if (from > at) {
      runs.push({ start: at, end: from, marked: false });
    }
    runs.push({ start: from, end: to, marked: true });
    at = to;
  }
  if (at < end) {
    runs.push({ start: at, end, marked: false });
  }
  return runs;
}

/**
 * Whether the file may show View code's focus: source the graph was built from (`current`), or an
 * evidence file read as text. A file changed since the compile, or one outside the graph, never shows
 * a range recorded for other text.
 */
export function showsFocus(graph: GraphMatch): boolean {
  return graph === "current" || graph === "evidence";
}

/** Whether `line` is one of the focus's rows. */
export function inFocus(line: number, focus: PeekFocus): boolean {
  return line >= focus.startLine && line <= focus.endLine;
}

/**
 * The focus's expression on `line` (`length` UTF-16 units long), relative to the line's start: from
 * `startColumn` on the first line to `endColumn` on the last, the whole line in between. None without
 * columns, off the focus's rows, or when it would be empty.
 */
export function focusMarkOn(line: number, focus: PeekFocus, length: number): LineMark | null {
  if (focus.startColumn === undefined || focus.endColumn === undefined || !inFocus(line, focus)) {
    return null;
  }
  const start = line === focus.startLine ? Math.min(focus.startColumn, length) : 0;
  const end = line === focus.endLine ? Math.min(focus.endColumn, length) : length;
  return end > start ? { start, end } : null;
}

/** Lines past which a focus no longer fits in the middle of the sheet, so it starts at the top. */
const LONG_FOCUS = 20;

/** The line View code brings to the middle: the focus's middle line, or its first when it is long. */
export function focusAnchor(focus: PeekFocus): { readonly line: number; readonly align: "center" | "start" } {
  const span = focus.endLine - focus.startLine;
  return span <= LONG_FOCUS ? { line: focus.startLine + Math.floor(span / 2), align: "center" } : { line: focus.startLine, align: "start" };
}
