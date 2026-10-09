// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StateIndicator, type StateIndicatorProps } from "./StateIndicator";

type State = StateIndicatorProps["state"];

const STATES: readonly [State, string | null, string][] = [
  [{ kind: "current" }, null, "state-dot-current"],
  [{ kind: "compiling", done: 412, total: 1058 }, "Compiling", "progress-ring"],
  [{ kind: "updating", files: 3 }, "Updating", "progress-ring"],
  [{ kind: "problems", count: 2 }, "2 problems", "state-dot-problems"],
  [{ kind: "paused", changed: 12 }, "Background updates paused", "state-glyph-paused"],
  [{ kind: "not_compiled" }, "Not compiled yet", "state-glyph-not-compiled"],
  [{ kind: "failed", message: "disk full" }, "Compile failed", "state-glyph-failed"],
  [{ kind: "resting", last: { kind: "current" }, atMs: 0 }, "Resting", "state-glyph-resting"],
  [{ kind: "unavailable", reason: "The folder is gone." }, "The folder is gone.", "state-glyph-failed"],
];

describe("StateIndicator", () => {
  it.each(STATES)("draws %o in the row slot", (state, words) => {
    const { container } = render(<StateIndicator state={state} variant="row" />);
    const slot = container.querySelector(".state-indicator-row");
    expect(slot?.getAttribute("data-state")).toBe(state.kind);
    if (words === null) {
      // A current project's slot stays empty (07: quiet is the normal state).
      expect(slot?.children).toHaveLength(0);
      expect(slot?.getAttribute("aria-hidden")).toBe("true");
    } else {
      expect(slot?.getAttribute("role")).toBe("img");
      expect(slot?.getAttribute("aria-label")).toBe(words);
    }
  });

  it.each(STATES)("draws %o on the state line", (state, _words, glyphClass) => {
    const { container } = render(<StateIndicator state={state} variant="line" />);
    expect(container.querySelector(`.state-indicator-line .${glyphClass}`)).not.toBeNull();
  });

  it("sizes the ring, triangle, and moon down on the state line", () => {
    const width = (state: State, variant: "row" | "line") =>
      render(<StateIndicator state={state} variant={variant} />).container.querySelector("svg")?.getAttribute("width");
    expect(width({ kind: "compiling", done: 0, total: 0 }, "row")).toBe("14");
    expect(width({ kind: "compiling", done: 0, total: 0 }, "line")).toBe("12");
    expect(width({ kind: "failed", message: "" }, "line")).toBe("12");
    expect(width({ kind: "resting", last: { kind: "current" }, atMs: 0 }, "row")).toBe("14");
  });

  it("puts the problem count in the dot's tooltip", () => {
    vi.useFakeTimers();
    const { container } = render(<StateIndicator state={{ kind: "problems", count: 1 }} variant="row" />);
    const dot = container.querySelector(".state-indicator");
    expect(dot).not.toBeNull();
    if (dot) {
      fireEvent.pointerOver(dot);
    }
    act(() => {
      vi.advanceTimersByTime(600);
    });
    expect(document.querySelector(".tooltip")?.textContent).toBe("1 problem");
    vi.useRealTimers();
  });
});
