import { describe, expect, it } from "vitest";
import { parseLines } from "./groupFromLines";

describe("Make a group from lines", () => {
  it("reads a line range in either order, or one line", () => {
    expect(parseLines("27-31")).toEqual([27, 31]);
    expect(parseLines(" 31 – 27 ")).toEqual([27, 31]);
    expect(parseLines("12")).toEqual([12, 12]);
    expect(parseLines("12 to 14")).toBeNull();
    expect(parseLines("")).toBeNull();
  });
});
