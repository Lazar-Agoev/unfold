/** Arrange by call depth on a page: one undoable move of every card. */

import { displayedRects, editPage, findPage, getOpenBoard, moveCards, type PageTarget } from "../state/boards";
import type { Arrow } from "./arrows";
import { arrangedPositions } from "./layout";

/**
 * Lays the page out in columns by call depth (`arrangedPositions`) as one edit. `portOffset` gives an
 * arrow's measured port offset below its card's top. Returns how many cards moved.
 */
export function arrangeBoard(target: PageTarget, arrows: readonly Arrow[], portOffset: (arrow: Arrow) => number | undefined): number {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  if (!board || !page || page.cards.length === 0) {
    return 0;
  }
  const positions = arrangedPositions(page.cards, displayedRects(board, target.page), arrows, portOffset);
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
