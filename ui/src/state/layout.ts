/**
 * The window's layout (UI plan §3.9, V-79): the sidebar's width and visibility, the Selected section's
 * collapse, the peek sheet's pin, Home's view and boards filter, and the expanded sidebar folders. It
 * is saved with the window (`windows.json` `layout`) as plain JSON; the board lens is not part of it
 * (it is per page, V-55).
 */

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { ProjectId } from "../ipc/generated";

export type HomeView = "overview" | "health" | "activity" | "recents" | "drafts";
export type BoardsFilter = "all" | "published" | "drafts";
export type BoardsSort = "edited" | "title";

export interface WindowLayout {
  readonly v: 1;
  readonly sidebar: { readonly width: number; readonly visible: boolean };
  readonly selectedCollapsed: boolean;
  readonly peek: { readonly pinned: boolean };
  readonly home: { readonly view: HomeView; readonly boardsFilter: BoardsFilter; readonly boardsSort: BoardsSort };
  /** Expanded sidebar folders, as `folderKey(project, path)`, oldest first. */
  readonly folders: readonly string[];
}

/** The sidebar's width bounds and default, in CSS pixels. */
export const SIDEBAR_WIDTH = { min: 220, max: 360, initial: 264 } as const;

/** At most this many expanded folders are remembered (the oldest are forgotten). */
const FOLDERS_LIMIT = 500;

export const initialLayout: WindowLayout = {
  v: 1,
  sidebar: { width: SIDEBAR_WIDTH.initial, visible: true },
  selectedCollapsed: false,
  peek: { pinned: false },
  home: { view: "overview", boardsFilter: "all", boardsSort: "edited" },
  folders: [],
};

export const useLayout = create<WindowLayout>()(immer(() => initialLayout));

/** The key of a sidebar folder in `WindowLayout.folders`. */
export function folderKey(project: ProjectId, path: string): string {
  return `${project}\u0000${path}`;
}

function clampWidth(width: number): number {
  return Number.isFinite(width) ? Math.round(Math.min(SIDEBAR_WIDTH.max, Math.max(SIDEBAR_WIDTH.min, width))) : SIDEBAR_WIDTH.initial;
}

export function setSidebarWidth(px: number): void {
  useLayout.setState((state) => {
    state.sidebar.width = clampWidth(px);
  });
}

export function toggleSidebar(): void {
  useLayout.setState((state) => {
    state.sidebar.visible = !state.sidebar.visible;
  });
}

export function setSelectedCollapsed(collapsed: boolean): void {
  useLayout.setState((state) => {
    state.selectedCollapsed = collapsed;
  });
}

export function setPeekPinned(pinned: boolean): void {
  useLayout.setState((state) => {
    state.peek.pinned = pinned;
  });
}

export function setHomeView(view: HomeView): void {
  useLayout.setState((state) => {
    state.home.view = view;
  });
}

export function setBoardsFilter(filter: BoardsFilter): void {
  useLayout.setState((state) => {
    state.home.boardsFilter = filter;
  });
}

export function setBoardsSort(sort: BoardsSort): void {
  useLayout.setState((state) => {
    state.home.boardsSort = sort;
  });
}

export function setFolderExpanded(project: ProjectId, path: string, expanded: boolean): void {
  const key = folderKey(project, path);
  useLayout.setState((state) => {
    const folders = state.folders.filter((folder) => folder !== key);
    if (expanded) {
      folders.push(key);
    }
    state.folders = folders.slice(-FOLDERS_LIMIT);
  });
}

/** The layout as `save_window_state` stores it. */
export function layoutSnapshot(): WindowLayout {
  return useLayout.getState();
}

const HOME_VIEWS: readonly HomeView[] = ["overview", "health", "activity", "recents", "drafts"];
const FILTERS: readonly BoardsFilter[] = ["all", "published", "drafts"];
const SORTS: readonly BoardsSort[] = ["edited", "title"];

function record(value: unknown): Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function oneOf<T extends string>(values: readonly T[], value: unknown, fallback: T): T {
  return values.includes(value as T) ? (value as T) : fallback;
}

function flag(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/**
 * Restores a saved layout. The input comes from disk, so every field is checked: missing or malformed
 * fields take their defaults, and the width is clamped to its bounds.
 */
export function applyLayoutSnapshot(json: unknown): void {
  const saved = record(json);
  const sidebar = record(saved["sidebar"]);
  const peek = record(saved["peek"]);
  const home = record(saved["home"]);
  const folders = Array.isArray(saved["folders"]) ? saved["folders"].filter((folder): folder is string => typeof folder === "string") : [];
  const fallback = initialLayout;
  useLayout.setState({
    v: 1,
    sidebar: {
      width: typeof sidebar["width"] === "number" ? clampWidth(sidebar["width"]) : fallback.sidebar.width,
      visible: flag(sidebar["visible"], fallback.sidebar.visible),
    },
    selectedCollapsed: flag(saved["selectedCollapsed"], fallback.selectedCollapsed),
    peek: { pinned: flag(peek["pinned"], fallback.peek.pinned) },
    home: {
      view: oneOf(HOME_VIEWS, home["view"], fallback.home.view),
      boardsFilter: oneOf(FILTERS, home["boardsFilter"], fallback.home.boardsFilter),
      boardsSort: oneOf(SORTS, home["boardsSort"], fallback.home.boardsSort),
    },
    folders: [...new Set(folders)].slice(-FOLDERS_LIMIT),
  });
}

/** Calls `onChange` with the layout whenever it changes. Returns the unsubscribe function. */
export function subscribeToLayout(onChange: (layout: WindowLayout) => void): () => void {
  return useLayout.subscribe((layout, previous) => {
    if (layout !== previous) {
      onChange(layout);
    }
  });
}
