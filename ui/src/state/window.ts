/**
 * This window: what `window_info` said about it at boot (its id, and the board it showed last), and the
 * project its Home shows (the engine keeps the selection, `select_project`).
 */

import { create } from "zustand";
import type { ProjectId } from "../ipc/generated";
import type { WindowInfo } from "../ipc/unfold";

export interface WindowStore {
  /** Null outside the app, where there is no main process to ask. */
  readonly info: WindowInfo | null;
  /** The project Home shows; never changed by showing a board. */
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
