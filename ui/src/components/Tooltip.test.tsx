// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { vi } from "vitest";
import { HoverCard, Tooltip } from "./Tooltip";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Tooltip", () => {
  it("opens after a delay, describes its trigger, and closes on Escape", () => {
    render(
      <Tooltip content="Using fallback location">
        <button type="button">Graph current</button>
      </Tooltip>,
    );
    const trigger = screen.getByRole("button", { name: "Graph current" });
    expect(screen.getByText("Using fallback location", { selector: "[hidden]" })).toBeTruthy();
    expect(trigger.getAttribute("aria-describedby")).not.toBeNull();
    fireEvent.pointerOver(trigger);
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByRole("tooltip", { hidden: true }).textContent).toBe("Using fallback location");
    fireEvent.keyDown(trigger, { key: "Escape" });
    act(() => {
      vi.advanceTimersByTime(0);
    });
    expect(screen.queryByRole("tooltip", { hidden: true })).toBeNull();
  });
});

describe("HoverCard", () => {
  it("shows richer detail on hover and keeps it while the pointer is inside", () => {
    render(
      <HoverCard content={<p>processRefund · jobs/queues.ts:41</p>}>
        <button type="button">refund</button>
      </HoverCard>,
    );
    fireEvent.pointerOver(screen.getByRole("button", { name: "refund" }));
    act(() => {
      vi.advanceTimersByTime(400);
    });
    const card = screen.getByRole("tooltip");
    expect(card.classList.contains("hovercard")).toBe(true);
    fireEvent.pointerOut(screen.getByRole("button", { name: "refund" }), { relatedTarget: card });
    fireEvent.pointerEnter(card);
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByRole("tooltip")).toBeTruthy();
  });
});
