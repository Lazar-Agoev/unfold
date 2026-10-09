/** Match highlighting for search results. Offsets are UTF-16, as the engine sends them. */

import type { ReactNode } from "react";

/** Merges sorted character offsets into `[start, end)` runs. */
export function offsetsToRanges(offsets: readonly number[]): [number, number][] {
  const sorted = [...offsets].sort((a, b) => a - b);
  const ranges: [number, number][] = [];
  for (const offset of sorted) {
    const last = ranges.at(-1);
    if (last && offset <= last[1]) {
      last[1] = Math.max(last[1], offset + 1);
    } else {
      ranges.push([offset, offset + 1]);
    }
  }
  return ranges;
}

/** `text` with `<mark>` around each `[start, end)` range (clamped, overlaps merged). */
export function highlightRanges(text: string, ranges: readonly (readonly [number, number])[]): ReactNode {
  if (ranges.length === 0) {
    return text;
  }
  const parts: ReactNode[] = [];
  let at = 0;
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  for (const [rawStart, rawEnd] of sorted) {
    const start = Math.max(at, Math.min(text.length, rawStart));
    const end = Math.max(start, Math.min(text.length, rawEnd));
    if (end <= start) {
      continue;
    }
    if (start > at) {
      parts.push(text.slice(at, start));
    }
    parts.push(
      <mark key={start} className="match">
        {text.slice(start, end)}
      </mark>,
    );
    at = end;
  }
  if (at < text.length) {
    parts.push(text.slice(at));
  }
  return parts;
}

/** `text` with the characters at `offsets` highlighted. */
export function highlightChars(text: string, offsets: readonly number[]): ReactNode {
  return highlightRanges(text, offsetsToRanges(offsets));
}
