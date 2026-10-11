/**
 * Reviewing the launches a deploy-file import found: one at a time, each saved or skipped, and a
 * summary once the last is done or the review ends early.
 */

export interface ReviewProgress {
  /** The launch on screen. */
  readonly index: number;
  readonly saved: number;
  readonly skipped: number;
}

export const REVIEW_START: ReviewProgress = { index: 0, saved: 0, skipped: 0 };

/** The progress after the launch on screen is saved or skipped; `done` once it was the last of `total`. */
export function advance(progress: ReviewProgress, outcome: "saved" | "skipped", total: number): { readonly progress: ReviewProgress; readonly done: boolean } {
  const next = {
    index: progress.index + 1,
    saved: progress.saved + (outcome === "saved" ? 1 : 0),
    skipped: progress.skipped + (outcome === "skipped" ? 1 : 0),
  };
  return { progress: next, done: next.index >= total };
}

function launches(count: number): string {
  return count === 1 ? "1 launch" : `${count} launches`;
}

/** The summary toast's title, or null when nothing was saved. */
export function reviewSummary(progress: ReviewProgress): string | null {
  if (progress.saved === 0) {
    return null;
  }
  const skipped = progress.skipped === 0 ? "" : `, skipped ${progress.skipped}`;
  return `Added ${launches(progress.saved)} to unfold.toml${skipped}`;
}
