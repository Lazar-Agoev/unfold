/**
 * Where the labels of a page's lines sit: connections' chips. A label never
 * covers a card or another label: it tries points along its own curve (the midpoint, then 0.4 and
 * 0.6, 0.3 and 0.7, …), then small steps above and below them into free canvas, then hanging beside
 * those points (to the right, then the left, level or a step above or below), and takes the first
 * spot that meets nothing. When none fits, it tries its shorter forms (a chip without its label, then
 * without what it carries) the same way; when even those don't fit, it isn't drawn at rest. Pure: the same
 * labels and cards always give the same placements, whatever order they come in.
 */

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

/** A cubic Bézier curve: its ends and its two control points. */
export interface Curve {
  readonly from: Point;
  readonly c1: Point;
  readonly c2: Point;
  readonly to: Point;
}

/** The side of a card an end leaves or enters by (React Flow's `Position` values). */
export type Side = "left" | "right" | "top" | "bottom";

/** React Flow's default curvature for `getBezierPath`. */
const CURVATURE = 0.25;

function controlOffset(distance: number): number {
  return distance >= 0 ? 0.5 * distance : CURVATURE * 25 * Math.sqrt(-distance);
}

function control(side: Side, x1: number, y1: number, x2: number, y2: number): Point {
  switch (side) {
    case "left":
      return { x: x1 - controlOffset(x1 - x2), y: y1 };
    case "right":
      return { x: x1 + controlOffset(x2 - x1), y: y1 };
    case "top":
      return { x: x1, y: y1 - controlOffset(y1 - y2) };
    case "bottom":
      return { x: x1, y: y1 + controlOffset(y2 - y1) };
  }
}

/** The curve `getBezierPath` draws between two ends (so its `t = 0.5` is React Flow's label point). */
export function bezierCurve(from: Point, fromSide: Side, to: Point, toSide: Side): Curve {
  return { from, c1: control(fromSide, from.x, from.y, to.x, to.y), c2: control(toSide, to.x, to.y, from.x, from.y), to };
}

/** The point at `t` (0 to 1) along a curve. */
export function pointAt(curve: Curve, t: number): Point {
  const u = 1 - t;
  const a = u * u * u;
  const b = 3 * u * u * t;
  const c = 3 * u * t * t;
  const d = t * t * t;
  return {
    x: a * curve.from.x + b * curve.c1.x + c * curve.c2.x + d * curve.to.x,
    y: a * curve.from.y + b * curve.c1.y + c * curve.c2.y + d * curve.to.y,
  };
}

/** One line's label: its curve and the width of each of its forms, longest first, all `height` high. */
export interface LabelSpec {
  readonly id: string;
  readonly curve: Curve;
  readonly widths: readonly number[];
  readonly height: number;
}

/** Where a label is drawn: its center, and which of its forms. */
export interface Placement {
  readonly x: number;
  readonly y: number;
  readonly form: number;
}

/** Points tried along a curve, in order. */
const CURVE_STEPS: readonly number[] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];
/** Vertical steps tried above, then below, each of those points. */
const NUDGES: readonly number[] = [12, 24, 36];
/** Space kept between a label and a card or another label. */
const LABEL_GAP = 2;
/** How far a hanging label's near edge stays from its point on the curve. */
const HANG_GAP = 4;

function meets(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width + LABEL_GAP && b.x < a.x + a.width + LABEL_GAP && a.y < b.y + b.height + LABEL_GAP && b.y < a.y + a.height + LABEL_GAP;
}

/**
 * The centers a label `width` wide tries, in order: centered on points along the curve, then nudged
 * above and below them; then hanging beside each point (its left edge just right of it, then its
 * right edge just left of it), level with it and then nudged above and below. A 20px label nudged
 * 12px has its edge 2px from the point, so a hanging label sits just above or below the line there.
 */
function candidates(curve: Curve, width: number): Point[] {
  const along = CURVE_STEPS.map((t) => pointAt(curve, t));
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
    for (const spot of candidates(label.curve, width)) {
      const box = { x: spot.x - width / 2, y: spot.y - label.height / 2, width, height: label.height };
      if (!taken.some((other) => meets(box, other))) {
        taken.push(box);
        return { x: spot.x, y: spot.y, form };
      }
    }
  }
  return null;
}

/** Each label's placement, or null when no form of it fits anywhere near its curve. */
export function placeLabels(labels: readonly LabelSpec[], cards: readonly Rect[]): Map<string, Placement | null> {
  const mid = (label: LabelSpec) => pointAt(label.curve, 0.5);
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
