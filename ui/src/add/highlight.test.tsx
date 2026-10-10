// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { highlightRanges } from "./highlight";

describe("highlightRanges", () => {
  it("marks each range, clamped and in order, and keeps every character", () => {
    const { container } = render(<p>{highlightRanges('log.info("refund_order")', [[10, 16], [0, 3], [40, 50]], "hit")}</p>);
    expect(container.textContent).toBe('log.info("refund_order")');
    expect([...container.querySelectorAll("mark.hit")].map((mark) => mark.textContent)).toEqual(["log", "refund"]);
  });

  it("returns the text as is without ranges", () => {
    expect(highlightRanges("plain", [], "hit")).toBe("plain");
  });
});
