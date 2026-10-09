// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ListRow } from "./ListRow";

describe("ListRow", () => {
  it("lays out its slots and keeps an empty 16px trailing slot", () => {
    const { container } = render(<ListRow role="option" label="Placing an order" leading={<span>p</span>} selected aria-selected />);
    const row = screen.getByRole("option");
    expect(row.querySelector(".list-row-label")?.textContent).toBe("Placing an order");
    expect(row.classList.contains("list-row-selected")).toBe(true);
    expect(container.querySelector(".list-row-trailing")?.classList.contains("list-row-trailing-filled")).toBe(false);
  });

  it("indents by depth and runs onClick from Enter", () => {
    const onClick = vi.fn();
    render(<ListRow role="treeitem" tabIndex={0} label="reserveInventory" depth={3} trailing="not connected" muted onClick={onClick} />);
    const row = screen.getByRole("treeitem");
    expect(row.style.paddingLeft).toBe("calc(var(--list-row-inline, 8px) + 48px)");
    expect(row.querySelector(".list-row-trailing-filled")?.textContent).toBe("not connected");
    expect(row.classList.contains("list-row-muted")).toBe(true);
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.click(row);
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it("holds a menu button after the trailing slot", () => {
    const onContextMenu = vi.fn();
    render(<ListRow label="storefront-api" menu={<button type="button">More</button>} indent={18} depth={1} onContextMenu={onContextMenu} />);
    const more = screen.getByRole("button", { name: "More" });
    expect(more.parentElement?.className).toBe("list-row-menu");
    fireEvent.contextMenu(more);
    expect(onContextMenu).toHaveBeenCalledTimes(1);
  });
});
