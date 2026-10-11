import { describe, expect, it } from "vitest";
import { REVIEW_START, advance, reviewSummary } from "./review";

describe("reviewing imported launches", () => {
  it("steps through each launch and ends after the last", () => {
    const first = advance(REVIEW_START, "saved", 3);
    expect(first).toEqual({ progress: { index: 1, saved: 1, skipped: 0 }, done: false });
    const second = advance(first.progress, "skipped", 3);
    expect(second.done).toBe(false);
    const third = advance(second.progress, "saved", 3);
    expect(third).toEqual({ progress: { index: 3, saved: 2, skipped: 1 }, done: true });
  });

  it("sums up what was saved", () => {
    expect(reviewSummary(REVIEW_START)).toBeNull();
    expect(reviewSummary({ index: 1, saved: 1, skipped: 0 })).toBe("Added 1 launch to unfold.toml");
    expect(reviewSummary({ index: 14, saved: 12, skipped: 2 })).toBe("Added 12 launches to unfold.toml, skipped 2");
  });
});
