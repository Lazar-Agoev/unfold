// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { vi } from "vitest";
import { Tooltip } from "./Tooltip";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Tooltip", () => {
  it("opens after a delay, describes its trigger, and closes on Escape", () => {
    render(
      <Tooltip content="Watching by scanning">
        <button type="button">Graph current</button>
      </Tooltip>,
    );
    const trigger = screen.getByRole("button", { name: "Graph current" });
    expect(screen.getByText("Watching by scanning", { selector: "[hidden]" })).toBeTruthy();
    expect(trigger.getAttribute("aria-describedby")).not.toBeNull();
    fireEvent.pointerOver(trigger);
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByRole("tooltip", { hidden: true }).textContent).toBe("Watching by scanning");
    fireEvent.keyDown(trigger, { key: "Escape" });
    act(() => {
      vi.advanceTimersByTime(0);
    });
    expect(screen.queryByRole("tooltip", { hidden: true })).toBeNull();
  });
});
