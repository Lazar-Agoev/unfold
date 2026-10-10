import { describe, expect, it } from "vitest";
import { linkEnds } from "./linkSides";

const card = { x: 0, y: 0, width: 400, height: 200 };

describe("drawn link sides", () => {
  it("joins cards side by side from the facing edges", () => {
    expect(linkEnds(card, { x: 600, y: 300, width: 200, height: 160 })).toEqual({ from: "right", to: "left" });
    expect(linkEnds(card, { x: -500, y: 0, width: 200, height: 160 })).toEqual({ from: "left", to: "right" });
  });

  it("joins cards one above the other from the bottom into the top", () => {
    expect(linkEnds(card, { x: 100, y: 400, width: 288, height: 165 })).toEqual({ from: "bottom", to: "top" });
    expect(linkEnds(card, { x: 100, y: -400, width: 288, height: 165 })).toEqual({ from: "top", to: "bottom" });
  });
});
