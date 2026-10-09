/**
 * Per-window layout state: which sidebar view shows, which panes are open, and how big they are.
 * Each window is its own renderer, so a module-level store is per window.
 *
 * Persistence (to `.unfold/local/state.json` through Rust) goes through `layoutSnapshot`,
 * `applyLayoutSnapshot`, and `subscribeToLayout`: the snapshot is plain JSON with only what should
 * come back when the window reopens.
 */

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";

export type SidebarView = "files" | "entrances" | "search" | "boards";
export type PanelTab = "problems" | "index_log" | "references";
export type Pane = "sidebar" | "inspector" | "panel";

/** Min, max, and initial size of each pane in CSS pixels (widths for the side panes, height for the panel). */
export const PANE_BOUNDS: Readonly<Record<Pane, { readonly min: number; readonly max: number; readonly initial: number }>> = {
  sidebar: { min: 180, max: 640, initial: 260 },
  inspector: { min: 240, max: 640, initial: 300 },
  panel: { min: 96, max: 720, initial: 200 },
};

export interface PaneSizes {
  readonly sidebar: number;
  readonly inspector: number;
  readonly panel: number;
}

export interface LayoutState {
  view: SidebarView;
  sidebarVisible: boolean;
  /** Closed by default (architecture §11.5). */
  inspectorOpen: boolean;
  panelOpen: boolean;
  panelTab: PanelTab;
  sizes: PaneSizes;
}

export interface LayoutActions {
  /**
   * Shows a sidebar view. With `toggle` (clicking the active activity-bar icon), showing the view
   * that is already visible hides the sidebar instead.
   */
  showView: (view: SidebarView, options?: { toggle?: boolean }) => void;
  toggleSidebar: () => void;
  toggleInspector: () => void;
  togglePanel: () => void;
  showPanelTab: (tab: PanelTab) => void;
  /** Sets a pane's size, clamped to its bounds. */
  setPaneSize: (pane: Pane, size: number) => void;
  resetPaneSize: (pane: Pane) => void;
}

export const initialLayout: LayoutState = {
  view: "files",
  sidebarVisible: true,
  inspectorOpen: false,
  panelOpen: true,
  panelTab: "problems",
  sizes: {
    sidebar: PANE_BOUNDS.sidebar.initial,
    inspector: PANE_BOUNDS.inspector.initial,
    panel: PANE_BOUNDS.panel.initial,
  },
};

function clampPane(pane: Pane, size: number): number {
  const { min, max, initial } = PANE_BOUNDS[pane];
  return Number.isFinite(size) ? Math.round(Math.min(max, Math.max(min, size))) : initial;
}

export const createLayoutStore = (initial: LayoutState = initialLayout) =>
  create<LayoutState & LayoutActions>()(
    immer((set) => ({
      ...initial,
      showView: (view, options) => {
        set((state) => {
          if (options?.toggle && state.view === view && state.sidebarVisible) {
            state.sidebarVisible = false;
          } else {
            state.view = view;
            state.sidebarVisible = true;
          }
        });
      },
      toggleSidebar: () => {
        set((state) => {
          state.sidebarVisible = !state.sidebarVisible;
        });
      },
      toggleInspector: () => {
        set((state) => {
          state.inspectorOpen = !state.inspectorOpen;
        });
      },
      togglePanel: () => {
        set((state) => {
          state.panelOpen = !state.panelOpen;
        });
      },
      showPanelTab: (tab) => {
        set((state) => {
          state.panelTab = tab;
          state.panelOpen = true;
        });
      },
      setPaneSize: (pane, size) => {
        set((state) => {
          state.sizes[pane] = clampPane(pane, size);
        });
      },
      resetPaneSize: (pane) => {
        set((state) => {
          state.sizes[pane] = PANE_BOUNDS[pane].initial;
        });
      },
    })),
  );

export type LayoutStore = ReturnType<typeof createLayoutStore>;

export const useLayout = createLayoutStore();

/** The persisted part of the layout, as plain JSON. */
export interface LayoutSnapshot {
  readonly view: SidebarView;
  readonly sidebarVisible: boolean;
  readonly inspectorOpen: boolean;
  readonly panelOpen: boolean;
  readonly panelTab: PanelTab;
  readonly sizes: PaneSizes;
}

export function layoutSnapshot(store: LayoutStore = useLayout): LayoutSnapshot {
  const { view, sidebarVisible, inspectorOpen, panelOpen, panelTab, sizes } = store.getState();
  return { view, sidebarVisible, inspectorOpen, panelOpen, panelTab, sizes: { ...sizes } };
}

const VIEWS: readonly SidebarView[] = ["files", "entrances", "search", "boards"];
const PANEL_TABS: readonly PanelTab[] = ["problems", "index_log", "references"];

/**
 * Restores a saved layout. The input comes from disk, so every field is checked: unknown or
 * malformed fields keep their current values, and sizes are clamped to the pane bounds.
 */
export function applyLayoutSnapshot(saved: unknown, store: LayoutStore = useLayout): void {
  if (typeof saved !== "object" || saved === null) {
    return;
  }
  const value = saved as Partial<Record<keyof LayoutSnapshot, unknown>>;
  const sizes = (typeof value.sizes === "object" && value.sizes !== null ? value.sizes : {}) as Partial<
    Record<Pane, unknown>
  >;
  store.setState((state) => {
    if (VIEWS.includes(value.view as SidebarView)) {
      state.view = value.view as SidebarView;
    }
    if (PANEL_TABS.includes(value.panelTab as PanelTab)) {
      state.panelTab = value.panelTab as PanelTab;
    }
    for (const key of ["sidebarVisible", "inspectorOpen", "panelOpen"] as const) {
      const flag = value[key];
      if (typeof flag === "boolean") {
        state[key] = flag;
      }
    }
    for (const pane of ["sidebar", "inspector", "panel"] as const) {
      const size = sizes[pane];
      if (typeof size === "number") {
        state.sizes[pane] = clampPane(pane, size);
      }
    }
  });
}

/** Calls `onChange` with a fresh snapshot whenever the persisted part of the layout changes. */
export function subscribeToLayout(onChange: (snapshot: LayoutSnapshot) => void, store: LayoutStore = useLayout): () => void {
  let previous = JSON.stringify(layoutSnapshot(store));
  return store.subscribe(() => {
    const snapshot = layoutSnapshot(store);
    const serialized = JSON.stringify(snapshot);
    if (serialized !== previous) {
      previous = serialized;
      onChange(snapshot);
    }
  });
}
