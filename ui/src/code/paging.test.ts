import { describe, expect, it } from "vitest";
import { PAGE_LINES, pageOf, pageRange } from "./paging";

describe("pages", () => {
  it("maps lines to 200-line pages", () => {
    expect(PAGE_LINES).toBe(200);
    expect([pageOf(1), pageOf(200), pageOf(201)]).toEqual([0, 0, 1]);
  });

  it("covers a page's lines, clamped to the file", () => {
    expect(pageRange(0, 1000)).toEqual({ startLine: 1, endLine: 200 });
    expect(pageRange(1, 250)).toEqual({ startLine: 201, endLine: 250 });
    expect(pageRange(0, 0)).toEqual({ startLine: 1, endLine: 1 });
  });
});
