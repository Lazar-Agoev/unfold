/**
 * The marks at the ends of connections: a filled arrowhead where the data goes, and a
 * relationship's crow's-foot ends (a bar for one, a crow's foot for many). Pure SVG path data.
 */

import { Position } from "@xyflow/react";
import type { Cardinality } from "../../ipc/generated";

/** Which way a line entering a card by `side` points, as a unit vector. */
export const INTO: Readonly<Record<Position, readonly [number, number]>> = {
  [Position.Left]: [1, 0],
  [Position.Right]: [-1, 0],
  [Position.Top]: [0, 1],
  [Position.Bottom]: [0, -1],
};

/** A filled head with its tip at (x, y), pointing along (dx, dy). */
export function arrowHead(x: number, y: number, [dx, dy]: readonly [number, number], length: number, width: number): string {
  const bx = x - dx * length;
  const by = y - dy * length;
  const nx = -dy * (width / 2);
  const ny = dx * (width / 2);
  return `M${bx + nx} ${by + ny} L${x} ${y} L${bx - nx} ${by - ny} Z`;
}

/** How far a crow's foot reaches out of its card, and how wide it opens. */
const FOOT_LENGTH = 10;
const FOOT_HALF = 6;
/** How far a "one" bar stands out of its card. */
const BAR_OUT = 7;

/**
 * The mark of one end of a relationship at (x, y), where the line meets a card on `side`: a crow's
 * foot for "many" (three strokes fanning into the card), a bar across the line for "one".
 */
export function cardinalityMark(x: number, y: number, side: Position, many: boolean): string {
  const [ix, iy] = INTO[side];
  const [dx, dy] = [-ix, -iy];
  const [nx, ny] = [-dy, dx];
  if (many) {
    const tx = x + dx * FOOT_LENGTH;
    const ty = y + dy * FOOT_LENGTH;
    return `M${tx} ${ty} L${x + nx * FOOT_HALF} ${y + ny * FOOT_HALF} M${tx} ${ty} L${x} ${y} M${tx} ${ty} L${x - nx * FOOT_HALF} ${y - ny * FOOT_HALF}`;
  }
  const bx = x + dx * BAR_OUT;
  const by = y + dy * BAR_OUT;
  return `M${bx + nx * FOOT_HALF} ${by + ny * FOOT_HALF} L${bx - nx * FOOT_HALF} ${by - ny * FOOT_HALF}`;
}

/** Whether each end of a relationship is "many" (`one-to-many`: the `to` end). */
export function manyEnds(cardinality: Cardinality): { readonly from: boolean; readonly to: boolean } {
  const [from, to] = cardinality.split("-to-");
  return { from: from === "many", to: to === "many" };
}
