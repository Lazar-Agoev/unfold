/**
 * What screens do to a page's cards: add a definition, a type, an app, or an entrance of the board's
 * project, bring a card, resource card, or note into view, author descriptions, names, combined steps,
 * connections, and notes, and read which project-qualified keys a page shows (for the on-page marks).
 * Each change is one undo step and autosaves. A page shows one project: its board's.
 */

import { useMemo } from "react";
import type { CardRef, ConnectionKind, Key, Page } from "../../ipc/generated";
import { fallbackItemSize, pageGeometry } from "../../canvas/frames/geometry";
import { newCardPlacement, type Rect } from "../../canvas/layout";
import { showBoard } from "../shownBoard";
import {
  addCard,
  cardKey,
  findCard,
  findCardByKey,
  findPage,
  removeCard as removeCardFrom,
  removeCombined,
  removeItems,
  removeLink as removeLinkFrom,
  setCardDescription,
  setName,
  updateCombined,
  type CombinedChange,
} from "./document";
import { setConnectionKind, setLinkDescription, setLinkLabel as setLinkLabelOf } from "./connections";
import { findFrame, frameChain, removeFrame as removeFrameFrom, updateFrame, type FrameChange } from "./frames";
import { findNote, setNoteText } from "./notes";
import { findResource, setResourceDescription } from "./resources";
import { cardRefKey, boardKey } from "./keys";
import { editPage, getOpenBoard, requestCardFocus, select, setActivePage, useBoards } from "./store";
import type { OpenBoard, PageId, PageTarget } from "./types";

/**
 * Every drawn item's rectangle on a page as the canvas shows it (saved positions reflowed by measured
 * sizes): its cards, frames' boxes, and collapsed frames' nodes (`pageGeometry`).
 */
export function displayedRects(board: OpenBoard, pageId: PageId): ReadonlyMap<string, Rect> {
  const page = findPage(board.document, pageId);
  const sizes = board.pages[pageId]?.sizes ?? {};
  return page ? pageGeometry(page, (id) => sizes[id] ?? fallbackItemSize(page, id)).rects : new Map();
}

/** Shows a page: its board and the page itself. */
function showPage(target: PageTarget): void {
  showBoard(target.board);
  setActivePage(target.board, target.page);
}

/** Brings an existing card into view and selects it, expanding the collapsed frames around it. */
export function focusCard(target: PageTarget, cardId: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const card = page ? findCard(page, cardId) : undefined;
  if (!page || !card) {
    return;
  }
  const around = frameChain(page, cardId).filter((frame) => frame.collapsed === true);
  if (around.length > 0) {
    editPage(target, (draft) => {
      for (const frame of around) {
        updateFrame(draft, frame.id, { collapsed: false });
      }
    });
  }
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
}

/**
 * Adds a card for `ref` (a module, definition, or type; or an app or entrance) of the board's project to
 * the page, or focuses the card that already shows it. `name` is what the card is called, which its id
 * is made from. Shows the page. Returns the card's id (undefined when it can't be added).
 */
export function addRef(target: PageTarget, ref: CardRef, name: string): string | undefined {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  if (!board || !page) {
    return undefined;
  }
  showPage(target);
  const existing = findCardByKey(page, cardKey({ ref }));
  if (existing) {
    focusCard(target, existing.id);
    return existing.id;
  }
  const at = newCardPlacement([...displayedRects(board, target.page).values()]);
  const cardId = editPage(target, (draft) => addCard(draft, { ref, name, at }));
  if (cardId === undefined) {
    return undefined;
  }
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
  return cardId;
}

/** Adds a module, definition, or type of the board's project (`addRef`). */
export function addBlock(target: PageTarget, key: Key, name: string): string | undefined {
  return addRef(target, { block: key }, name);
}

const NO_KEYS: ReadonlySet<string> = new Set();

/** The project-qualified keys (`cardRefKey`) of the cards on a page. */
export function usePageKeys(target: PageTarget | null): ReadonlySet<string> {
  const key = target ? boardKey(target.board) : null;
  const pageId = target?.page;
  const cards = useBoards((state) => (key !== null ? state.boards[key]?.document.pages.find((page) => page.id === pageId)?.cards : undefined));
  const boardProject = target?.board.project;
  return useMemo(
    () => (cards && boardProject !== undefined ? new Set(cards.map((card) => cardRefKey(boardProject, cardKey(card)))) : NO_KEYS),
    [cards, boardProject],
  );
}

// ---------------------------------------------------------------------------------------------
// Authoring (Selected, card menus): each is one undo step; typing in one field merges into one step.
// `author` is `person:<username>` (`useAuthor`).

/** Sets or clears (blank text) a card's description; the next save signs it. */
export function describeCard(target: PageTarget, cardId: string, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setCardDescription(page, cardId, text, author);
    },
    { coalesce: `describe:${cardId}` },
  );
}

/** Sets or clears (blank text) the page's name for a block. */
export function nameBlock(target: PageTarget, key: Key, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setName(page, key, text, author);
    },
    { coalesce: `name:${key}` },
  );
}

/** Renames a combined step, sets its description, or collapses or opens it. */
export function editCombined(target: PageTarget, combinedId: string, change: CombinedChange): void {
  editPage(
    target,
    (page) => {
      updateCombined(page, combinedId, change);
    },
    change.collapsed === undefined ? { coalesce: `combined:${combinedId}` } : {},
  );
}

/** Removes a combined step: its blocks are drawn one by one again. */
export function uncombine(target: PageTarget, combinedId: string): void {
  editPage(target, (page) => {
    removeCombined(page, combinedId);
  });
}

/** Removes a card from the page, with its connections (and a frame it leaves empty). */
export function removeCard(target: PageTarget, cardId: string): void {
  editPage(target, (page) => {
    removeCardFrom(page, cardId);
  });
}

/** Expands the collapsed frames around item `id` of the page (one undo step), so the item is drawn. */
function expandAround(target: PageTarget, page: Page, id: string): void {
  const around = frameChain(page, id).filter((frame) => frame.collapsed === true);
  if (around.length > 0) {
    editPage(target, (draft) => {
      for (const frame of around) {
        updateFrame(draft, frame.id, { collapsed: false });
      }
    });
  }
}

/** Selects a frame and brings it into view, expanding the collapsed frames around it. */
export function focusFrame(target: PageTarget, frameId: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  if (!page || !findFrame(page, frameId)) {
    return;
  }
  expandAround(target, page, frameId);
  select(target, { kind: "frame", frameId });
  requestCardFocus(target, [frameId], "fit");
}

/** Renames, describes, or collapses a frame; typing in one field merges into one undo step. */
export function editFrame(target: PageTarget, frameId: string, change: FrameChange): void {
  editPage(
    target,
    (page) => {
      updateFrame(page, frameId, change);
    },
    change.collapsed === undefined ? { coalesce: `frame:${frameId}` } : {},
  );
}

/** Removes a frame from the page; what it holds stays, in the frame that held it. */
export function removeFrame(target: PageTarget, frameId: string): void {
  editPage(target, (page) => {
    removeFrameFrom(page, frameId);
  });
}

/** Sets or clears (blank text) a connection's label; typing merges into one undo step. */
export function setLinkLabel(target: PageTarget, linkId: string, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setLinkLabelOf(page, linkId, text, author);
    },
    { coalesce: `link:${linkId}` },
  );
}

export function removeLink(target: PageTarget, linkId: string): void {
  editPage(target, (page) => {
    removeLinkFrom(page, linkId);
  });
}

/** Gives a connection another kind, which makes you the one who drew it. */
export function setLinkKind(target: PageTarget, linkId: string, kind: ConnectionKind, author: string): void {
  editPage(target, (page) => {
    setConnectionKind(page, linkId, kind, author);
  });
}

/** Sets or clears (blank text) a connection's description; typing merges into one undo step. */
export function describeLink(target: PageTarget, linkId: string, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setLinkDescription(page, linkId, text, author);
    },
    { coalesce: `link-description:${linkId}` },
  );
}

/** Sets or clears (blank text) a resource card's description; typing merges into one undo step. */
export function describeResource(target: PageTarget, resourceId: string, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setResourceDescription(page, resourceId, text, author);
    },
    { coalesce: `resource:${resourceId}` },
  );
}

/** Rewrites a note (blank text keeps the old); typing merges into one undo step. */
export function editNote(target: PageTarget, noteId: string, text: string, author: string): void {
  editPage(
    target,
    (page) => {
      setNoteText(page, noteId, text, author);
    },
    { coalesce: `note:${noteId}` },
  );
}

/** Removes a resource card or a note from the page, with the links to it; a note attached to it stays where it was. */
export function removeItem(target: PageTarget, id: string): void {
  editPage(target, (page) => {
    removeItems(page, [id]);
  });
}

/** Selects a resource card or a note and brings it into view, expanding the collapsed frames around it. */
export function focusItem(target: PageTarget, id: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const resource = page ? findResource(page, id) : undefined;
  if (!page || (!resource && !findNote(page, id))) {
    return;
  }
  expandAround(target, page, id);
  select(target, resource ? { kind: "resource", resourceId: id } : { kind: "note", noteId: id });
  requestCardFocus(target, [id], "center");
}
