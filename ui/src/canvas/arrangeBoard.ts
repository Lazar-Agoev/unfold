/** Arrange by call depth on an open board: one undoable move of every card (architecture §11.8). */

import { displayedRects } from "../state/boards/api";
import { moveCards } from "../state/boards/document";
import { editBoard, getBoard } from "../state/boards/store";
import type { Arrow } from "./arrows";
import { HEADER_PORT, arrange } from "./layout";

/**
 * Lays the board out in columns by call depth from its entrances (then the cards nothing calls),
 * in a stable order: entrances first, then left to right and top to bottom. `portOffset` gives a
 * port's measured offset below its card's top. Returns how many cards moved.
 */
export function arrangeBoard(boardId: string, arrows: readonly Arrow[], portOffset: (cardId: string, handle: string) => number | undefined): number {
  const board = getBoard(boardId);
  if (!board || board.document.cards.length === 0) {
    return 0;
  }
  const rects = displayedRects(board);
  const ordered = [...board.document.cards].sort((a, b) => {
    const entrance = Number("block" in a.ref) - Number("block" in b.ref);
    const ra = rects.get(a.id);
    const rb = rects.get(b.id);
    return entrance || (ra?.x ?? 0) - (rb?.x ?? 0) || (ra?.y ?? 0) - (rb?.y ?? 0) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  });
  const all = [...rects.values()];
  const origin = { x: Math.min(...all.map((rect) => rect.x)), y: Math.min(...all.map((rect) => rect.y)) };
  const positions = arrange(
    ordered.map((card) => {
      const rect = rects.get(card.id);
      return { id: card.id, width: rect?.width ?? 520, height: rect?.height ?? 320 };
    }),
    arrows.map((arrow) => ({
      source: arrow.source,
      target: arrow.target,
      port: portOffset(arrow.source, arrow.sourceHandle) ?? HEADER_PORT,
      order: arrow.order,
    })),
    origin,
  );
  const moves: Record<string, [number, number]> = {};
  let moved = 0;
  for (const card of board.document.cards) {
    const at = positions.get(card.id);
    if (at && (at.x !== card.at[0] || at.y !== card.at[1])) {
      moves[card.id] = [at.x, at.y];
      moved += 1;
    }
  }
  if (moved > 0) {
    editBoard(boardId, (document) => {
      moveCards(document, moves);
    });
  }
  return moved;
}
