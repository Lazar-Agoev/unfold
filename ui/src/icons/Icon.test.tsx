// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ICON_NAMES, Icon, StateGlyph } from ".";

/** The icons the screens use. */
const PLANNED = [
  "chevron-left",
  "chevron-right",
  "chevron-down",
  "chevron-right-small",
  "chevron-down-small",
  "dots",
  "plus",
  "page",
  "link",
  "search",
  "folder",
  "board",
  "check",
  "gear",
  "arrange",
  "external-link",
  "close",
  "warning-circle",
  "warning-triangle",
  "info",
  "copy",
  "edit",
  "minus",
  "agent",
  "frame",
  "frame-api",
  "frame-worker",
  "frame-schedule",
  "frame-cli",
  "frame-web",
  "frame-library",
  "frame-system",
  "resource-table",
  "resource-queue",
  "resource-cache",
  "resource-storage",
  "resource-external",
  "resource-actor",
];

describe("Icon", () => {
  it("has exactly the planned names", () => {
    expect([...ICON_NAMES].sort()).toEqual([...PLANNED].sort());
  });

  it.each(ICON_NAMES)("draws %s as a hidden, currentColor SVG", (name) => {
    const { container } = render(<Icon name={name} size={14} />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("width")).toBe("14");
    expect(svg?.getAttribute("stroke")).toBe("currentColor");
    expect(svg?.getAttribute("viewBox")).toMatch(/^0 0 (10|16|24) (10|16|24)$/);
    expect(svg?.querySelectorAll("path, circle, rect").length).toBeGreaterThan(0);
  });

  it("keeps the dashes and filled dots", () => {
    const { container } = render(
      <>
        <Icon name="link" />
        <Icon name="dots" />
      </>,
    );
    expect(container.querySelector("[data-icon='link'] path")?.getAttribute("stroke-dasharray")).toBe("2.5 2");
    expect(container.querySelectorAll("[data-icon='dots'] circle[fill='currentColor']")).toHaveLength(3);
  });
});

describe("StateGlyph", () => {
  it("draws the sidebar row's sizes and the state line's smaller ones", () => {
    const size = (glyph: "ring" | "failed" | "resting", variant: "row" | "line") =>
      render(<StateGlyph glyph={glyph} variant={variant} />).container.querySelector("svg")?.getAttribute("width");
    expect(size("ring", "row")).toBe("14");
    expect(size("ring", "line")).toBe("12");
    expect(size("failed", "row")).toBe("14");
    expect(size("resting", "line")).toBe("12");
  });

  it("thickens the ring on the state line, as 07 draws it", () => {
    const { container } = render(<StateGlyph glyph="ring" variant="line" />);
    expect(container.querySelector("svg")?.getAttribute("stroke-width")).toBe("2");
  });
});
