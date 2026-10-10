import { describe, expect, it } from "vitest";
import { CENTERED, placeLens, sameLensPlace } from "./lensPlace";

// 02's top bar: 1176 wide, the lens 124, the state ending at 608, Share at 1009.
const WIDE = { width: 1176, covered: 0, lensWidth: 124, leftEnd: 608 - 264, shareLeft: 1009 };

describe("placeLens", () => {
  it("keeps the lens centered when it clears both sides", () => {
    expect(placeLens(WIDE)).toBe(CENTERED);
  });

  it("moves the lens right of a long publish state, as close to the center as it fits", () => {
    const place = placeLens({ ...WIDE, width: 800, shareLeft: 633, leftEnd: 400 });
    expect(place).toEqual({ left: 412, room: null });
  });

  it("puts the lens before Share and shortens the title and state on a narrow canvas", () => {
    // A pinned peek at 1280×800 leaves a 376px canvas.
    const place = placeLens({ width: 376, covered: 0, lensWidth: 124, leftEnd: 340, shareLeft: 210 });
    expect(place).toEqual({ left: 74, room: 42 });
  });

  it("keeps the lens on the uncovered canvas while a sheet floats over Share", () => {
    const place = placeLens({ width: 1016, covered: 640, lensWidth: 124, leftEnd: 340, shareLeft: 850 });
    expect(place.left).toBe(1016 - 640 - 12 - 124);
    expect(place.room).toBe(place.left === null ? null : place.left - 32);
  });

  it("never puts the lens over the left padding", () => {
    expect(placeLens({ width: 200, covered: 0, lensWidth: 124, leftEnd: 300, shareLeft: 60 })).toEqual({ left: 20, room: 0 });
  });
});

describe("sameLensPlace", () => {
  it("compares both fields", () => {
    expect(sameLensPlace(CENTERED, { left: null, room: null })).toBe(true);
    expect(sameLensPlace(CENTERED, { left: 10, room: null })).toBe(false);
  });
});
