import { describe, expect, it } from "vitest";
import type { OutlineItem } from "../ipc/generated";
import { fixture } from "../test/engine";
import { canFold, definitionAt, focusAnchor, focusMarkOn, inFocus, markRuns, pageOf, pageRange, showsFocus, visibleLines } from "./model";

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

  it("counts type declarations as definitions, as the catalog does", () => {
    const item = (kind: OutlineItem["kind"]): OutlineItem => ({ key: kind, kind, startLine: 3, endLine: 5, start: 0, end: 1, groupable: false });
    for (const kind of ["interface", "type_alias", "enum"] as const) {
      expect(definitionAt([item(kind)], 3)?.key).toBe(kind);
    }
    expect(definitionAt([item("variable")], 3)).toBeUndefined();
  });
});

describe("View code's focus", () => {
  it("is shown only on source the graph was built from, or on evidence read as text", () => {
    expect(showsFocus("current")).toBe(true);
    expect(showsFocus("evidence")).toBe(true);
    expect(showsFocus("not_compiled")).toBe(false);
    expect(showsFocus("not_indexed")).toBe(false);
  });

  it("tints its rows and marks its expression from the start column on its first line to the end column on its last", () => {
    const focus = { startLine: 27, startColumn: 6, endLine: 29, endColumn: 4 };
    expect([26, 27, 28, 29, 30].map((line) => inFocus(line, focus))).toEqual([false, true, true, true, false]);
    expect(focusMarkOn(27, focus, 40)).toEqual({ start: 6, end: 40 });
    expect(focusMarkOn(28, focus, 12)).toEqual({ start: 0, end: 12 });
    expect(focusMarkOn(29, focus, 20)).toEqual({ start: 0, end: 4 });
    expect(focusMarkOn(30, focus, 20)).toBeNull();
    // Lines only (a definition, a combined block): rows, no expression.
    expect(focusMarkOn(27, { startLine: 27, endLine: 31 }, 40)).toBeNull();
  });

  it("measures columns in UTF-16 code units, as the file's text is sliced, not in bytes", () => {
    const line = '  if (név === "😀" && ok) {';
    const condition = 'név === "😀" && ok';
    const startColumn = line.indexOf(condition);
    const endColumn = startColumn + condition.length;
    // The same columns counted in UTF-8 bytes land elsewhere: é is 2 bytes, 😀 is 4 (2 UTF-16 units).
    expect(new TextEncoder().encode(line.slice(0, endColumn)).length).not.toBe(endColumn);
    const mark = focusMarkOn(4, { startLine: 4, startColumn, endLine: 4, endColumn }, line.length);
    expect(mark && line.slice(mark.start, mark.end)).toBe(condition);
    const runs = markRuns(0, line.length, mark ? [mark] : []);
    expect(runs.map((run) => [line.slice(run.start, run.end), run.marked])).toEqual([
      ["  if (", false],
      [condition, true],
      [") {", false],
    ]);
    expect(markRuns(0, 10, [{ start: 30, end: 39 }])).toEqual([{ start: 0, end: 10, marked: false }]);
  });

  it("brings a short focus's middle to the middle of the sheet, and a long one's first line to the top", () => {
    expect(focusAnchor({ startLine: 27, endLine: 31 })).toEqual({ line: 29, align: "center" });
    expect(focusAnchor({ startLine: 10, endLine: 80 })).toEqual({ line: 10, align: "start" });
  });
});
