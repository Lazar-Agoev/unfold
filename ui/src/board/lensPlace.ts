/**
 * Where the top bar's Diagram | Code lens goes: centered over the uncovered canvas while it clears the
 * page title on its left and the bar's end on its right; otherwise as close to the center as it fits,
 * and on a canvas too narrow for both, at the right of the free space with the title shortened to the
 * room left of it.
 */

/** The top bar's left padding. */
const PADDING_LEFT = 20;
/** The space kept between the lens and its neighbours. */
const CLEARANCE = 12;

export interface LensMetrics {
  /** The top bar's width. */
  readonly width: number;
  /** The width a floating sheet covers on its right. */
  readonly covered: number;
  readonly lensWidth: number;
  /** Where the title (and "Claude is working…") end at their full width, from the top bar's left edge. */
  readonly leftEnd: number;
  /** Where the top bar's content ends on the right (inside its padding), from its left edge. */
  readonly rightEnd: number;
}

export interface LensPlace {
  /** The lens's left edge, or null when it stays centered. */
  readonly left: number | null;
  /** The width the title may take, or null for its full width. */
  readonly room: number | null;
}

export const CENTERED: LensPlace = { left: null, room: null };

export function placeLens({ width, covered, lensWidth, leftEnd, rightEnd }: LensMetrics): LensPlace {
  const centered = (width - covered - lensWidth) / 2;
  const minLeft = leftEnd + CLEARANCE;
  const maxLeft = Math.min(rightEnd, width - covered) - CLEARANCE - lensWidth;
  if (centered >= minLeft && centered <= maxLeft) {
    return CENTERED;
  }
  const left = Math.max(PADDING_LEFT, Math.min(Math.max(centered, minLeft), maxLeft));
  return { left, room: left < minLeft ? Math.max(0, left - CLEARANCE - PADDING_LEFT) : null };
}

/** Whether two placements are the same (so a measurement that changes nothing doesn't re-render). */
export function sameLensPlace(a: LensPlace, b: LensPlace): boolean {
  return a.left === b.left && a.room === b.room;
}
