/**
 * Where a line's contract opens (docs/proposals/typed-lines.md §7): clear of the caller's calling line
 * and the callee's marked lines, so what it reads stays in view. It tries being centered on points
 * along its curve (the label's point first, then 0.4 and 0.6, 0.3 and 0.7, …), then hanging just
 * below each of them (centered, then to its left, then to its right), and takes the first spot that
 * covers none of those rows; when none is clear, it is centered on the label's point. Pure: the same size, curve, and rows always give the same spot.
 */

import { CURVE_STEPS, pointAt, type Curve, type Point, type Rect } from "./placeLabels";

/** How far below a point on the curve a hanging contract starts, so the line stays in view above it. */
export const BELOW_GAP = 12;
/** Space kept between the contract and a row it keeps clear of. */
const ROW_GAP = 4;

function covers(box: Rect, row: Rect): boolean {
  return box.x < row.x + row.width + ROW_GAP && row.x < box.x + box.width + ROW_GAP && box.y < row.y + row.height + ROW_GAP && row.y < box.y + box.height + ROW_GAP;
}

/** The contract's top center, in canvas units, for a contract of `size` on `curve` that keeps clear of `rows`. */
export function placeContract(size: { readonly width: number; readonly height: number }, curve: Curve, rows: readonly Rect[]): Point {
  const along = CURVE_STEPS.map((t) => pointAt(curve, t));
  const half = size.width / 2;
  const tops = [
    ...along.map((point) => ({ x: point.x, y: point.y - size.height / 2 })),
    // Below a point: centered under it, then ending at it (toward the caller), then starting at it.
    ...along.flatMap((point) => [0, -half, half].map((shift) => ({ x: point.x + shift, y: point.y + BELOW_GAP }))),
  ];
  const clear = tops.find((top) => !rows.some((row) => covers({ x: top.x - size.width / 2, y: top.y, width: size.width, height: size.height }, row)));
  if (clear) {
    return clear;
  }
  const label = pointAt(curve, 0.5);
  return { x: label.x, y: label.y - size.height / 2 };
}
