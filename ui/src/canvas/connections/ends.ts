/**
 * The marks at the ends of connections: a filled arrowhead where the data goes, and a
 * relationship's ERD ends (two bars for one, a crow's foot for many). Pure SVG path data.
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

/** How far a crow's foot reaches out of its card, and how wide it opens at the card. */
const FOOT_LENGTH = 10;
const FOOT_HALF = 7;
/** A "one" end's two bars: how far out each stands, and how long they are each side of the line. */
const ONE_BARS = [8, 13] as const;
const BAR_HALF = 6;

/**
 * The mark of one end of a relationship at (x, y), where the line meets a card on `side`, as an ERD
 * draws it: for "many", a crow's foot (two toes from the card's edge meeting the line 10px out) with a
 * bar across its tip; for "one", two bars across the line.
 */
export function cardinalityMark(x: number, y: number, side: Position, many: boolean): string {
  const [ix, iy] = INTO[side];
  const [dx, dy] = [-ix, -iy];
  const [nx, ny] = [-dy, dx];
  const bar = (out: number, half: number) => {
    const bx = x + dx * out;
    const by = y + dy * out;
    return `M${bx + nx * half} ${by + ny * half} L${bx - nx * half} ${by - ny * half}`;
  };
  if (many) {
    const tx = x + dx * FOOT_LENGTH;
    const ty = y + dy * FOOT_LENGTH;
    return `M${x + nx * FOOT_HALF} ${y + ny * FOOT_HALF} L${tx} ${ty} L${x - nx * FOOT_HALF} ${y - ny * FOOT_HALF} ${bar(FOOT_LENGTH, FOOT_HALF)}`;
  }
  return ONE_BARS.map((out) => bar(out, BAR_HALF)).join(" ");
}

/** Whether each end of a relationship is "many" (`one-to-many`: the `to` end). */
export function manyEnds(cardinality: Cardinality): { readonly from: boolean; readonly to: boolean } {
  const [from, to] = cardinality.split("-to-");
  return { from: from === "many", to: to === "many" };
}
