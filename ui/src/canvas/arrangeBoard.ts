/** Arrange by call depth on a page: one undoable move of every card. */

import { displayedRects, editPage, findPage, getOpenBoard, moveCards, type PageTarget } from "../state/boards";
import type { Arrow } from "./arrows";
import { HEADER_PORT, arrange } from "./layout";

/**
 * Lays the page out in columns by call depth from its entrances (then the cards nothing calls), in a
 * stable order: entrances first, then left to right and top to bottom. `portOffset` gives a port's
 * measured offset below its card's top. Returns how many cards moved.
 */
export function arrangeBoard(target: PageTarget, arrows: readonly Arrow[], portOffset: (cardId: string, handle: string) => number | undefined): number {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  if (!board || !page || page.cards.length === 0) {
    return 0;
  }
  const rects = displayedRects(board, target.page);
  const ordered = [...page.cards].sort((a, b) => {
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
  for (const card of page.cards) {
    const at = positions.get(card.id);
    if (at && (at.x !== card.at[0] || at.y !== card.at[1])) {
      moves[card.id] = [at.x, at.y];
      moved += 1;
    }
  }
  if (moved > 0) {
    editPage(target, (draft) => {
      moveCards(draft, moves);
    });
  }
  return moved;
}
