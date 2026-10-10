/**
 * Which sides a drawn link leaves and enters by: sideways when the cards sit side by side, from the
 * bottom into the top when one is below the other (as "queues a receipt" is drawn).
 */

import type { Rect } from "./layout";

export const LINK_SIDES = ["left", "right", "top", "bottom"] as const;
type LinkSide = (typeof LINK_SIDES)[number];

export interface LinkEnds {
  readonly from: LinkSide;
  readonly to: LinkSide;
}

/** The sides joining `from` to `to`: right → left when `to` is to the right, and so on. */
export function linkEnds(from: Rect, to: Rect): LinkEnds {
  if (to.x >= from.x + from.width) {
    return { from: "right", to: "left" };
  }
  if (to.x + to.width <= from.x) {
    return { from: "left", to: "right" };
  }
  return to.y >= from.y + from.height / 2 ? { from: "bottom", to: "top" } : { from: "top", to: "bottom" };
}
