// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Popover } from "./Popover";

function Switcher() {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <button
        type="button"
        onClick={(event) => {
          setAnchor(event.currentTarget);
        }}
      >
        main
      </button>
      <Popover
        open={anchor !== null}
        anchor={anchor}
        label="Worktrees"
        width={264}
        onClose={() => {
          setAnchor(null);
        }}
      >
        <button type="button">feat/cart-v2</button>
      </Popover>
    </>
  );
}

describe("Popover", () => {
  it("takes focus when it opens and gives it back on Escape", () => {
    render(<Switcher />);
    const trigger = screen.getByRole("button", { name: "main" });
    trigger.focus();
    fireEvent.click(trigger);
    const panel = screen.getByRole("dialog", { name: "Worktrees" });
    expect(panel.style.width).toBe("264px");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "feat/cart-v2" }));
    fireEvent.keyDown(panel, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("closes on a press outside but not on its anchor", () => {
    render(<Switcher />);
    const trigger = screen.getByRole("button", { name: "main" });
    fireEvent.click(trigger);
    act(() => {
      trigger.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.getByRole("dialog", { name: "Worktrees" })).toBeTruthy();
    act(() => {
      document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
