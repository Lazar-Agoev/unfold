import { describe, expect, it } from "vitest";
import { fixture } from "../../test/engine";
import { foldsStartingAt, lineSegments, lineText, pageOf, pageRange, visibleLines } from "./model";

describe("pages", () => {
  it("maps lines to 200-line pages, clamped to the file", () => {
    expect([pageOf(1), pageOf(200), pageOf(201)]).toEqual([0, 0, 1]);
    expect(pageRange(1, 250)).toEqual({ startLine: 201, endLine: 250 });
  });
});

describe("visibleLines", () => {
  it("hides the lines after a folded range's first line, and nested ranges inside a fold", () => {
    expect(visibleLines(10, [])).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(visibleLines(10, [{ startLine: 2, endLine: 5 }])).toEqual([1, 2, 6, 7, 8, 9, 10]);
    expect(
      visibleLines(10, [
        { startLine: 2, endLine: 8 },
        { startLine: 3, endLine: 4 },
      ]),
    ).toEqual([1, 2, 9, 10]);
    expect(
      visibleLines(10, [
        { startLine: 3, endLine: 4 },
        { startLine: 6, endLine: 9 },
      ]),
    ).toEqual([1, 2, 3, 5, 6, 10]);
  });
});

describe("lineSegments", () => {
  const page = fixture("file_view", "file_view_orders_page");

  it("splits a line at color and link boundaries and keeps every character", () => {
    const line = page.lines[1];
    if (!line) {
      throw new Error("the golden page has a second line");
    }
    const segments = lineSegments(page, line, page.links);
    expect(segments.map((segment) => segment.text).join("")).toBe(lineText(page, line));
    expect(lineText(page, line)).toBe('import { asyncHandler } from "../lib/async-handler.ts";');
    const linked = segments.filter((segment) => segment.link !== null);
    expect(linked.map((segment) => segment.text).join("")).toBe('"../lib/async-handler.ts"');
    expect(linked[0]?.link?.target).toBe("src/lib/async-handler.ts#");
    expect(segments[0]).toMatchObject({ text: "import", token: "keyword", link: null });
  });

  it("returns nothing for an empty line", () => {
    const empty = page.lines[7];
    expect(empty && lineSegments(page, empty)).toEqual([]);
  });

  it("finds the folds that start on a line, outermost first", () => {
    const folds = page.outline.filter((item) => item.fold !== undefined);
    const first = folds[0];
    if (first) {
      expect(foldsStartingAt(page.outline, first.startLine)[0]?.startLine).toBe(first.startLine);
    }
    expect(foldsStartingAt(page.outline, 8)).toEqual([]);
  });
});
