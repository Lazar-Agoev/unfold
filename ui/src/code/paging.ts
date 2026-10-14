/** A long file is read in pages of 200 lines: which lines a page covers. */

/** Lines per page. */
export const PAGE_LINES = 200;

/** The one-based, inclusive line range of a page, clamped to the file. */
export function pageRange(page: number, lineCount: number): { startLine: number; endLine: number } {
  const startLine = page * PAGE_LINES + 1;
  return { startLine, endLine: Math.max(startLine, Math.min(lineCount, startLine + PAGE_LINES - 1)) };
}
