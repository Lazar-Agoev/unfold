// @vitest-environment jsdom
import "../test/dom";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TokenText, tokenRuns } from "./TokenText";

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
});
