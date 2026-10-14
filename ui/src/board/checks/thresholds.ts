/**
 * Every limit the page checks apply, in one place (the review checklist: about 12 visible
 * items per frame, 30 per page, and 12 visible steps per expanded function card).
 */

export const THRESHOLDS = {
  /** Visible items a frame holds before it is too dense. */
  frameItems: 12,
  /** Visible items a page holds before it is too dense. */
  pageItems: 30,
  /** Visible steps an expanded function card shows before it is too dense. */
  flowSteps: 12,
  /** Crossings between lines a page may have before it asks for a rearrangement. */
  lineCrossings: 3,
  /** How far two boxes must overlap on both axes, in canvas units, to count (touching borders don't). */
  overlap: 2,
  /** Lines that cross this close to either line's ends (where they leave and enter cards) don't count. */
  endSlack: 12,
  /** Names a finding lists before "and N more". */
  listedNames: 6,
} as const;
