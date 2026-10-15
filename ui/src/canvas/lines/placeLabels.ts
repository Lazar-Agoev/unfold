/**
 * Where the labels of a page's lines sit: connections' chips. A label never covers a card or another
 * label: it tries points along its own line (`alongLine`: the middle of its longest straight run, then
 * out from it, then along the next longest), then small steps above and below them into free canvas,
 * then hanging beside those points (to the right, then the left, level or a step above or below), and
 * takes the first spot that meets nothing. When none fits, it tries its shorter forms (a chip without
 * its label, then without what it carries) the same way; when even those don't fit, it isn't drawn at
 * rest. Pure: the same labels and cards always give the same placements, whatever order they come in.
 */

import { alongLine, lineMiddle } from "./paths";

/** A rectangle in canvas units, from its top left. */
export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

/** One line's label: its line's points and the width of each of its forms, longest first, all `height` high. */
export interface LabelSpec {
  readonly id: string;
  readonly points: readonly Point[];
  readonly widths: readonly number[];
  readonly height: number;
}

/** Where a label is drawn: its center, and which of its forms. */
export interface Placement {
  readonly x: number;
  readonly y: number;
  readonly form: number;
}

/** Vertical steps tried above, then below, each of those points. */
const NUDGES: readonly number[] = [12, 24, 36];
/** Space kept between a label and a card or another label. */
const LABEL_GAP = 2;
/** How far a hanging label's near edge stays from its point on the line. */
const HANG_GAP = 4;

function meets(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width + LABEL_GAP && b.x < a.x + a.width + LABEL_GAP && a.y < b.y + b.height + LABEL_GAP && b.y < a.y + a.height + LABEL_GAP;
}

/**
 * The centers a label `width` wide tries, in order: centered on points along the line, then nudged
 * above and below them; then hanging beside each point (its left edge just right of it, then its
 * right edge just left of it), level with it and then nudged above and below. A 20px label nudged
 * 12px has its edge 2px from the point, so a hanging label sits just above or below the line there.
 */
function candidates(points: readonly Point[], width: number): Point[] {
  const along = alongLine(points);
  const out = [...along];
  for (const step of NUDGES) {
    for (const sign of [-1, 1]) {
      out.push(...along.map((point) => ({ x: point.x, y: point.y + sign * step })));
    }
  }
  const levels = [0, ...NUDGES.flatMap((step) => [-step, step])];
  const hang = HANG_GAP + width / 2;
  for (const point of along) {
    for (const level of levels) {
      out.push({ x: point.x + hang, y: point.y + level }, { x: point.x - hang, y: point.y + level });
    }
  }
  return out;
}

/** The first form and spot of `label` that meets nothing in `taken` (which then takes it), or null. */
function fit(label: LabelSpec, taken: Rect[]): Placement | null {
  for (const [form, width] of label.widths.entries()) {
    for (const spot of candidates(label.points, width)) {
      const box = { x: spot.x - width / 2, y: spot.y - label.height / 2, width, height: label.height };
      if (!taken.some((other) => meets(box, other))) {
        taken.push(box);
        return { x: spot.x, y: spot.y, form };
      }
    }
  }
  return null;
}

/** Each label's placement, or null when no form of it fits anywhere near its line. */
export function placeLabels(labels: readonly LabelSpec[], cards: readonly Rect[]): Map<string, Placement | null> {
  const mid = (label: LabelSpec) => lineMiddle(label.points);
  const order = [...labels].sort((a, b) => {
    const pa = mid(a);
    const pb = mid(b);
    return pa.y - pb.y || pa.x - pb.x || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  });
  const taken: Rect[] = [...cards];
  const placements = new Map<string, Placement | null>();
  for (const label of order) {
    placements.set(label.id, fit(label, taken));
  }
  return placements;
}
