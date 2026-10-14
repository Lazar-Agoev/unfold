// @vitest-environment jsdom
import "../test/dom";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TokenText, lineRuns, tokenRuns } from "./TokenText";

describe("a line's tokens", () => {
  it("cut the line into colored runs with plain text between them", () => {
    expect(tokenRuns(18, [[0, 5, "keyword"], [6, 11, "variable"], [14, 18, "number"]])).toEqual([
      { start: 0, end: 5, token: "keyword" },
      { start: 5, end: 6, token: null },
      { start: 6, end: 11, token: "variable" },
      { start: 11, end: 14, token: null },
      { start: 14, end: 18, token: "number" },
    ]);
  });

  it("are clamped to the line, and an empty token is left out", () => {
    expect(tokenRuns(4, [[2, 2, "comment"], [2, 9, "string"]])).toEqual([
      { start: 0, end: 2, token: null },
      { start: 2, end: 4, token: "string" },
    ]);
    expect(tokenRuns(3, [])).toEqual([{ start: 0, end: 3, token: null }]);
  });

  it("index the line in UTF-16 units, as JavaScript slices it", () => {
    const text = 'x = "😀"';
    const { container } = render(<TokenText text={text} tokens={[[4, 8, "string"]]} />);
    expect(container.querySelector(".tok-string")?.textContent).toBe('"😀"');
    expect(container.textContent).toBe(text);
  });

  it("are cut again at the calls' names and the mark, each run carrying its call and whether it is marked", () => {
    // `  x = foo(bar(1))`: foo's name is 6–9, bar's 10–13; the mark is foo's whole expression, 6–17.
    const runs = lineRuns(17, [[6, 9, "function"], [10, 13, "function"]], [{ start: 6, end: 9, key: "foo" }, { start: 10, end: 13, key: "bar" }], [{ start: 6, end: 17 }]);
    expect(runs).toEqual([
      { start: 0, end: 6, token: null, call: null, marked: false },
      { start: 6, end: 9, token: "function", call: "foo", marked: true },
      { start: 9, end: 10, token: null, call: null, marked: true },
      { start: 10, end: 13, token: "function", call: "bar", marked: true },
      { start: 13, end: 17, token: null, call: null, marked: true },
    ]);
  });

  it("render a call's name with its key and the mark filled", () => {
    const { container } = render(<TokenText text="x = foo(1)" tokens={[[4, 7, "function"]]} calls={[{ start: 4, end: 7, key: "foo-key" }]} marks={[{ start: 4, end: 10 }]} />);
    expect(container.querySelector('[data-call="foo-key"]')?.className).toBe("tok-function code-mark");
    expect([...container.querySelectorAll(".code-mark")].map((item) => item.textContent).join("")).toBe("foo(1)");
  });
});
