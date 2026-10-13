/**
 * The window's layout: the Selected section's collapse, the peek sheet's pin, Home's view, and the
 * expanded sidebar folders. It lasts only while the app runs; the board lens is not part of it (it is
 * per page).
 */

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { ProjectId } from "../ipc/generated";

export type HomeView = "overview" | "health";

export interface WindowLayout {
  readonly selectedCollapsed: boolean;
  readonly peek: { readonly pinned: boolean };
  readonly home: { readonly view: HomeView };
  /** Expanded sidebar folders, as `folderKey(project, folder)`, oldest first. */
  readonly folders: readonly string[];
}

/** At most this many expanded folders are remembered (the oldest are forgotten). */
const FOLDERS_LIMIT = 500;

export const initialLayout: WindowLayout = {
  selectedCollapsed: false,
  peek: { pinned: false },
  home: { view: "overview" },
  folders: [],
};

export const useLayout = create<WindowLayout>()(immer(() => initialLayout));

/** The key of a sidebar folder in `WindowLayout.folders`. */
export function folderKey(project: ProjectId, folder: string): string {
  return `${project}\u0000${folder}`;
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

export function setFolderExpanded(project: ProjectId, folder: string, expanded: boolean): void {
  const key = folderKey(project, folder);
  useLayout.setState((state) => {
    const folders = state.folders.filter((folder) => folder !== key);
    if (expanded) {
      folders.push(key);
    }
    state.folders = folders.slice(-FOLDERS_LIMIT);
  });
}
