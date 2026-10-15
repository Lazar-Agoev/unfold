/**
 * Every limit the page checks apply, in one place (the review checklist: about 12 visible
 * items per frame, 30 per page, and 12 visible steps per expanded function card; an agent's titles on
 * about 3 cards, its descriptions on about 3 compact cards, and 2 of its notes).
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
  /**
   * Definition cards an agent may title before the page is too wordy: a title is for the few cards
   * whose code name says nothing, and the code name shows beside it.
   */
  titledCards: 3,
  /** Compact cards an agent may describe before the page is too wordy: the one to three the page is about. */
  describedCards: 3,
  /** Notes an agent may leave on a page before it is too wordy: a caveat or an open question a reader would miss. */
  notes: 2,
} as const;
