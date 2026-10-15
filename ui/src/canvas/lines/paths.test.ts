import { describe, expect, it } from "vitest";
import { roundedPath } from "./paths";

describe("orthogonal paths", () => {
  it("draw straight runs, joined by corners of the radius", () => {
    expect(roundedPath([{ x: 0, y: 0 }, { x: 100, y: 0 }])).toBe("M0 0 L100 0");
    expect(roundedPath([{ x: 0, y: 0 }, { x: 50, y: 0 }, { x: 50, y: 40 }, { x: 100, y: 40 }])).toBe("M0 0 L42 0 Q50 0 50 8 L50 32 Q50 40 58 40 L100 40");
  });

  it("round a corner less where a run is too short for it, and drop repeated and in-between points", () => {
    expect(roundedPath([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 6 }])).toBe("M0 0 L7 0 Q10 0 10 3 L10 6");
    expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 0 }, { x: 30, y: 0 }, { x: 60, y: 0 }])).toBe("M0 0 L60 0");
    expect(roundedPath([])).toBe("");
  });
});
