/**
 * Where a note on a line goes (design §4.8): beside the line's middle, covering nothing. Arrange and
 * an agent's `add_notes` place a link's note this way. Pure.
 */

import { HEADER_PORT, clearSpot, type Point, type Rect, type Size } from "../layout";
import { linkEnds } from "../linkSides";
import { NOTE_GAP } from "../frames/geometry";

/**
 * The middle of a line drawn between rectangles `from` and `to`: halfway between where it leaves and
 * enters, by the sides facing each other (`linkEnds`: a side's end at the header's height, a top or
 * bottom end mid-edge), which is where the canvas's curve between them passes its middle.
 */
export function lineMiddle(from: Rect, to: Rect): Point {
  const sides = linkEnds(from, to);
  const end = (rect: Rect, side: typeof sides.from): Point => {
    switch (side) {
      case "left":
        return { x: rect.x, y: rect.y + HEADER_PORT };
      case "right":
        return { x: rect.x + rect.width, y: rect.y + HEADER_PORT };
      case "top":
        return { x: rect.x + rect.width / 2, y: rect.y };
      case "bottom":
        return { x: rect.x + rect.width / 2, y: rect.y + rect.height };
    }
  };
  const a = end(from, sides.from);
  const b = end(to, sides.to);
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

/**
 * Where a note of `size` goes beside a line's `middle`: centered under it, else centered over it,
 * where that keeps clear of everything in `occupied` (as `clearSpot` keeps cards apart); else the
 * nearer of those two moved down, or up, until it does.
 */
export function lineNoteSpot(middle: Point, size: Size, occupied: readonly Rect[]): [number, number] {
  const x = middle.x - size.width / 2;
  const under = Math.round(middle.y + NOTE_GAP);
  const over = Math.round(middle.y - NOTE_GAP - size.height);
  const down = clearSpot({ x, y: under }, size, occupied);
  const up = clearSpot({ x, y: over }, size, occupied, "up");
  if (down[1] === under) {
    return down;
  }
  if (up[1] === over) {
    return up;
  }
  return down[1] - under <= over - up[1] ? down : up;
}
