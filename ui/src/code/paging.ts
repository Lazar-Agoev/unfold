/** A long file is read in pages of 200 lines: which page holds a line, and which lines a page covers. */

/** Lines per page. */
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
