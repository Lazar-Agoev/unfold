/**
 * Opening and closing folders from the UI: the folder picker, a folder dropped on the window, a
 * recent folder by path, and Close Folder. The main process opens them (and brings back their saved windows); the window
 * learns its new folder from the `folder_changed` event.
 *
 * A folder opening in this window replaces the one it shows, and Close Folder returns the window to
 * the welcome screen; either way the window leaves the old folder's workspace, so unsaved board
 * edits are written first, while the engine still has that folder.
 */

import { showToast } from "../components";
import type { DroppedFile, OpenedIn } from "@unfold/desktop/unfold-api";
import { bridge, toUnfoldError, windowCommand } from "../ipc/unfold";
import { flushAllBoards } from "../state/boards/store";
import { useWindow } from "../state/window";

export type OpenTarget = "this_window" | "new_window";

function adopt(opened: OpenedIn | null): void {
  const info = useWindow.getState().info;
  if (opened && opened.windowId === info?.windowId) {
    useWindow.getState().setFolder(opened.folder);
  }
}

/** Writes this window's unsaved board edits if the folder will open here. */
async function beforeReplacing(target: OpenTarget): Promise<void> {
  if (target === "this_window") {
    await flushAllBoards();
  }
}

function fail(error: unknown): void {
  showToast({ message: `Couldn't open the folder: ${toUnfoldError(error).message}`, tone: "danger" });
}

/** Shows the folder picker; the chosen folder opens here or in a new window. */
export async function chooseFolder(target: OpenTarget = "this_window"): Promise<void> {
  await beforeReplacing(target);
  adopt(await windowCommand("choose_folder", { target }));
}

/** Opens a folder dropped on the window (the drop event's first file). */
export async function openDroppedFolder(file: DroppedFile, target: OpenTarget = "this_window"): Promise<void> {
  const api = bridge();
  if (!api) {
    return;
  }
  try {
    await beforeReplacing(target);
    adopt(await api.openDroppedFolder(file, { target }));
  } catch (error) {
    const failure = toUnfoldError(error);
    showToast({
      message: failure.code === "not_a_folder" ? "Drop a folder to open it." : `Couldn't open the folder: ${failure.message}`,
      tone: "danger",
    });
  }
}

/**
 * Opens a recent folder by its path, here or in a new window: the main process's `open_folder`
 * window command, which accepts only a folder the engine lists in `recent_folders`.
 */
export async function openRecentFolder(path: string, target: OpenTarget = "this_window"): Promise<void> {
  if (!bridge()) {
    return;
  }
  try {
    await beforeReplacing(target);
    adopt(await windowCommand("open_folder", { path, target }));
  } catch (error) {
    fail(error);
  }
}

/**
 * Close Folder: writes this window's unsaved board edits, then has the main process detach the
 * window from its folder and show the welcome screen (the window stays open).
 */
export async function closeFolder(): Promise<void> {
  if ((useWindow.getState().info?.folder ?? null) === null) {
    return;
  }
  await flushAllBoards();
  const closed = await windowCommand("close_folder");
  if (closed) {
    useWindow.getState().setFolder(null, closed.windowKey);
  }
}
