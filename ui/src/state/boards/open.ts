/**
 * Opening and closing boards in this window: `open_board` and `new_board` claim a
 * board, a board held elsewhere focuses that window instead, and closing saves first, then releases
 * the claim. A board's session and its tab come and go together.
 */

import { showToast } from "../../components/Toast";
import { toUnfoldError } from "../../ipc/unfold";
import type { ProjectId } from "../../ipc/generated";
import { assertTabRoom, openBoardTab, removeBoardTab, useTabs } from "../tabStore";
import { call, windowCommand } from "./engine";
import { boardKey } from "./keys";
import {
  adoptBoardState,
  flushAllBoards,
  flushBoard,
  flushBoardView,
  forgetBoard,
  getOpenBoard,
  handleOutsideChange,
  setActivePage,
} from "./store";
import type { BoardRef, PageId, WindowId } from "./types";

export type OpenOutcome = { readonly status: "opened"; readonly ref: BoardRef } | { readonly status: "held"; readonly heldBy: WindowId };

const opening = new Map<string, Promise<OpenOutcome>>();

async function claim(ref: BoardRef, activate: boolean, page: PageId | undefined, focusHolder: boolean): Promise<OpenOutcome> {
  assertTabRoom(ref);
  const result = await call("open_board", { project: ref.project, id: ref.id });
  if (result.status === "held") {
    if (focusHolder) {
      await windowCommand("focus_window", { windowId: result.heldBy, board: { project: ref.project, board: ref.id } });
    }
    return { status: "held", heldBy: result.heldBy };
  }
  const opened = adoptBoardState(result.board, { page: page ?? null });
  openBoardTab(opened, { activate });
  return { status: "opened", ref: opened };
}

/**
 * Opens a board in a tab of this window (claiming it), on `page` when given, or shows its tab if it is
 * already open here. A board another window holds is focused there instead. In the `background` (for
 * an agent) the tab is added without switching to it, a board open here keeps the page it shows, and
 * a window that holds the board is left alone. Rejects with `too_many_tabs` when the window already
 * has 100 board tabs.
 */
export async function openBoard(
  ref: BoardRef,
  options: { readonly activate?: boolean; readonly page?: PageId; readonly background?: boolean } = {},
): Promise<OpenOutcome> {
  const background = options.background === true;
  const activate = !background && (options.activate ?? true);
  if (getOpenBoard(ref)) {
    if (options.page !== undefined && !background) {
      setActivePage(ref, options.page);
    }
    openBoardTab(ref, { activate });
    return { status: "opened", ref };
  }
  // A second click while the first open is on its way waits for it rather than claiming twice.
  const key = boardKey(ref);
  let pending = opening.get(key);
  if (!pending) {
    pending = claim(ref, activate, options.page, !background).finally(() => {
      opening.delete(key);
    });
    opening.set(key, pending);
  }
  return pending;
}

/** Creates an empty draft in `project`, opens it, and activates its tab. */
export async function newBoard(
  project: ProjectId,
  options: { readonly title?: string; readonly folder?: string; readonly pageTitle?: string } = {},
): Promise<BoardRef> {
  assertTabRoom();
  const title = options.title?.trim();
  const state = await call("new_board", {
    project,
    ...(title !== undefined && title.length > 0 ? { title } : {}),
    ...(options.folder !== undefined ? { folder: options.folder } : {}),
    ...(options.pageTitle !== undefined ? { pageTitle: options.pageTitle } : {}),
  });
  const ref = adoptBoardState(state);
  openBoardTab(ref);
  return ref;
}

/** Saves what is unsaved (edits and view), releases the claim, and closes the board's tab. */
export async function closeBoard(ref: BoardRef): Promise<void> {
  if (!getOpenBoard(ref)) {
    removeBoardTab(ref);
    return;
  }
  await Promise.all([flushBoard(ref), flushBoardView(ref)]);
  try {
    await call("close_board", { project: ref.project, id: ref.id });
  } finally {
    forgetBoard(ref);
    removeBoardTab(ref);
  }
}

/** Closes this window's boards of a project (before it is removed or switched to another worktree). */
export async function closeBoardsOfProject(project: ProjectId): Promise<void> {
  const refs = useTabs.getState().boards.filter((ref) => ref.project === project);
  await Promise.all(refs.map((ref) => closeBoard(ref)));
}

/**
 * After an engine-host restart: the claims went with the engine, so each board
 * tab is claimed again, in tab order, before anything is saved. A board another window took meanwhile
 * loses its tab; one whose file changed meanwhile reloads, or asks about its unsaved edits. Then every
 * unsaved edit and view is written. (The window's session code orders this between `select_project`
 * and `save_window_state`.)
 */
export async function reclaimBoards(): Promise<void> {
  for (const ref of useTabs.getState().boards) {
    const board = getOpenBoard(ref);
    if (!board) {
      continue;
    }
    try {
      const result = await call("open_board", { project: ref.project, id: ref.id });
      if (result.status === "held") {
        forgetBoard(ref);
        removeBoardTab(ref);
        showToast({ title: `“${board.document.title}” is open in another window`, body: "Its tab was closed here." });
      } else if (result.board.revision !== board.revision) {
        await handleOutsideChange(ref, result.board.revision);
      }
    } catch (cause) {
      const error = toUnfoldError(cause);
      if (error.code === "disconnected" || error.code === "engine_unavailable") {
        return; // The next reconnect tries again.
      }
      console.warn(`Reopening ${ref.id} failed:`, error);
    }
  }
  await flushAllBoards();
}

