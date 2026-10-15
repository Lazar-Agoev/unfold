/**
 * Orthogonal lines as SVG paths (straight runs joined by rounded corners), and the places along a line
 * its chip and notes go: the middle of its longest straight run first. Pure.
 */

import type { Point } from "../layout";

/** The radius of a line's corners. */
const CORNER_RADIUS = 8;

/** `points` without repeats and without points in the middle of a straight run. */
function corners(points: readonly Point[]): Point[] {
  const kept: Point[] = [];
  for (const point of points) {
    const last = kept[kept.length - 1];
    if (last && Math.abs(last.x - point.x) < 0.01 && Math.abs(last.y - point.y) < 0.01) {
      continue;
    }
    const before = kept[kept.length - 2];
    if (last && before && ((before.x === last.x && last.x === point.x) || (before.y === last.y && last.y === point.y))) {
      kept[kept.length - 1] = point;
      continue;
    }
    kept.push(point);
  }
  return kept;
}

/**
 * An SVG path through `points` (each run horizontal or vertical), its corners rounded to `radius`, or
 * less where a run is too short for two corners.
 */
export function roundedPath(points: readonly Point[], radius = CORNER_RADIUS): string {
  const kept = corners(points);
  const [first] = kept;
  if (!first) {
    return "";
  }
  let d = `M${first.x} ${first.y}`;
  for (let index = 1; index < kept.length; index += 1) {
    const point = kept[index];
    const before = kept[index - 1];
    const after = kept[index + 1];
    if (!point || !before) {
      continue;
    }
    if (!after) {
      d += ` L${point.x} ${point.y}`;
      continue;
    }
    const into = Math.hypot(point.x - before.x, point.y - before.y);
    const out = Math.hypot(after.x - point.x, after.y - point.y);
    const r = Math.min(radius, into / 2, out / 2);
    const enter = { x: point.x - ((point.x - before.x) / into) * r, y: point.y - ((point.y - before.y) / into) * r };
    const leave = { x: point.x + ((after.x - point.x) / out) * r, y: point.y + ((after.y - point.y) / out) * r };
    d += ` L${enter.x} ${enter.y} Q${point.x} ${point.y} ${leave.x} ${leave.y}`;
  }
  return d;
}

/** The straight runs of a line, longest first (the first among equals). */
function runs(points: readonly Point[]): (readonly [Point, Point])[] {
  const out: (readonly [Point, Point, number])[] = [];
  for (let index = 1; index < points.length; index += 1) {
    const [a, b] = [points[index - 1], points[index]];
    if (a && b) {
      out.push([a, b, index]);
    }
  }
  return out.sort(([a1, b1, i], [a2, b2, j]) => Math.abs(b2.x - a2.x) + Math.abs(b2.y - a2.y) - (Math.abs(b1.x - a1.x) + Math.abs(b1.y - a1.y)) || i - j).map(([a, b]) => [a, b] as const);
}

function between(a: Point, b: Point, t: number): Point {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

/** The middle of a line's longest straight run: where its chip and a note on it go. */
export function lineMiddle(points: readonly Point[]): Point {
  const [run] = runs(points);
  return run ? between(run[0], run[1], 0.5) : (points[0] ?? { x: 0, y: 0 });
}

/** Points tried along a run, in order: its middle, then out from it. */
const ALONG_RUN: readonly number[] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];

/** The points along a line a label tries, in order: along its longest straight run (its middle, then out from it), then along the next longest. */
export function alongLine(points: readonly Point[]): Point[] {
  return runs(points).flatMap(([a, b]) => ALONG_RUN.map((t) => between(a, b, t)));
}

/** `points` with its first run shortened by `start` and its last by `end` (where an end's mark goes). */
export function trimmed(points: readonly Point[], start: number, end: number): Point[] {
  const out = [...points];
  const cut = (at: number, toward: number, by: number) => {
    const [a, b] = [out[at], out[toward]];
    if (!a || !b || by <= 0) {
      return;
    }
    const length = Math.abs(b.x - a.x) + Math.abs(b.y - a.y);
    out[at] = length > 0 ? between(a, b, Math.min(by, length) / length) : a;
  };
  cut(0, 1, start);
  cut(out.length - 1, out.length - 2, end);
  return out;
}
