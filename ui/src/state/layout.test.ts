import { describe, expect, it, vi } from "vitest";
import {
  applyLayoutSnapshot,
  createLayoutStore,
  initialLayout,
  layoutSnapshot,
  PANE_BOUNDS,
  subscribeToLayout,
  type LayoutSnapshot,
} from "./layout";

describe("layout store", () => {
  it("starts with the inspector closed", () => {
    expect(createLayoutStore().getState().inspectorOpen).toBe(false);
  });

  it("shows a view, and hides the sidebar when its active icon is clicked again", () => {
    const store = createLayoutStore();
    store.getState().showView("search");
    expect(store.getState()).toMatchObject({ view: "search", sidebarVisible: true });
    store.getState().showView("search", { toggle: true });
    expect(store.getState().sidebarVisible).toBe(false);
    store.getState().showView("search", { toggle: true });
    expect(store.getState().sidebarVisible).toBe(true);
  });

  it("keyboard view switches always show the sidebar", () => {
    const store = createLayoutStore({ ...initialLayout, sidebarVisible: false });
    store.getState().showView("files");
    expect(store.getState().sidebarVisible).toBe(true);
    store.getState().showView("files");
    expect(store.getState().sidebarVisible).toBe(true);
  });

  it("toggles panes, and choosing a panel tab opens the panel", () => {
    const store = createLayoutStore();
    store.getState().toggleInspector();
    store.getState().togglePanel();
    expect(store.getState()).toMatchObject({ inspectorOpen: true, panelOpen: false });
    store.getState().showPanelTab("references");
    expect(store.getState()).toMatchObject({ panelOpen: true, panelTab: "references" });
  });
});

describe("pane sizes", () => {
  it("clamps sizes to each pane's bounds and resets to the initial size", () => {
    const store = createLayoutStore();
    store.getState().setPaneSize("sidebar", 5000);
    expect(store.getState().sizes.sidebar).toBe(PANE_BOUNDS.sidebar.max);
    store.getState().setPaneSize("panel", 10);
    expect(store.getState().sizes.panel).toBe(PANE_BOUNDS.panel.min);
    store.getState().setPaneSize("inspector", 333.6);
    expect(store.getState().sizes.inspector).toBe(334);
    store.getState().resetPaneSize("sidebar");
    expect(store.getState().sizes.sidebar).toBe(PANE_BOUNDS.sidebar.initial);
  });
});

describe("layout snapshots", () => {
  it("round-trips the persisted fields", () => {
    const store = createLayoutStore();
    store.getState().showView("boards");
    store.getState().toggleInspector();
    store.getState().setPaneSize("panel", 320);
    const saved: unknown = JSON.parse(JSON.stringify(layoutSnapshot(store)));
    const restored = createLayoutStore();
    applyLayoutSnapshot(saved, restored);
    expect(layoutSnapshot(restored)).toEqual(layoutSnapshot(store));
  });

  it("ignores malformed fields from disk and clamps sizes", () => {
    const store = createLayoutStore();
    applyLayoutSnapshot({ view: "graph", panelTab: 3, inspectorOpen: "yes", panelOpen: false, sizes: { sidebar: 9999, panel: "big" } }, store);
    expect(store.getState()).toMatchObject({
      view: "files",
      panelTab: "problems",
      inspectorOpen: false,
      panelOpen: false,
      sizes: { sidebar: PANE_BOUNDS.sidebar.max, panel: PANE_BOUNDS.panel.initial },
    });
    applyLayoutSnapshot(null, store);
    applyLayoutSnapshot("nonsense", store);
    expect(store.getState().view).toBe("files");
  });

  it("notifies only when the persisted part changes", () => {
    const store = createLayoutStore();
    const onChange = vi.fn<(snapshot: LayoutSnapshot) => void>();
    const unsubscribe = subscribeToLayout(onChange, store);
    store.getState().showView("files");
    expect(onChange).not.toHaveBeenCalled();
    store.getState().setPaneSize("sidebar", 300);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.lastCall?.[0].sizes.sidebar).toBe(300);
    unsubscribe();
  });
});
