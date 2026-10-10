/**
 * What Home's buttons and rows do with boards: open one, make one, trace an entrance onto a new draft,
 * and Review a board's health (V-77). Each runs from a click, so failures show as a toast rather than
 * throwing (the `flows.ts` pattern).
 */

import { showToast } from "../components";
import { toUnfoldError, type Key, type ProjectId } from "../ipc";
import { getOpenBoard, newBoard, openBoard, requestCardFocus, select, setActivePage, traceEntrance, type BoardRef } from "../state/boards";

function failed(title: string, error: unknown): void {
  const failure = toUnfoldError(error);
  showToast({
    title,
    body: failure.code === "too_many_tabs" ? "Close a tab first: a window holds at most 100 boards." : failure.message,
    tone: "danger",
  });
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

/** Opens a board on the page of its first health item, with that card selected (V-77). */
export async function reviewBoard(ref: BoardRef): Promise<void> {
  try {
    const outcome = await openBoard(ref);
    if (outcome.status !== "opened") {
      return;
    }
    const item = getOpenBoard(outcome.ref)?.health.items[0];
    if (!item) {
      return;
    }
    setActivePage(outcome.ref, item.page);
    if ("card" in item.target) {
      const target = { board: outcome.ref, page: item.page };
      select(target, { kind: "card", cardId: item.target.card, key: null });
      requestCardFocus(target, [item.target.card], "center");
    }
  } catch (error) {
    failed("Couldn't open the board", error);
  }
}
