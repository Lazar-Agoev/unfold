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

const entries = (onOpen: () => void, onCopy: () => void): MenuEntry[] => [
  { id: "open", label: "Open card", onSelect: onOpen },
  { id: "source", label: "Open source", disabled: true, onSelect: vi.fn() },
  { kind: "separator", id: "s1" },
  { id: "copy", label: "Copy link", onSelect: onCopy },
];

describe("ContextMenu", () => {
  it("opens at the pointer with the first item focused, skips disabled items, and chooses with Enter", () => {
    const onOpen = vi.fn();
    const onCopy = vi.fn();
    render(<Row entries={entries(onOpen, onCopy)} />);
    const trigger = screen.getByText("orders.ts");
    trigger.focus();
    fireEvent.contextMenu(trigger, { clientX: 40, clientY: 50 });

    const menu = screen.getByRole("menu", { name: "File actions" });
    expect(document.activeElement?.textContent).toContain("Open card");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Copy link");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toContain("Open card");
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Copy link" }), { key: "Enter" });

    expect(onCopy).toHaveBeenCalledTimes(1);
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
    const onCopy = vi.fn();
    render(<Row entries={entries(vi.fn(), onCopy)} />);
    fireEvent.contextMenu(screen.getByText("orders.ts"));
    fireEvent.keyDown(screen.getByRole("menu"), { key: "c" });
    expect(document.activeElement?.textContent).toBe("Copy link");
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
    expect(document.activeElement?.textContent).toBe("Copy link");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Tab" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);
  });
});

describe("Submenus", () => {
  const boardMenu = (onExport: () => void, onDelete: () => void): MenuEntry[] => [
    { id: "rename", label: "Rename…", onSelect: vi.fn() },
    {
      id: "share",
      label: "Share",
      submenu: [
        { id: "export", label: "Export PNG…", onSelect: onExport },
        { id: "copy", label: "Copy link", onSelect: vi.fn() },
      ],
    },
    { kind: "separator", id: "s1" },
    { id: "delete", label: "Delete…", danger: true, onSelect: onDelete },
  ];

  it("opens with → or Enter, leaves with ← or Escape, and chooses inside", () => {
    const onExport = vi.fn();
    render(<MenuButton variant="icon" icon="dots" label="Board menu" width={220} entries={boardMenu(onExport, vi.fn())} />);
    const button = screen.getByRole("button", { name: "Board menu" });
    button.focus();
    fireEvent.click(button);
    const menu = screen.getByRole("menu", { name: "Board menu" });
    expect(menu.style.width).toBe("220px");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    const share = screen.getByRole("menuitem", { name: "Share" });
    expect(document.activeElement).toBe(share);
    expect(share.getAttribute("aria-haspopup")).toBe("menu");

    fireEvent.keyDown(share, { key: "ArrowRight" });
    expect(share.getAttribute("aria-expanded")).toBe("true");
    const submenu = screen.getByRole("menu", { name: "Share" });
    expect(document.activeElement?.textContent).toBe("Export PNG…");
    fireEvent.keyDown(submenu, { key: "ArrowLeft" });
    expect(screen.queryByRole("menu", { name: "Share" })).toBeNull();
    expect(document.activeElement).toBe(share);
    expect(screen.getByRole("menu", { name: "Board menu" })).toBeTruthy();

    fireEvent.keyDown(share, { key: "Enter" });
    fireEvent.keyDown(screen.getByRole("menu", { name: "Share" }), { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Share" })).toBeNull();
    expect(screen.getByRole("menu", { name: "Board menu" })).toBeTruthy();

    fireEvent.click(share);
    fireEvent.click(screen.getByRole("menuitem", { name: "Export PNG…" }));
    expect(onExport).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("opens on hover, closes when another row is hovered, and survives presses inside", () => {
    const onDelete = vi.fn();
    render(<MenuButton variant="icon" icon="dots" label="Board menu" entries={boardMenu(vi.fn(), onDelete)} />);
    fireEvent.click(screen.getByRole("button", { name: "Board menu" }));
    const share = screen.getByRole("menuitem", { name: "Share" });
    fireEvent.pointerMove(share);
    expect(document.activeElement).toBe(share);
    const copy = screen.getByRole("menuitem", { name: "Copy link" });
    act(() => {
      copy.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(screen.getByRole("menu", { name: "Share" })).toBeTruthy();
    fireEvent.pointerMove(screen.getByRole("menuitem", { name: "Rename…" }));
    expect(screen.queryByRole("menu", { name: "Share" })).toBeNull();
    const remove = screen.getByRole("menuitem", { name: "Delete…" });
    expect(remove.classList.contains("menu-item-danger")).toBe(true);
    fireEvent.click(remove);
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it("lines up labels: an icon column only when an item has an icon, a trailing column for chevrons", () => {
    render(<MenuButton variant="icon" icon="dots" label="Board menu" entries={boardMenu(vi.fn(), vi.fn())} />);
    fireEvent.click(screen.getByRole("button", { name: "Board menu" }));
    const rename = screen.getByRole("menuitem", { name: "Rename…" });
    expect(rename.querySelector(".menu-item-icon")).toBeNull();
    expect(rename.querySelector(".menu-item-trailing")).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Share" }).querySelector(".menu-item-trailing [data-icon='chevron-right']")).not.toBeNull();
  });
});
