/**
 * The board API the rest of the window uses: open a definition or an entrance on the active board,
 * and read which keys are on it (for the `●` marks). Opening onto a window with no board open
 * creates a draft first (architecture §10.5: every board saves itself from its first card).
 */

import { useMemo } from "react";
import { showToast } from "../../components/Toast";
import type { CardView } from "../../ipc/generated";
import { queryClient } from "../../ipc/client";
import { toUnfoldError } from "../../ipc/unfold";
import { cardQuery } from "../../canvas/cardData";
import { CARD_MIN_WIDTH, FOLLOW_GAP, STACK_GAP, fallbackSize, newCardPlacement, reflow, type Rect } from "../../canvas/layout";
import {
  addCard,
  cardKey,
  findCardByKey,
  keyDisplayName,
  removeCard,
  removeGroup,
  setCardDescription,
  setCardsFolded,
  setLabel,
  updateGroup,
  type GroupChange,
} from "./document";
import { openBoardTab } from "../tabs";
import { editBoard, getBoard, newBoard, pushFocusMove, requestFocus, selectCard, setPendingPlacement, useBoards } from "./store";
import type { OpenBoard, TrailItem } from "./types";

let creating: Promise<string> | null = null;

/** The board new cards go to, creating a draft named `title` when none is open. */
export async function ensureActiveBoard(title: string): Promise<string> {
  const { activeBoardId, order } = useBoards.getState();
  const existing = activeBoardId ?? order.at(-1) ?? null;
  if (existing !== null) {
    // Opening from a source tab (a call link, ⊕) or a sidebar view while a source tab shows: the
    // card goes to this board, so its tab comes forward to show it.
    openBoardTab(existing);
    return existing;
  }
  creating ??= newBoard(title).finally(() => {
    creating = null;
  });
  return creating;
}

/** Every card's rectangle as the canvas shows it (saved positions reflowed by measured sizes). */
export function displayedRects(board: OpenBoard): Map<string, Rect> {
  const sizeOf = (id: string) => {
    const measured = board.view.sizes[id];
    if (measured) {
      return measured;
    }
    const card = board.document.cards.find((item) => item.id === id);
    return fallbackSize(card !== undefined && "entrance" in card.ref);
  };
  const positions = reflow(board.document.cards, sizeOf);
  const rects = new Map<string, Rect>();
  for (const card of board.document.cards) {
    const at = positions.get(card.id) ?? { x: card.at[0], y: card.at[1] };
    rects.set(card.id, { ...at, ...sizeOf(card.id) });
  }
  return rects;
}

/** Brings an existing card into view, unfolding it if it was folded to its title. */
export function focusCard(boardId: string, cardId: string): void {
  const board = getBoard(boardId);
  const card = board?.document.cards.find((item) => item.id === cardId);
  if (!board || !card) {
    return;
  }
  if (card.folded === true) {
    // Following into a folded card opens it; the reader never folds anything by itself.
    editBoard(boardId, (document) => {
      setCardsFolded(document, [cardId], false);
    });
  }
  selectCard(boardId, cardId);
  requestFocus(boardId, [cardId], "center");
}

function reportFailure(error: unknown, what: string): void {
  showToast({ message: `Couldn't open ${what}: ${toUnfoldError(error).message}`, tone: "danger" });
}

async function openBlockCardAsync(key: string): Promise<void> {
  const label = keyDisplayName(key);
  const boardId = await ensureActiveBoard(label);
  const board = getBoard(boardId);
  if (!board) {
    return;
  }
  const trail: TrailItem[] = [];
  const existing = findCardByKey(board.document, key);
  if (existing) {
    pushFocusMove(boardId, [{ cardId: existing.id, label }]);
    focusCard(boardId, existing.id);
    return;
  }
  const at = newCardPlacement([...displayedRects(board).values()]);
  const cardId = editBoard(boardId, (document) => addCard(document, { ref: { block: key }, at }));
  if (cardId === undefined) {
    return;
  }
  trail.push({ cardId, label });
  pushFocusMove(boardId, trail);
  selectCard(boardId, cardId);
  requestFocus(boardId, [cardId], "center");
}

/** Opens a definition (or any indexed block) as a full card on the active board, or focuses it. */
export function openBlockCard(key: string): void {
  openBlockCardAsync(key).catch((error: unknown) => {
    reportFailure(error, keyDisplayName(key));
  });
}

/** The entrance's title as the trail and a new board show it: its label, or `POST /orders`. */
export function entranceTitle(card: Extract<CardView, { type: "entrance" }>): string {
  const { header } = card;
  if (header.label !== null && header.label.length > 0) {
    return header.label;
  }
  if (header.method !== null && header.trigger !== null) {
    return `${header.method} ${header.trigger}`;
  }
  return header.trigger ?? header.title;
}

async function openEntranceAsync(entranceKey: string): Promise<void> {
  // Read through the cache: the entrance card then draws from it at once. Its title names a new draft.
  const view = await queryClient.query(cardQuery(entranceKey));
  if (view.type !== "entrance") {
    throw new Error("that key is not an entrance");
  }
  const title = entranceTitle(view);
  const boardId = await ensureActiveBoard(title);
  const board = getBoard(boardId);
  if (!board) {
    return;
  }
  const existing = findCardByKey(board.document, entranceKey);
  if (existing) {
    pushFocusMove(boardId, [{ cardId: existing.id, label: title }]);
    focusCard(boardId, existing.id);
    return;
  }
  const rects = displayedRects(board);
  const [x, y] = newCardPlacement([...rects.values()]);
  const onBoard = new Set(board.document.cards.map(cardKey));
  // Every resolved handler opens as a full card, in registration order, once each.
  const handlers: string[] = [];
  for (const target of view.targets) {
    if (target.resolution === "resolved" && target.target !== null && !onBoard.has(target.target) && !handlers.includes(target.target)) {
      handlers.push(target.target);
    }
  }
  // Handlers start in a column to the right, stacked; once the entrance card is measured, the canvas
  // moves each level with its handler row (`PendingPlacement`).
  const column = x + CARD_MIN_WIDTH + FOLLOW_GAP;
  const handlerHeight = fallbackSize(false).height + STACK_GAP;
  const opened = editBoard(boardId, (document) => {
    const entranceId = addCard(document, { ref: { entrance: entranceKey }, at: [x, y] });
    const ids = handlers.map((handler, index) => ({ cardId: addCard(document, { ref: { block: handler }, at: [column, y + index * handlerHeight] }), key: handler }));
    return { entranceId, handlers: ids };
  });
  if (!opened) {
    return;
  }
  pushFocusMove(boardId, [{ cardId: opened.entranceId, label: title }]);
  selectCard(boardId, opened.entranceId);
  if (opened.handlers.length > 0) {
    setPendingPlacement(boardId, opened);
  } else {
    requestFocus(boardId, [opened.entranceId], "fit");
  }
}

/**
 * Opens an entrance card and a full card for each resolved handler on the active board (creating a
 * draft when none is open), or focuses the entrance card if it is already there.
 */
export function openEntrance(entranceKey: string): void {
  openEntranceAsync(entranceKey).catch((error: unknown) => {
    reportFailure(error, keyDisplayName(entranceKey));
  });
}

/** The active board's id: the board tab that was active last, or null when no board is open. */
export function useActiveBoardId(): string | null {
  return useBoards((state) => state.activeBoardId);
}

const NO_KEYS: ReadonlySet<string> = new Set();

/** The structural keys on the active board (blocks and entrances), for the `●` marks. */
export function useActiveBoardKeys(): ReadonlySet<string> {
  const cards = useBoards((state) => (state.activeBoardId !== null ? state.boards[state.activeBoardId]?.document.cards : undefined));
  return useMemo(() => (cards ? new Set(cards.map(cardKey)) : NO_KEYS), [cards]);
}

// ---------------------------------------------------------------------------------------------
// Authoring (the inspector and board menus): each is one undo step and autosaves. Typing in one
// field merges into one step. `author` is `person:<username>` (from `app_info`).

/** Sets or clears (blank text) a card's description; the next save signs it. */
export function describeCard(boardId: string, cardId: string, text: string, author: string): void {
  editBoard(boardId, (document) => {
    setCardDescription(document, cardId, text, author);
  }, { coalesce: `describe:${cardId}` });
}

/** Sets or clears (blank text) the Flow label of a block. */
export function labelBlock(boardId: string, key: string, text: string, author: string): void {
  editBoard(boardId, (document) => {
    setLabel(document, key, text, author);
  }, { coalesce: `label:${key}` });
}

/** Renames a group, sets its description, or collapses it. */
export function editGroup(boardId: string, groupId: string, change: GroupChange): void {
  editBoard(boardId, (document) => {
    updateGroup(document, groupId, change);
  }, change.collapsed === undefined ? { coalesce: `group:${groupId}` } : {});
}

export function ungroup(boardId: string, groupId: string): void {
  editBoard(boardId, (document) => {
    removeGroup(document, groupId);
  });
}

export function removeCardFromBoard(boardId: string, cardId: string): void {
  editBoard(boardId, (document) => {
    removeCard(document, cardId);
  });
}

/** Folds (or unfolds) every card to its title: Fold all and Unfold all. */
export function foldAllCards(boardId: string, folded: boolean): void {
  editBoard(boardId, (document) => {
    setCardsFolded(document, document.cards.map((card) => card.id), folded);
  });
}

/** A board's zoom (updated when the camera stops), for the status bar. */
export function useBoardZoom(boardId: string): number {
  return useBoards((state) => state.boards[boardId]?.view.camera?.zoom ?? 1);
}

/** Open board ids in the order they opened (for the editor's tabs and the window state). */
export function useOpenBoardIds(): readonly string[] {
  return useBoards((state) => state.order);
}

export {
  adoptBoardState,
  canRedo,
  canUndo,
  forgetBoard,
  reloadBoard,
  updateView,
  useBoards,
  adoptSession,
  closeBoard,
  editBoard,
  exportSession,
  flushAllBoards,
  flushBoard,
  newBoard,
  openBoard,
  redo,
  resolveConflict,
  saveBoard,
  setActiveBoard,
  subscribeBoardOpened,
  undo,
} from "./store";
