/**
 * The peek sheet's render model, as pure functions over `file_view` pages: which page holds a line,
 * which lines show (folds hide ranges), which outline items are definitions (they get `⊕` and a
 * fold), where text matches fall on a line, and the match ticks beside the scrollbar. Offsets are
 * UTF-16 offsets from the start of the file; a page's text begins at `textStart`.
 */

import type { BlockKind, OutlineItem, TextMatch } from "../ipc/generated";

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

/** The block kinds that are definitions: only they get `⊕` and a fold. */
const DEFINITION_KINDS: ReadonlySet<BlockKind> = new Set(["function", "class", "method", "constructor", "getter", "setter"]);

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

/** A highlighted text match on a line, relative to the line's start. */
export interface LineMark {
  readonly start: number;
  readonly end: number;
  readonly current: boolean;
}

/** The text matches on `line`, from the matches the sheet was opened with (`current` is their index). */
export function marksOn(line: number, matches: readonly TextMatch[], current: number | undefined): LineMark[] {
  const marks: LineMark[] = [];
  matches.forEach((match, index) => {
    if (match.line !== line) {
      return;
    }
    for (const [start, end] of match.ranges) {
      marks.push({ start: match.column + start, end: match.column + end, current: index === current });
    }
  });
  return marks.sort((a, b) => a.start - b.start);
}

/** A run of a line's text with at most one mark. */
interface MarkedRun {
  readonly start: number;
  readonly end: number;
  readonly mark: "current" | "other" | null;
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
      runs.push({ start: at, end: from, mark: null });
    }
    runs.push({ start: from, end: to, mark: mark.current ? "current" : "other" });
    at = to;
  }
  if (at < end) {
    runs.push({ start: at, end, mark: null });
  }
  return runs;
}

/** A tick beside the scrollbar: where a matching line sits among the visible rows, from 0 to 1. */
interface MatchTick {
  readonly line: number;
  readonly at: number;
  readonly current: boolean;
}

export function matchTicks(rows: readonly number[], matches: readonly TextMatch[], current: number | undefined): MatchTick[] {
  if (rows.length === 0) {
    return [];
  }
  const indexOf = new Map(rows.map((line, index) => [line, index]));
  const ticks = new Map<number, MatchTick>();
  matches.forEach((match, index) => {
    const row = indexOf.get(match.line);
    if (row !== undefined) {
      const isCurrent = index === current || ticks.get(match.line)?.current === true;
      ticks.set(match.line, { line: match.line, at: row / rows.length, current: isCurrent });
    }
  });
  return [...ticks.values()];
}
