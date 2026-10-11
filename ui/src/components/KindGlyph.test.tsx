// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KindGlyph } from "./KindGlyph";

describe("KindGlyph", () => {
  it("draws ƒ for functions and icons for the other kinds", () => {
    const { container } = render(
      <>
        <KindGlyph kind="function" selected />
        <KindGlyph kind="entrance" />
        <KindGlyph kind="group" />
        <KindGlyph kind="link" />
        <KindGlyph kind="frame" />
        <KindGlyph kind="frame" icon="frame-worker" />
      </>,
    );
    const glyphs = [...container.querySelectorAll(".kind-glyph")];
    expect(glyphs[0]?.textContent).toBe("ƒ");
    expect(glyphs[0]?.classList.contains("kind-glyph-selected")).toBe(true);
    expect(glyphs.slice(1).map((glyph) => glyph.querySelector("svg")?.getAttribute("data-icon"))).toEqual(["entrance", "group", "link", "frame", "frame-worker"]);
  });
});
