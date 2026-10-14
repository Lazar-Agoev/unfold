/**
 * Following calls on a page: a followed target opens `FOLLOW_GAP` to the right of the calling card with
 * its header level with the calling line, or the existing card is focused.
 */

import { HEADER_PORT, followPlacement } from "../../canvas/layout";
import { displayedRects, focusCard } from "./api";
import { addCard, findCard, findCardByKey, findPage } from "./document";
import { editPage, getOpenBoard, requestCardFocus, select } from "./store";
import type { PageTarget } from "./types";

/**
 * The calling line's vertical center below the top of its card, in canvas units, from the clicked
 * element. Rows can wrap, so a code row counts its first visual line.
 */
export function lineOffsetIn(anchor: HTMLElement | null): number | undefined {
  const card = anchor?.closest<HTMLElement>("[data-card-id]");
  if (!anchor || !card) {
    return undefined;
  }
  const cardBox = card.getBoundingClientRect();
  // The canvas zoom scales the card; offsetWidth is its unscaled width.
  const scale = card.offsetWidth > 0 && cardBox.width > 0 ? cardBox.width / card.offsetWidth : 1;
  const row = anchor.closest<HTMLElement>("[data-port-row]") ?? anchor;
  const rowBox = row.getBoundingClientRect();
  const firstLine = row.dataset["portRow"] === "code" ? Math.min(rowBox.height / 2, 10.5 * scale) : rowBox.height / 2;
  return (rowBox.top - cardBox.top + firstLine) / scale;
}

export interface FollowRequest {
  readonly target: PageTarget;
  /** The card the call is written in. */
  readonly fromCardId: string;
  /** The block to open. */
  readonly targetKey: string;
  /** What the block is called (the drawn flow's name for the callee), which the new card's id is made from. */
  readonly name: string;
  /** The calling line's center below the source card's top (defaults to its header). */
  readonly lineOffset?: number;
}

/** Follows a call: opens the target beside the calling line, or focuses it. Returns the target card's id. */
export function followCall(request: FollowRequest): string | undefined {
  const { target } = request;
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const source = page ? findCard(page, request.fromCardId) : undefined;
  if (!board || !page || !source) {
    return undefined;
  }
  const existing = findCardByKey(page, request.targetKey);
  if (existing) {
    focusCard(target, existing.id);
    return existing.id;
  }
  const rect = displayedRects(board, target.page).get(request.fromCardId);
  const at = rect ? followPlacement(rect, request.lineOffset ?? HEADER_PORT) : ([0, 0] as [number, number]);
  const cardId = editPage(target, (draft) => addCard(draft, { ref: { block: request.targetKey }, name: request.name, at }));
  if (cardId === undefined) {
    return undefined;
  }
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
  return cardId;
}
