/** What this window shows: its id, its stable key, its folder, and the platform it runs on. */

import { create } from "zustand";
import type { WindowInfo } from "../ipc/unfold";

export interface WindowState {
  /** Null outside the app, where there is no main process to ask. */
  info: WindowInfo | null;
  setInfo: (info: WindowInfo | null) => void;
  /** The window now shows another folder (or none); `key` is its stable key from now on. */
  setFolder: (folder: string | null, key?: string) => void;
}

export const useWindow = create<WindowState>()((set) => ({
  info: null,
  setInfo: (info) => {
    set({ info });
  },
  setFolder: (folder, key) => {
    set((state) => (state.info ? { info: { ...state.info, folder, key: key ?? state.info.key } } : state));
  },
}));

/** The last segment of a folder path, for titles. */
export function folderName(folder: string): string {
  const parts = folder.split(/[\\/]/).filter((part) => part.length > 0);
  return parts.at(-1) ?? folder;
}

/** A folder path for display: the home folder shortened to `~`. */
export function displayPath(folder: string): string {
  return folder.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, "~").replace(/^[A-Za-z]:\\Users\\[^\\]+(?=\\|$)/, "~");
}
