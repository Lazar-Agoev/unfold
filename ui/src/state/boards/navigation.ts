/**
 * Following calls on a board (architecture §10.2): a followed target opens `FOLLOW_GAP` to the right
 * of the calling card with its header level with the calling line, or the existing card is focused
 * (and unfolded). Every follow is a focus move for Back and Forward and extends the breadcrumb trail.
 */

import { HEADER_PORT, followPlacement } from "../../canvas/layout";
import { displayedRects, focusCard } from "./api";
import { addCard, findCardByKey } from "./document";
import { editBoard, getBoard, pushFocusMove, requestFocus, selectCard } from "./store";
import type { TrailItem } from "./types";

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

/**
 * The trail after following from `fromCard` (at `line`) to `to`: everything after the source
 * drops. A source not on the trail (a handler an entrance opened) joins it at the end, so the
 * trail reads "POST /orders › createOrder › OrderService.place".
 */
export function extendTrail(trail: readonly TrailItem[], from: TrailItem, line: number | undefined, to: TrailItem): TrailItem[] {
  const index = trail.findIndex((item) => item.cardId === from.cardId);
  const base = index >= 0 ? trail.slice(0, index + 1) : [...trail.filter((item) => item.cardId !== to.cardId), from];
  const last = base[base.length - 1];
  if (last) {
    base[base.length - 1] = line !== undefined ? { ...last, line } : { cardId: last.cardId, label: last.label };
  }
  return [...base, to];
}

export interface FollowRequest {
  readonly boardId: string;
  /** The card the call is written in. */
  readonly fromCardId: string;
  readonly fromLabel: string;
  /** The block to open. */
  readonly targetKey: string;
  readonly targetLabel: string;
  /** The calling line's center below the source card's top (defaults to its header). */
  readonly lineOffset?: number;
  /** The calling line's number, for the trail. */
  readonly line?: number;
}

/** Follows a call: opens the target beside the calling line, or focuses it. Returns the target card's id. */
export function followCall(request: FollowRequest): string | undefined {
  const board = getBoard(request.boardId);
  if (!board) {
    return undefined;
  }
  const from: TrailItem = { cardId: request.fromCardId, label: request.fromLabel };
  const existing = findCardByKey(board.document, request.targetKey);
  if (existing) {
    pushFocusMove(request.boardId, extendTrail(board.view.trail, from, request.line, { cardId: existing.id, label: request.targetLabel }));
    focusCard(request.boardId, existing.id);
    return existing.id;
  }
  const source = displayedRects(board).get(request.fromCardId);
  const at = source ? followPlacement(source, request.lineOffset ?? HEADER_PORT) : ([0, 0] as [number, number]);
  const cardId = editBoard(request.boardId, (document) => addCard(document, { ref: { block: request.targetKey }, at }));
  if (cardId === undefined) {
    return undefined;
  }
  pushFocusMove(request.boardId, extendTrail(board.view.trail, from, request.line, { cardId, label: request.targetLabel }));
  selectCard(request.boardId, cardId);
  requestFocus(request.boardId, [cardId], "center");
  return cardId;
}
