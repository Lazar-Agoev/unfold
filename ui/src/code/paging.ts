/** A view's lines are read in pages of 200, counted from its first line: which lines a page covers. */

/** Lines per page. */
export const PAGE_LINES = 200;

/** The first and last line of the view whose lines are `span`, first and last. */
export type LineSpan = readonly [number, number];

/** The pages of the view `span`, in order. */
export function pagesOf([first, last]: LineSpan): number[] {
  return Array.from({ length: Math.max(0, Math.ceil((last - first + 1) / PAGE_LINES)) }, (_, page) => page);
}

/** The first and last line of page `page` of the view `span`, clamped to it. */
export function pageLines(page: number, [first, last]: LineSpan): LineSpan {
  const start = first + page * PAGE_LINES;
  return [start, Math.max(start, Math.min(last, start + PAGE_LINES - 1))];
}
