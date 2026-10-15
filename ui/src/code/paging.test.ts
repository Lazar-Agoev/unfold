import { describe, expect, it } from "vitest";
import { PAGE_LINES, pageLines, pagesOf } from "./paging";

describe("pages", () => {
  it("covers a page's 200 lines from the view's first line, clamped to the view", () => {
    expect(PAGE_LINES).toBe(200);
    expect(pagesOf([1, 1000])).toEqual([0, 1, 2, 3, 4]);
    expect(pageLines(0, [1, 1000])).toEqual([1, 200]);
    expect(pageLines(1, [1, 250])).toEqual([201, 250]);
    expect(pagesOf([25, 35])).toEqual([0]);
    expect(pageLines(0, [25, 35])).toEqual([25, 35]);
    expect(pageLines(2, [101, 1000])).toEqual([501, 700]);
  });
});
