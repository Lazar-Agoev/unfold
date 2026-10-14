// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useRef, useState } from "react";
import { describe, expect, it } from "vitest";
import { Popover } from "./Popover";

function AddAsEntrance() {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <button
        type="button"
        onClick={(event) => {
          setAnchor(event.currentTarget);
        }}
      >
        Add as entrance
      </button>
      <Popover
        open={anchor !== null}
        anchor={anchor}
        label="Add as entrance"
        width={340}
        onClose={() => {
          setAnchor(null);
        }}
      >
        <button type="button">Add</button>
      </Popover>
    </>
  );
}

describe("Popover", () => {
  it("takes focus when it opens and gives it back on Escape", () => {
    render(<AddAsEntrance />);
    const trigger = screen.getByRole("button", { name: "Add as entrance" });
    trigger.focus();
    fireEvent.click(trigger);
    const panel = screen.getByRole("dialog", { name: "Add as entrance" });
    expect(panel.style.width).toBe("340px");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Add" }));
    fireEvent.keyDown(panel, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("closes on a press outside but not on its anchor", () => {
    render(<AddAsEntrance />);
    const trigger = screen.getByRole("button", { name: "Add as entrance" });
    fireEvent.click(trigger);
    act(() => {
      trigger.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.getByRole("dialog", { name: "Add as entrance" })).toBeTruthy();
    act(() => {
      document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("focuses its initialFocus element when given one", () => {
    function Form() {
      const name = useRef<HTMLInputElement>(null);
      return (
        <Popover open anchor={null} label="Add" initialFocus={name} onClose={() => undefined}>
          <button type="button">First</button>
          <input ref={name} aria-label="Name" />
        </Popover>
      );
    }
    render(<Form />);
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Name" }));
  });
});
