import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  SIDEBAR_WIDTH,
  applyLayoutSnapshot,
  folderKey,
  initialLayout,
  layoutSnapshot,
  setFolderExpanded,
  setHomeView,
  setSidebarWidth,
  subscribeToLayout,
  toggleSidebar,
  useLayout,
} from "./layout";

beforeEach(() => {
  useLayout.setState(initialLayout);
});

describe("window layout", () => {
  it("clamps the sidebar width and toggles its visibility", () => {
    setSidebarWidth(1000);
    expect(layoutSnapshot().sidebar.width).toBe(SIDEBAR_WIDTH.max);
    setSidebarWidth(10);
    expect(layoutSnapshot().sidebar.width).toBe(SIDEBAR_WIDTH.min);
    toggleSidebar();
    expect(layoutSnapshot().sidebar.visible).toBe(false);
  });

  it("remembers expanded folders per project, without duplicates", () => {
    setFolderExpanded("p-a", "Checkout", true);
    setFolderExpanded("p-b", "Checkout", true);
    setFolderExpanded("p-a", "Checkout", true);
    expect(layoutSnapshot().folders).toEqual([folderKey("p-b", "Checkout"), folderKey("p-a", "Checkout")]);
    setFolderExpanded("p-b", "Checkout", false);
    expect(layoutSnapshot().folders).toEqual([folderKey("p-a", "Checkout")]);
  });

  it("round-trips through JSON and notifies on change", () => {
    const changed = vi.fn();
    const stop = subscribeToLayout(changed);
    setHomeView("health");
    expect(changed).toHaveBeenCalledTimes(1);
    stop();
    const saved: unknown = JSON.parse(JSON.stringify(layoutSnapshot()));
    useLayout.setState(initialLayout);
    applyLayoutSnapshot(saved);
    expect(layoutSnapshot().home.view).toBe("health");
  });

  it("validates a saved layout field by field, falling back to defaults", () => {
    applyLayoutSnapshot({
      sidebar: { width: 9999, visible: "no" },
      selectedCollapsed: true,
      peek: { pinned: 1 },
      home: { view: "files", boardsFilter: "drafts", boardsSort: "title" },
      folders: ["p-a\u0000X", 3, "p-a\u0000X"],
      lens: "flow",
    });
    expect(layoutSnapshot()).toEqual({
      v: 1,
      sidebar: { width: SIDEBAR_WIDTH.max, visible: true },
      selectedCollapsed: true,
      peek: { pinned: false },
      home: { view: "overview", boardsFilter: "drafts", boardsSort: "title" },
      folders: ["p-a\u0000X"],
    });
    for (const junk of [null, 42, "layout", []]) {
      applyLayoutSnapshot(junk);
      expect(layoutSnapshot()).toEqual(initialLayout);
    }
  });
});
