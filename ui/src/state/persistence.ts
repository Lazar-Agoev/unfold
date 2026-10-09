/**
 * Remembering the window (architecture §11.2): on opening a folder, the window's saved tabs,
 * layout, lens, and boards come back from `window_state`; afterwards every change is saved with
 * `save_window_state`, debounced. The window's stable `key` comes from the main process.
 */

import { call } from "../ipc/commands";
import type { JsonValue, WindowState } from "../ipc/generated";
import { adoptBoardState } from "./boards/api";
import { applyLayoutSnapshot, layoutSnapshot, subscribeToLayout } from "./layout";
import { applyLens, getLens, subscribeToLens } from "./lens";
import { select } from "./selection";
import { activateTab, applyTabsSnapshot, removeTab, subscribeToTabs, tabsSnapshot, useTabs, type BoardTab } from "./tabs";

/** How long the window waits after a change before saving. */
export const SAVE_DELAY_MS = 400;

/** The window's state as `save_window_state` takes it. Bounds stay with the main process. */
export function currentWindowState(key: string): WindowState {
  const tabs = tabsSnapshot();
  const layout: JsonValue = { ...layoutSnapshot(), sizes: { ...layoutSnapshot().sizes }, lens: getLens() };
  return { key, bounds: null, maximized: false, tabs: tabs.tabs, activeTab: tabs.activeTab, layout, openBoards: tabs.openBoards };
}

/** Reopens the boards of restored board tabs; a board that can't open (held elsewhere, gone) loses its tab. */
async function reopenBoards(tabs: readonly BoardTab[]): Promise<void> {
  await Promise.all(
    tabs.map(async (tab) => {
      try {
        const result = await call("open_board", { id: tab.boardId });
        if (result.status === "opened") {
          adoptBoardState(result.board, { activate: false });
        } else {
          removeTab(tab.id);
        }
      } catch {
        removeTab(tab.id);
      }
    }),
  );
}

/** Restores what the window with `key` remembered. Resolves once its boards are open again. */
export async function restoreWindow(key: string): Promise<void> {
  applyTabsSnapshot([], null);
  select(null);
  const saved = await call("window_state", { key }).catch(() => null);
  if (!saved) {
    return;
  }
  applyLayoutSnapshot(saved.layout);
  if (typeof saved.layout === "object" && saved.layout !== null && !Array.isArray(saved.layout)) {
    applyLens(saved.layout["lens"]);
  }
  applyTabsSnapshot(saved.tabs, saved.activeTab);
  const active = useTabs.getState().activeId;
  await reopenBoards(useTabs.getState().tabs.filter((tab): tab is BoardTab => tab.kind === "board"));
  // Opening boards announces their tabs; the saved active tab wins.
  if (active !== null) {
    activateTab(active);
  }
}

/**
 * Saves the window's state after each change (debounced). A save still waiting when the page goes
 * away (the window closes, the app quits) is sent at once, so the last change is not lost; the
 * engine host lets it land before the window's workspace closes. Returns the stop function.
 */
export function followWindowState(key: string, delay = SAVE_DELAY_MS): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    timer = null;
    call("save_window_state", { state: currentWindowState(key) }).catch((error: unknown) => {
      console.warn("Saving the window's state failed:", error);
    });
  };
  const schedule = () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = setTimeout(save, delay);
  };
  const saveNow = () => {
    if (timer !== null) {
      clearTimeout(timer);
      save();
    }
  };
  const stops = [subscribeToTabs(schedule), subscribeToLayout(schedule), subscribeToLens(schedule)];
  window.addEventListener("pagehide", saveNow);
  return () => {
    window.removeEventListener("pagehide", saveNow);
    if (timer !== null) {
      clearTimeout(timer);
    }
    for (const stop of stops) {
      stop();
    }
  };
}
