// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { IconButton } from "./IconButton";

describe("IconButton", () => {
  it("is named by its label, sized, and shows a pressed toggle", () => {
    render(<IconButton icon="pin" label="Pin" size={24} pressed />);
    const button = screen.getByRole("button", { name: "Pin" });
    expect(button.getAttribute("aria-pressed")).toBe("true");
    expect(button.classList.contains("icon-button-24")).toBe(true);
    expect(button.querySelector("svg")?.getAttribute("width")).toBe("16");
  });

  it("names itself in a tooltip on hover", () => {
    vi.useFakeTimers();
    render(<IconButton icon="dots" label="Board menu" />);
    fireEvent.pointerOver(screen.getByRole("button", { name: "Board menu" }));
    act(() => {
      vi.advanceTimersByTime(600);
    });
    const tooltip = document.querySelector(".tooltip");
    expect(tooltip?.textContent).toBe("Board menu");
    vi.useRealTimers();
  });
});
