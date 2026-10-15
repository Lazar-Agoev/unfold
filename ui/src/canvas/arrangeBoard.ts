/** Arrange on a page, from the inside out (`arrangePage`): one undoable move of every card, resource card, and note. */

import { editPage, findPage, getOpenBoard, moveItems, type PageTarget } from "../state/boards";
import { arrangePage, lineNoteSpots } from "./arrangePage";
import type { Arrow } from "./arrows";
import { fallbackItemSize, pageGeometry } from "./frames/geometry";

/**
 * Lays the page out (`arrangePage`) as one edit, with the sizes the canvas measured (else estimated
 * for each card's view). `portOffset` gives a card's measured port (an arrow's, or a connection's step)
 * below its top. Returns how many cards and resource cards moved.
 */
export function arrangeBoard(target: PageTarget, arrows: readonly Arrow[], portOffset: (card: string, port: string) => number | undefined): number {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  if (!board || !page || (page.cards.length === 0 && (page.resources ?? []).length === 0)) {
    return 0;
  }
  const sizes = board.pages[target.page]?.sizes ?? {};
  const sizeOf = (id: string) => sizes[id] ?? fallbackItemSize(page, id);
  const positions = arrangePage({ page, rects: pageGeometry(page, sizeOf).rects, sizeOf, arrows, portOffset });
  const moves: Record<string, [number, number]> = {};
  for (const item of [...page.cards, ...(page.resources ?? [])]) {
    const at = positions.get(item.id);
    if (at && (at.x !== item.at[0] || at.y !== item.at[1])) {
      moves[item.id] = [at.x, at.y];
    }
  }
  const moved = Object.keys(moves).length;
  // The notes under them follow, at their new offsets, and the free notes go to the top right.
  let notes = false;
  for (const note of page.notes ?? []) {
    const at = positions.get(note.id);
    if (at) {
      moves[note.id] = [at.x, at.y];
      notes ||= note.attach === undefined && (at.x !== note.at[0] || at.y !== note.at[1]);
    }
  }
  if (moved > 0 || notes) {
    editPage(target, (draft) => {
      moveItems(draft, moves);
      // Then the notes on connections, beside their lines as they now run.
      moveItems(draft, lineNoteSpots(draft, sizeOf));
    });
  }
  return moved;
}
