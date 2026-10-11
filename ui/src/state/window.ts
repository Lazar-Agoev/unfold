/**
 * This window: its id, stable key, and platform (from `window_info`), and the project its Home shows
 * (per window, app-data state; saved in `windows.json`). Each window is its own renderer, so a module-level
 * store is per window.
 */

import { create } from "zustand";
import type { ProjectId } from "../ipc/generated";
import type { WindowInfo } from "../ipc/unfold";

export interface WindowStore {
  /** Null outside the app, where there is no main process to ask. */
  readonly info: WindowInfo | null;
  /** The project Home shows; never changed by activating a board tab. */
  readonly selectedProject: ProjectId | null;
}

export const useWindowStore = create<WindowStore>()(() => ({ info: null, selectedProject: null }));

/** Records what `window_info` said about this window (at boot). */
export function setWindowInfo(info: WindowInfo | null): void {
  useWindowStore.setState({ info });
}

/** The project Home shows, kept current. */
export function useSelectedProject(): ProjectId | null {
  return useWindowStore((state) => state.selectedProject);
}
