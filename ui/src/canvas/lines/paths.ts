/**
 * Orthogonal lines as SVG paths: straight runs joined by rounded corners. Pure.
 */

import type { Point } from "./placeLabels";

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
