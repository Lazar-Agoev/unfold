/**
 * What Home's buttons and rows do with boards: open one, make one, trace an entrance onto a new draft,
 * and Review a board's health. Each runs from a click, so failures show as a toast rather than
 * throwing (the `flows.ts` pattern).
 */

import { showToast } from "../components";
import { toUnfoldError, type Key, type ProjectId } from "../ipc";
import { getOpenBoard, newBoard, openBoard, requestCardFocus, select, setActivePage, traceEntrance, type BoardRef } from "../state/boards";

function failed(title: string, error: unknown): void {
  showToast({ title, body: toUnfoldError(error).message, tone: "danger" });
}

export async function openBoardFromHome(ref: BoardRef): Promise<void> {
  try {
    await openBoard(ref);
  } catch (error) {
    failed("Couldn't open the board", error);
  }
}

export async function newBoardIn(project: ProjectId, options: { readonly folder?: string } = {}): Promise<void> {
  try {
    await newBoard(project, options);
  } catch (error) {
    failed("Couldn't make a board", error);
  }
}

/** Opens the entrance on a new draft board (Trace →). */
export async function traceFromHome(project: ProjectId, entrance: Key): Promise<void> {
  try {
    await traceEntrance(project, entrance);
  } catch (error) {
    failed("Couldn't trace the entrance", error);
  }
}

/** Opens a board on the page of its first health item, with that card selected. */
export async function reviewBoard(ref: BoardRef): Promise<void> {
  try {
    await openBoard(ref);
    const item = getOpenBoard(ref)?.health.items[0];
    if (!item) {
      return;
    }
    setActivePage(ref, item.page);
    if ("card" in item.target) {
      const target = { board: ref, page: item.page };
      select(target, { kind: "card", cardId: item.target.card, key: null });
      requestCardFocus(target, [item.target.card], "center");
    }
  } catch (error) {
    failed("Couldn't open the board", error);
  }
}
