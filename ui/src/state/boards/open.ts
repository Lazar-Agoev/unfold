/**
 * Opening and closing boards: `open_board` and `new_board` open a board's session, which stays for as
 * long as the app runs, with its edits and undo history. Closing (when the board's project is removed,
 * or its file is gone) saves first, then closes it in the engine.
 */

import { toUnfoldError } from "../../ipc/unfold";
import type { ProjectId } from "../../ipc/generated";
import { hideBoard, showBoard } from "../shownBoard";
import { call } from "./engine";
import { boardKey } from "./keys";
import { adoptBoardState, adoptOutsideChange, flushAllBoards, flushBoard, forgetBoard, getOpenBoard, openBoards, setActivePage } from "./store";
import type { BoardRef, PageId } from "./types";

const opening = new Map<string, Promise<BoardRef>>();

/**
 * Shows a board, opening it the first time, on `page` when given. In the `background` (for an agent)
 * the board opens without being shown, and a board already open keeps the page it shows.
 */
export async function openBoard(ref: BoardRef, options: { readonly page?: PageId; readonly background?: boolean } = {}): Promise<void> {
  const background = options.background === true;
  if (getOpenBoard(ref)) {
    if (options.page !== undefined && !background) {
      setActivePage(ref, options.page);
    }
  } else {
    // A second click while the first open is on its way waits for it rather than opening twice.
    const key = boardKey(ref);
    let pending = opening.get(key);
    if (!pending) {
      pending = call("open_board", { project: ref.project, id: ref.id })
        .then((state) => adoptBoardState(state, { page: options.page ?? null }))
        .finally(() => {
          opening.delete(key);
        });
      opening.set(key, pending);
    }
    await pending;
  }
  if (!background) {
    showBoard(ref);
  }
}

/** Creates an empty board file in `project`'s `.unfold/boards/`, opens it, and shows it. */
export async function newBoard(
  project: ProjectId,
  options: { readonly title?: string; readonly folder?: string; readonly pageTitle?: string } = {},
): Promise<BoardRef> {
  const title = options.title?.trim();
  const state = await call("new_board", {
    project,
    ...(title !== undefined && title.length > 0 ? { title } : {}),
    ...(options.folder !== undefined ? { folder: options.folder } : {}),
    ...(options.pageTitle !== undefined ? { pageTitle: options.pageTitle } : {}),
  });
  const ref = adoptBoardState(state);
  showBoard(ref);
  return ref;
}

/**
 * Saves the board's unsaved edits, closes it in the engine, and forgets its session. Home shows if it
 * was in front. Throws `unsaved_edits`, keeping the board open, when its edits can't be saved.
 */
export async function closeBoard(ref: BoardRef): Promise<void> {
  if (getOpenBoard(ref)) {
    await flushBoard(ref);
    try {
      await call("close_board", { project: ref.project, id: ref.id });
    } finally {
      forgetBoard(ref);
    }
  }
  hideBoard(ref);
}

/** Closes the open boards of a project (before it is removed). */
export async function closeBoardsOfProject(project: ProjectId): Promise<void> {
  const refs = openBoards()
    .map((board) => board.ref)
    .filter((ref) => ref.project === project);
  await Promise.all(refs.map((ref) => closeBoard(ref)));
}

/**
 * After an engine-host restart: the engine came back with no board open, so each open board is opened
 * again before anything is saved. One whose file changed meanwhile takes the file's version, unless it
 * has edits that couldn't be saved: those are kept, and written over the file next. Then every unsaved
 * edit is written. (The window's session code orders this after `select_project`.)
 */
export async function reopenBoards(): Promise<void> {
  for (const board of openBoards()) {
    const { ref } = board;
    try {
      const state = await call("open_board", { project: ref.project, id: ref.id });
      if (state.revision !== board.revision) {
        adoptOutsideChange(state);
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
