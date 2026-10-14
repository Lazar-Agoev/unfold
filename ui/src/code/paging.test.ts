import { describe, expect, it } from "vitest";
import { PAGE_LINES, pageRange } from "./paging";

describe("pages", () => {
  it("covers a page's 200 lines, clamped to the file", () => {
    expect(PAGE_LINES).toBe(200);
    expect(pageRange(0, 1000)).toEqual({ startLine: 1, endLine: 200 });
    expect(pageRange(1, 250)).toEqual({ startLine: 201, endLine: 250 });
    expect(pageRange(0, 0)).toEqual({ startLine: 1, endLine: 1 });
  });
});
