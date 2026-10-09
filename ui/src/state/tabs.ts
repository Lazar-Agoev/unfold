/**
 * The window's tabs (UI plan §3.3): Home, implicit, always first and never closable, then board tabs
 * in order, one tab active. A board tab exists exactly while its board is open in this window, so
 * closing one saves and closes the board.
 */

import type { WindowTab } from "../ipc/generated";
import { boardKey } from "./boards/keys";
import { closeBoard } from "./boards/open";
import { getOpenBoard, useBoards } from "./boards/store";
import { useTabs, type TabKey } from "./tabStore";

export {
  MAX_BOARD_TABS,
  activateBoard,
  activateHome,
  activateTab,
  getActiveBoard,
  moveTab,
  nextTab,
  openBoardTab,
  previousTab,
  renameBoardTab,
  useActiveBoard,
  useTabs,
  type TabKey,
  type TabsState,
} from "./tabStore";

/** Closes a board tab: saves the board's edits and view, releases it, then removes the tab. Home never closes. */
export async function closeTab(key: TabKey): Promise<void> {
  const ref = useTabs.getState().boards.find((board) => boardKey(board) === key);
  if (ref) {
    await closeBoard(ref);
  }
}

/** Closes every board tab but `key` (all of them for Home). */
export async function closeOtherTabs(key: TabKey): Promise<void> {
  const others = useTabs.getState().boards.filter((board) => boardKey(board) !== key);
  await Promise.all(others.map((ref) => closeBoard(ref)));
}

/** The tabs as `save_window_state` stores them: board tabs in order with their active page, and the active index (null: Home). */
export function tabsSnapshot(): { readonly tabs: WindowTab[]; readonly activeTab: number | null } {
  const { boards, active } = useTabs.getState();
  const index = boards.findIndex((ref) => boardKey(ref) === active);
  return {
    tabs: boards.map((ref) => ({ project: ref.project, board: ref.id, page: getOpenBoard(ref)?.activePage ?? null })),
    activeTab: index < 0 ? null : index,
  };
}

/**
 * Calls `onChange` with a fresh snapshot whenever the tabs, the active tab, or an open board's active page
 * change (what the window state saves). Returns the unsubscribe function.
 */
export function subscribeToTabs(onChange: (snapshot: ReturnType<typeof tabsSnapshot>) => void): () => void {
  let previous = JSON.stringify(tabsSnapshot());
  const check = () => {
    const snapshot = tabsSnapshot();
    const serialized = JSON.stringify(snapshot);
    if (serialized !== previous) {
      previous = serialized;
      onChange(snapshot);
    }
  };
  const stops = [useTabs.subscribe(check), useBoards.subscribe(check)];
  return () => {
    for (const stop of stops) {
      stop();
    }
  };
}
