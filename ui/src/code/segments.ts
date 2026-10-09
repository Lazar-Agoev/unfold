/**
 * Splits a range of shown text into runs with one syntax class and at most one call link each.
 * Offsets are UTF-16 code units, which are JavaScript string indices (U-12), so they slice the text
 * directly.
 */

import type { CallLink, CodeSpan, TokenClass } from "../ipc/generated";

export interface Segment {
  readonly start: number;
  readonly end: number;
  readonly token: TokenClass | null;
  /** The index of the link (in the model's `links`) covering this run, if any. */
  readonly link: number | null;
}

/** A link's underlined range with its index; links without a span are left out. */
export interface LinkRange {
  readonly start: number;
  readonly end: number;
  readonly index: number;
}

/** A highlighted span as an object, relative to the shown text. */
export interface SpanRange {
  readonly start: number;
  readonly end: number;
  readonly token: TokenClass;
}

/**
 * The underlined ranges of a model's links, sorted by start. Links never overlap (reader rule).
 * `base` is subtracted from every offset when the shown text doesn't start at 0 (a source tab's page).
 */
export function linkRanges(links: readonly CallLink[], base = 0): LinkRange[] {
  const ranges: LinkRange[] = [];
  links.forEach((link, index) => {
    if (link.span && link.span[1] > link.span[0]) {
      ranges.push({ start: link.span[0] - base, end: link.span[1] - base, index });
    }
  });
  return ranges.sort((a, b) => a.start - b.start || a.end - b.end);
}

/** The model's spans as objects relative to the shown text (computed once per model). */
export function spanRanges(spans: readonly CodeSpan[], base = 0): SpanRange[] {
  return spans.map(([start, end, token]) => ({ start: start - base, end: end - base, token }));
}

/** The first index in a sorted-by-start array whose end is past `offset`. */
function firstEndingAfter(items: readonly { readonly end: number }[], offset: number): number {
  let low = 0;
  let high = items.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    const item = items[middle];
    if (item && item.end <= offset) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

/** The runs of `[start, end)`. `spans` and `links` are sorted and non-overlapping. */
export function segmentRange(start: number, end: number, spans: readonly SpanRange[], links: readonly LinkRange[]): Segment[] {
  if (end <= start) {
    return [];
  }
  const cuts = new Set<number>([start, end]);
  const firstSpan = firstEndingAfter(spans, start);
  for (let index = firstSpan; index < spans.length; index += 1) {
    const span = spans[index];
    if (!span || span.start >= end) {
      break;
    }
    cuts.add(Math.max(start, span.start));
    cuts.add(Math.min(end, span.end));
  }
  const firstLink = firstEndingAfter(links, start);
  for (let index = firstLink; index < links.length; index += 1) {
    const link = links[index];
    if (!link || link.start >= end) {
      break;
    }
    cuts.add(Math.max(start, link.start));
    cuts.add(Math.min(end, link.end));
  }
  const points = [...cuts].sort((a, b) => a - b);
  const segments: Segment[] = [];
  let spanAt = firstSpan;
  let linkAt = firstLink;
  for (let index = 0; index + 1 < points.length; index += 1) {
    const from = points[index] ?? start;
    const to = points[index + 1] ?? end;
    while (spanAt < spans.length && (spans[spanAt]?.end ?? 0) <= from) {
      spanAt += 1;
    }
    while (linkAt < links.length && (links[linkAt]?.end ?? 0) <= from) {
      linkAt += 1;
    }
    const span = spans[spanAt];
    const link = links[linkAt];
    segments.push({
      start: from,
      end: to,
      token: span && span.start <= from ? span.token : null,
      link: link && link.start <= from ? link.index : null,
    });
  }
  return segments;
}

/** Consecutive runs of one link, grouped so each link renders as one element. */
export interface SegmentGroup {
  readonly link: number | null;
  readonly segments: readonly Segment[];
}

export function groupByLink(segments: readonly Segment[]): SegmentGroup[] {
  const groups: { link: number | null; segments: Segment[] }[] = [];
  for (const segment of segments) {
    const last = groups.at(-1);
    if (last && last.link !== null && last.link === segment.link) {
      last.segments.push(segment);
    } else {
      groups.push({ link: segment.link, segments: [segment] });
    }
  }
  return groups;
}
