import { describe, expect, it } from "vitest";
import type { CodeView } from "../ipc/generated";
import { loadedLines } from "./view";

const FILE = "src/routes/orders.ts";

/** An answer for the view of lines 1–500 holding `from`–`to`, a definition on each tenth line. */
function answer(from: number, to: number, overrides: Partial<CodeView> = {}): CodeView {
  const numbers = Array.from({ length: to - from + 1 }, (_, index) => from + index);
  return {
    file: FILE,
    state: "current",
    span: [1, 500],
    lines: numbers.map((number) => ({ number, text: `line ${number}`, tokens: [], calls: [] })),
    path: [{ label: FILE, showing: true }],
    definitions: numbers.filter((number) => number % 10 === 0).map((line) => ({ line, key: `${FILE}#f${line}` })),
    ...overrides,
  };
}

describe("the lines loaded so far", () => {
  it("merges the first answer with the pages asked for since, each line from the first answer holding it", () => {
    const first = answer(150, 349, { mark: { lines: [200, 201], expression: [{ line: 200, start: 0, end: 4 }] } });
    const page = answer(201, 400, { mark: { lines: [200, 201], expression: [{ line: 201, start: 1, end: 2 }] } });
    const loaded = loadedLines(first, [undefined, page]);
    expect(loaded.span).toEqual([1, 500]);
    expect([...loaded.lines.keys()].sort((a, b) => a - b)).toEqual(Array.from({ length: 251 }, (_, index) => 150 + index));
    expect(loaded.lines.get(200)?.marks).toEqual([{ line: 200, start: 0, end: 4 }]);
    // Line 201 came with the first answer, which marks nothing on it.
    expect(loaded.lines.get(201)?.marks).toEqual([]);
    expect(loaded.definitions.map((definition) => definition.line)).toEqual(Array.from({ length: 26 }, (_, index) => 150 + index * 10));
  });

  it("leaves out an answer for another state or span of the file", () => {
    const first = answer(1, 200);
    const changed = answer(201, 400, { state: "changed" });
    const longer = answer(201, 400, { span: [1, 520] });
    expect(loadedLines(first, [changed, longer]).lines.size).toBe(200);
  });

  it("holds nothing for a view that shows nothing", () => {
    const missing: CodeView = { state: "notFound", lines: [], path: [], definitions: [] };
    expect(loadedLines(missing, [])).toEqual({ span: [1, 0], lines: new Map(), definitions: [] });
  });
});
