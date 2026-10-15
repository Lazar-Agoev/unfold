/**
 * Where a note on a line goes: beside the line's middle (`lineMiddle`: the middle of its longest
 * straight run as routed), covering nothing. Arrange and an agent's `add_notes` place a link's note
 * this way. Pure.
 */

import { clearSpot, type Point, type Rect, type Size } from "../layout";
import { NOTE_GAP } from "../frames/geometry";

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
