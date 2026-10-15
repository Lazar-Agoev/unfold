/**
 * Lists a tool pages together with one cursor, such as `search`'s kinds or `get_relations`' callers
 * and calls: every list shows the same stretch from the cursor's offset, at most as many items as
 * fit in one result, so the size guard never cuts the line that says where the next page starts.
 */

import { MAX_RESULT_CHARS, RESULT_ROOM } from "./results";

/** One page of one list: its heading and lines, and whether the list goes on past it. */
export interface Section {
  readonly lines: readonly string[];
  readonly more: boolean;
}

/**
 * The page of a list from `offset`, at most `size` items of `items` (the list from `offset` on), under
 * `title` with what it shows of `total`; nothing when the list is empty. Paging stops at `end`, for a
 * list whose source reaches only so far.
 */
export function section<T>(title: string, items: readonly T[], total: number, offset: number, size: number, line: (item: T) => string, end = total): Section {
  if (total === 0) {
    return { lines: [], more: false };
  }
  const shown = items.slice(0, size);
  const range = shown.length === 0 ? `none past ${offset} of ${total}` : shown.length < total ? `${offset + 1}–${offset + shown.length} of ${total}` : `${total}`;
  return { lines: [`${title} (${range}):`, ...shown.map(line)], more: offset + size < end };
}

/** The line that says how to get the next page, when any list has more. */
export function moreLine(sections: readonly Section[], tool: string, next: number): string | null {
  return sections.some((item) => item.more) ? `More: ${tool} with cursor "${next}" for the next page.` : null;
}

/** The characters `lines` take in a result, one line each. */
export function linesLength(lines: readonly (string | null | undefined | false)[]): number {
  return lines.reduce((chars, line) => (typeof line === "string" ? chars + line.length + 1 : chars), 0);
}

/**
 * The sections of the largest page, at most `limit` items per list (at least 1), whose lines fit in a
 * result beside `room` characters of other lines. Every list shows the same number, so one cursor
 * pages them all.
 */
export function fitting<T extends Pick<Section, "lines">>(limit: number, room: number, build: (size: number) => readonly T[]): { readonly sections: readonly T[]; readonly limit: number } {
  const budget = MAX_RESULT_CHARS - RESULT_ROOM - room;
  for (let size = limit; size > 1; size -= 1) {
    const sections = build(size);
    if (linesLength(sections.flatMap((item) => item.lines)) <= budget) {
      return { sections, limit: size };
    }
  }
  return { sections: build(1), limit: 1 };
}
