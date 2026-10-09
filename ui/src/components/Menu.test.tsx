// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MenuButton, useContextMenu, type MenuEntry } from "./Menu";

function Row({ entries }: { entries: readonly MenuEntry[] }) {
  const menu = useContextMenu("File actions");
  return (
    <>
      <button
        type="button"
        onContextMenu={(event) => {
          menu.open(event, entries);
        }}
      >
        orders.ts
      </button>
      {menu.element}
    </>
  );
}

const entries = (onOpen: () => void, onReveal: () => void): MenuEntry[] => [
  { id: "open", label: "Open card", shortcut: "Mod+O", onSelect: onOpen },
  { id: "source", label: "Open source", disabled: true, onSelect: vi.fn() },
  { kind: "separator", id: "s1" },
  { id: "reveal", label: "Reveal in Finder", onSelect: onReveal },
];

describe("ContextMenu", () => {
  it("opens at the pointer with the first item focused, skips disabled items, and chooses with Enter", () => {
    const onOpen = vi.fn();
    const onReveal = vi.fn();
    render(<Row entries={entries(onOpen, onReveal)} />);
    const trigger = screen.getByText("orders.ts");
    trigger.focus();
    fireEvent.contextMenu(trigger, { clientX: 40, clientY: 50 });

    const menu = screen.getByRole("menu", { name: "File actions" });
    expect(document.activeElement?.textContent).toContain("Open card");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Reveal in Finder");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toContain("Open card");
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Reveal in Finder" }), { key: "Enter" });

    expect(onReveal).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("closes on Escape and on a press outside, returning focus", () => {
    render(<Row entries={entries(vi.fn(), vi.fn())} />);
    const trigger = screen.getByText("orders.ts");
    trigger.focus();
    fireEvent.contextMenu(trigger);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);

    fireEvent.contextMenu(trigger);
    expect(screen.getByRole("menu")).toBeTruthy();
    act(() => {
      document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("jumps to an item by its first letter and ignores clicks on disabled items", () => {
    const onReveal = vi.fn();
    render(<Row entries={entries(vi.fn(), onReveal)} />);
    fireEvent.contextMenu(screen.getByText("orders.ts"));
    fireEvent.keyDown(screen.getByRole("menu"), { key: "r" });
    expect(document.activeElement?.textContent).toBe("Reveal in Finder");
    fireEvent.click(screen.getByText("Open source"));
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getByText("Open source").closest("[role=menuitem]")?.getAttribute("aria-disabled")).toBe("true");
  });
});

describe("MenuButton", () => {
  it("opens below its button with ↓, reports expanded, and closes back to the button", () => {
    render(<MenuButton variant="text" label="Board" entries={entries(vi.fn(), vi.fn())} />);
    const button = screen.getByRole("button", { name: "Board" });
    expect(button.getAttribute("aria-expanded")).toBe("false");
    button.focus();
    fireEvent.keyDown(button, { key: "ArrowUp" });
    expect(button.getAttribute("aria-expanded")).toBe("true");
    expect(document.activeElement?.textContent).toBe("Reveal in Finder");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Tab" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);
  });
});
