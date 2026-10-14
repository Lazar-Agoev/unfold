/**
 * Show callers (the card menu, Selected): asks the engine for the distinct definitions that call a
 * card's definition (`callers`), and adds the ones not on the page yet as cards in a column to its
 * left, each named and sized from what the reader draws for it (in the page's default view), so each
 * one's right edge lines up. One undo step.
 */

import { queryClient } from "../ipc/client";
import type { CallingDefinition } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
import { showToast } from "../components";
import { addCard, displayedRects, findCard, findCardByKey, findPage, getOpenBoard, editPage, requestCardFocus, type PageTarget } from "../state/boards";
import { call } from "../state/boards/engine";
import { cardArgs, cardName, cardQuery, fetchCard, pageOverlay } from "../state/cards";
import { getShowAllCalls } from "../state/showAllCalls";
import { callerPlacement } from "./layout";
import { measureText } from "./measure";
import { estimatedCardSize } from "./sizing";

/** How many callers one Show callers adds at most (the engine's default page). */
const CALLERS_LIMIT = 20;
/** Adds the callers of a block card to its left (or brings those already on the page into view). */
export async function showCallers(target: PageTarget, cardId: string): Promise<void> {
  const start = getOpenBoard(target.board);
  const startPage = start ? findPage(start.document, target.page) : undefined;
  const card = startPage ? findCard(startPage, cardId) : undefined;
  if (!card || !("block" in card.ref)) {
    return;
  }
  const { project } = target.board;
  const own = await fetchCard(target, cardId).catch(() => undefined);
  const name = own ? cardName(own) : card.id;
  let total: number;
  let callers: CallingDefinition[];
  try {
    const result = await call("callers", { project, key: card.ref.block, limit: CALLERS_LIMIT });
    total = result.total;
    callers = result.callers;
  } catch (error) {
    showToast({ title: "Couldn't find the callers", body: toUnfoldError(error).message, tone: "danger" });
    return;
  }
  if (total === 0) {
    showToast({ title: `Nothing in the graph calls ${name}.` });
    return;
  }
  // What the reader draws for them sizes the new cards, and the cards draw from the same cache at once.
  const overlay = startPage ? pageOverlay(startPage) : {};
  const showAllCalls = getShowAllCalls();
  const models = await Promise.all(
    callers.map((caller) => queryClient.query(cardQuery(cardArgs(project, { ref: { block: caller.key } }, overlay, showAllCalls))).catch(() => undefined)),
  );

  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const rect = board ? displayedRects(board, target.page).get(cardId) : undefined;
  if (!board || !page || !rect) {
    return;
  }
  const present = callers.flatMap((caller) => findCardByKey(page, caller.key)?.id ?? []);
  // New cards have no view of their own: they show the page's default.
  const missing = callers.flatMap((caller, index) => {
    if (findCardByKey(page, caller.key)) {
      return [];
    }
    const model = models[index];
    return [{ key: caller.key, name: model ? cardName(model) : caller.name, size: estimatedCardSize({ model, view: page.defaultView }, measureText) }];
  });
  const positions = callerPlacement(
    rect,
    missing.map((item) => item.size),
  );
  const added =
    missing.length > 0
      ? (editPage(target, (draft) =>
          missing.map(({ key, name: callerName }, index) => addCard(draft, { ref: { block: key }, name: callerName, at: positions[index] ?? [rect.x, rect.y] })),
        ) ?? [])
      : [];
  requestCardFocus(target, [cardId, ...present, ...added], "fit");
  if (total > callers.length) {
    showToast({ title: `Showing the first ${callers.length} of ${total} callers of ${name}.` });
  }
}
