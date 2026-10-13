/** Match highlighting in Add's rows. Offsets are UTF-16, as the engine sends them. */

import type { ReactNode } from "react";

/** A `[start, end)` range of a string. */
export type TextRange = readonly [number, number];

/**
 * `text` with each range (clamped, overlaps merged) wrapped in a `<mark>` of `className`: text matches
 * get the tinted bold run.
 */
export function highlightRanges(text: string, ranges: readonly TextRange[], className: string): ReactNode {
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
      <mark key={start} className={className}>
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
