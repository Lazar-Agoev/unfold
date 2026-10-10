import { describe, expect, it } from "vitest";
import type { OutlineItem, TextMatch } from "../ipc/generated";
import { fixture } from "../test/engine";
import { canFold, definitionAt, markRuns, marksOn, matchTicks, pageOf, pageRange, visibleLines } from "./model";

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

describe("definitions", () => {
  const outline = fixture("file_view", "file_view_orders_page").outline;

  it("finds the definition that starts on a line; imports and statements are not definitions", () => {
    const listOrders = definitionAt(outline, 11);
    expect(listOrders?.key).toBe("src/routes/orders.ts#function:listOrders");
    expect(listOrders && canFold(listOrders)).toBe(true);
    expect(definitionAt(outline, 1)).toBeUndefined();
    expect(definitionAt(outline, 12)).toBeUndefined();
  });

  it("takes the outermost definition when several start on one line", () => {
    const item = (key: string, kind: OutlineItem["kind"], endLine: number): OutlineItem => ({ key, kind, startLine: 3, endLine, start: 0, end: 1, groupable: false });
    expect(definitionAt([item("m", "method", 4), item("c", "class", 9)], 3)?.key).toBe("c");
    expect(canFold(item("one", "function", 3))).toBe(false);
  });
});

describe("text matches", () => {
  const matches: TextMatch[] = [
    { line: 41, text: "return refund.status === 'succeeded'", column: 4, ranges: [[26, 35]] },
    { line: 59, text: "case 'payment_intent.succeeded':", column: 0, ranges: [[21, 30]] },
  ];

  it("places a line's matches by their column, the current one marked", () => {
    expect(marksOn(41, matches, 0)).toEqual([{ start: 30, end: 39, current: true }]);
    expect(marksOn(59, matches, 0)).toEqual([{ start: 21, end: 30, current: false }]);
    expect(marksOn(42, matches, 0)).toEqual([]);
  });

  it("cuts a run at its marks", () => {
    expect(markRuns(20, 45, [{ start: 30, end: 39, current: true }])).toEqual([
      { start: 20, end: 30, mark: null },
      { start: 30, end: 39, mark: "current" },
      { start: 39, end: 45, mark: null },
    ]);
    expect(markRuns(0, 10, [{ start: 30, end: 39, current: false }])).toEqual([{ start: 0, end: 10, mark: null }]);
  });

  it("ticks each matching line at its place among the shown rows", () => {
    const rows = Array.from({ length: 100 }, (_, index) => index + 1);
    expect(matchTicks(rows, matches, 1)).toEqual([
      { line: 41, at: 0.4, current: false },
      { line: 59, at: 0.58, current: true },
    ]);
    expect(matchTicks(rows.filter((line) => line !== 59), matches, 1)).toHaveLength(1);
  });
});
