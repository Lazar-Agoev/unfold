/**
 * Placing the handler cards an entrance opens (architecture §10.2, in the spirit of following a
 * call): a column `FOLLOW_GAP` to the right of the entrance card, the first handler's header level
 * with its handler row, each next one level with its own row or just below the one above it, so
 * no arrow bends upward and no two cards overlap. The rows are only known once the entrance card
 * is drawn, so the canvas places them then, as part of the step that opened them.
 */

import { useEffect } from "react";
import { displayedRects } from "../state/boards/api";
import { moveCards } from "../state/boards/document";
import { lineOffsetIn } from "../state/boards/navigation";
import { amendBoard, getBoard, requestFocus, setPendingPlacement, useBoards } from "../state/boards/store";
import type { PendingPlacement } from "../state/boards/types";
import { FOLLOW_GAP, HEADER_PORT, STACK_GAP, fallbackSize, type Rect, type Size } from "./layout";

/** How long the canvas waits for the cards to load and measure before placing them anyway. */
const PLACEMENT_TIMEOUT_MS = 2500;
const PLACEMENT_POLL_MS = 80;

/**
 * Positions for handler cards beside `entrance`: `rows[i]` is handler i's row center below the
 * entrance card's top (undefined when unknown), `sizes[i]` its card size.
 */
export function placeHandlers(entrance: Rect, rows: readonly (number | undefined)[], sizes: readonly Size[]): [number, number][] {
  const x = Math.round(entrance.x + entrance.width + FOLLOW_GAP);
  const positions: [number, number][] = [];
  let below = Number.NEGATIVE_INFINITY;
  rows.forEach((row, index) => {
    const level = entrance.y + (row ?? HEADER_PORT) - HEADER_PORT;
    const y = Math.round(Math.max(level, below));
    positions.push([x, y]);
    below = y + (sizes[index]?.height ?? fallbackSize(false).height) + STACK_GAP;
  });
  return positions;
}

function cardElement(cardId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`.board-card[data-card-id="${CSS.escape(cardId)}"]`);
}

/** The handler rows' offsets in the drawn entrance card, or null while it is loading. */
function handlerRows(placement: PendingPlacement): (number | undefined)[] | null {
  const card = cardElement(placement.entranceId);
  if (!card || card.dataset["loading"] === "true") {
    return null;
  }
  return placement.handlers.map(({ key }) => {
    const row = [...card.querySelectorAll<HTMLElement>("[data-target-key]")].find((element) => element.dataset["targetKey"] === key);
    return row ? lineOffsetIn(row) : undefined;
  });
}

function loaded(placement: PendingPlacement): boolean {
  return placement.handlers.every(({ cardId }) => {
    const element = cardElement(cardId);
    return element !== null && element.dataset["loading"] !== "true";
  });
}

function place(boardId: string, placement: PendingPlacement, rows: readonly (number | undefined)[]): void {
  const board = getBoard(boardId);
  const entrance = board ? displayedRects(board).get(placement.entranceId) : undefined;
  if (board && entrance) {
    const sizes = placement.handlers.map(({ cardId }) => board.view.sizes[cardId] ?? fallbackSize(false));
    const positions = placeHandlers(entrance, rows, sizes);
    amendBoard(boardId, (draft) => {
      moveCards(draft, Object.fromEntries(placement.handlers.map(({ cardId }, index) => [cardId, positions[index] ?? [entrance.x, entrance.y]])));
    });
  }
  setPendingPlacement(boardId, null);
  requestFocus(boardId, [placement.entranceId, ...placement.handlers.map(({ cardId }) => cardId)], "fit");
}

/** Places the handlers of a just-opened entrance once its card is drawn (while the board is shown). */
export function usePendingPlacement(boardId: string, active: boolean): void {
  const placement = useBoards((state) => state.boards[boardId]?.view.placement ?? null);
  useEffect(() => {
    if (!placement || !active) {
      return undefined;
    }
    const started = Date.now();
    let timer: ReturnType<typeof setTimeout> | null = null;
    const attempt = () => {
      const sizes = getBoard(boardId)?.view.sizes ?? {};
      const measured = sizes[placement.entranceId] !== undefined && placement.handlers.every(({ cardId }) => sizes[cardId] !== undefined);
      const rows = measured && loaded(placement) ? handlerRows(placement) : null;
      if (rows) {
        place(boardId, placement, rows);
      } else if (Date.now() - started > PLACEMENT_TIMEOUT_MS) {
        place(boardId, placement, handlerRows(placement) ?? placement.handlers.map(() => undefined));
      } else {
        timer = setTimeout(attempt, PLACEMENT_POLL_MS);
      }
    };
    timer = setTimeout(attempt, PLACEMENT_POLL_MS);
    return () => {
      if (timer !== null) {
        clearTimeout(timer);
      }
    };
  }, [placement, active, boardId]);
}
