/**
 * A board's pages: add, rename, reorder, delete. Each is one undo step of the board;
 * undoing it shows the page it was about.
 */

import {
  addPage as addPageTo,
  deletePage as deletePageOf,
  movePage as movePageOf,
  renamePage as renamePageOf,
} from "./document";
import { editBoard, getOpenBoard, setActivePage } from "./store";
import type { BoardRef, PageId, PageTarget } from "./types";

/** Adds an empty page after the active one ("Page N" unless titled) and shows it unless `activate` is false. */
export function addPage(ref: BoardRef, options: { readonly title?: string; readonly activate?: boolean } = {}): PageId | undefined {
  const board = getOpenBoard(ref);
  if (!board) {
    return undefined;
  }
  const after = board.activePage;
  const id = editBoard(ref, (document) => addPageTo(document, { after, ...(options.title !== undefined ? { title: options.title } : {}) }), {
    page: after,
  });
  if (id !== undefined && (options.activate ?? true)) {
    setActivePage(ref, id);
  }
  return id;
}

export function renamePage(target: PageTarget, title: string): void {
  editBoard(
    target.board,
    (document) => {
      renamePageOf(document, target.page, title);
    },
    { page: target.page, coalesce: `page-title:${target.page}` },
  );
}

export function movePage(target: PageTarget, delta: -1 | 1): void {
  editBoard(
    target.board,
    (document) => {
      movePageOf(document, target.page, delta);
    },
    { page: target.page },
  );
}

/** Deletes a page (never the only one); a deleted active page hands over to its neighbour. */
export function deletePage(target: PageTarget): void {
  const board = getOpenBoard(target.board);
  const pages = board?.document.pages ?? [];
  const index = pages.findIndex((page) => page.id === target.page);
  if (!board || index < 0 || pages.length < 2) {
    return;
  }
  if (board.activePage === target.page) {
    const neighbour = pages[index + 1] ?? pages[index - 1];
    if (neighbour) {
      setActivePage(target.board, neighbour.id);
    }
  }
  editBoard(
    target.board,
    (document) => {
      deletePageOf(document, target.page);
    },
    { page: target.page },
  );
}
