/**
 * The drawn page the checks read (boxes and lines as the canvas laid them out), and the plane geometry
 * they need: overlaps, crossing segments, and a segment meeting a box. Pure.
 */

/** A point on the canvas, in canvas units. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** A box on the canvas, in canvas units. */
export interface Box {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A drawn line between two items: a compiled arrow, or a connection. */
export interface DrawnLine {
  /** The line's id on the canvas: a connection's id, or an arrow's (which is no page item). */
  readonly id: string;
  /** The items it joins: cards, or frames and resource cards where the canvas draws lines to them. */
  readonly from: string;
  readonly to: string;
  /** Its path, sampled, in canvas units; empty when the canvas couldn't say. */
  readonly points: readonly Point[];
}

/** The page as the canvas laid it out. */
export interface PageGeometry {
  /** Each drawn item's box by id: cards, and frames, resource cards, and notes where the canvas draws them. */
  readonly boxes: ReadonlyMap<string, Box>;
  readonly lines: readonly DrawnLine[];
}

/** How far two boxes overlap along each axis (zero or less where they don't). */
export function overlapOf(a: Box, b: Box): { readonly width: number; readonly height: number } {
  return {
    width: Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x),
    height: Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y),
  };
}

/** The smallest box around `boxes`; undefined for none. */
export function boundsOf(boxes: readonly Box[]): Box | undefined {
  if (boxes.length === 0) {
    return undefined;
  }
  const left = Math.min(...boxes.map((box) => box.x));
  const top = Math.min(...boxes.map((box) => box.y));
  const right = Math.max(...boxes.map((box) => box.x + box.width));
  const bottom = Math.max(...boxes.map((box) => box.y + box.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

function cross(o: Point, a: Point, b: Point): number {
  return (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
}

/** Where segments `p1–p2` and `q1–q2` cross each other, or null (touching or parallel segments don't cross). */
export function crossingOf(p1: Point, p2: Point, q1: Point, q2: Point): Point | null {
  const d1 = cross(q1, q2, p1);
  const d2 = cross(q1, q2, p2);
  const d3 = cross(p1, p2, q1);
  const d4 = cross(p1, p2, q2);
  if (!(((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)))) {
    return null;
  }
  const t = d1 / (d1 - d2);
  return { x: p1.x + (p2.x - p1.x) * t, y: p1.y + (p2.y - p1.y) * t };
}

function inside(point: Point, box: Box): boolean {
  return point.x > box.x && point.x < box.x + box.width && point.y > box.y && point.y < box.y + box.height;
}

/** Whether segment `p–q` passes through the inside of `box` (or ends there). */
export function segmentMeetsBox(p: Point, q: Point, box: Box): boolean {
  if (inside(p, box) || inside(q, box)) {
    return true;
  }
  const corners: Point[] = [
    { x: box.x, y: box.y },
    { x: box.x + box.width, y: box.y },
    { x: box.x + box.width, y: box.y + box.height },
    { x: box.x, y: box.y + box.height },
  ];
  return corners.some((corner, index) => crossingOf(p, q, corner, corners[(index + 1) % corners.length] ?? corner) !== null);
}

/** The distance between two points. */
export function distance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}
