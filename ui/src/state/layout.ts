/**
 * The window's layout: the Selected section's collapse, the peek sheet's pin, Home's view and boards
 * filter, and the expanded sidebar folders. It lasts only while the app runs; the board lens is not
 * part of it (it is per page).
 */

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { ProjectId } from "../ipc/generated";

export type HomeView = "overview" | "health" | "activity" | "recents" | "drafts";
export type BoardsFilter = "all" | "published" | "drafts";
export type BoardsSort = "edited" | "title";

export interface WindowLayout {
  readonly selectedCollapsed: boolean;
  readonly peek: { readonly pinned: boolean };
  readonly home: { readonly view: HomeView; readonly boardsFilter: BoardsFilter; readonly boardsSort: BoardsSort };
  /** Expanded sidebar folders, as `folderKey(project, path)`, oldest first. */
  readonly folders: readonly string[];
}

/** At most this many expanded folders are remembered (the oldest are forgotten). */
const FOLDERS_LIMIT = 500;

export const initialLayout: WindowLayout = {
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
