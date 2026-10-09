/**
 * What screens do to a page's cards (UI plan §3.4, §3.7): add a definition or an entrance (from any
 * connected project), bring a card into view, author descriptions, labels, groups, and links, and read
 * which project-qualified keys a page shows (for the on-page marks). Each change is one undo step and
 * autosaves.
 *
 * A card's project is its `ref.project` resolved against the current workspace (D-3); a card from the
 * board's own project has no `ref.project`.
 */

import { useMemo } from "react";
import { queryClient } from "../../ipc/client";
import type { Card, CardRef, CardView, Key, ProjectId, ProjectName } from "../../ipc/generated";
import { cardQuery } from "../../canvas/cardData";
import { CARD_MIN_WIDTH, FOLLOW_GAP, STACK_GAP, fallbackSize, newCardPlacement, reflow, type Rect } from "../../canvas/layout";
import { showToast } from "../../components/Toast";
import { activateBoard } from "../tabStore";
import { getProjectIndex, refProjectFor, resolveRefProject, useProjectIndex } from "../workspace";
import {
  addCard,
  addLink as addLinkTo,
  cardKey,
  findCard,
  findCardByRef,
  findPage,
  keyDisplayName,
  removeCard as removeCardFrom,
  removeGroup,
  removeLink as removeLinkFrom,
  setCardDescription,
  setCardsFolded,
  setLabel,
  setLinkLabel as setLinkLabelOf,
  updateGroup,
  type GroupChange,
} from "./document";
import { cardRefKey, boardKey } from "./keys";
import { newBoard } from "./open";
import { editPage, getOpenBoard, pushFocusMove, requestCardFocus, select, setActivePage, setPendingPlacement, useBoards } from "./store";
import type { BoardRef, OpenBoard, PageId, PageTarget } from "./types";

/** A card ref for `key`, naming `project` when the card isn't from the board's own project. */
function refOf(kind: "block" | "entrance", key: Key, project: ProjectName | undefined): CardRef {
  const named = project !== undefined ? { project } : {};
  return kind === "block" ? { block: key, ...named } : { entrance: key, ...named };
}

/** Every card's rectangle on a page as the canvas shows it (saved positions reflowed by measured sizes). */
export function displayedRects(board: OpenBoard, pageId: PageId): Map<string, Rect> {
  const page = findPage(board.document, pageId);
  const sizes = board.pages[pageId]?.sizes ?? {};
  const rects = new Map<string, Rect>();
  if (!page) {
    return rects;
  }
  const sizeOf = (id: string) => {
    const measured = sizes[id];
    if (measured) {
      return measured;
    }
    const card = findCard(page, id);
    return fallbackSize(card !== undefined && "entrance" in card.ref);
  };
  const positions = reflow(page.cards, sizeOf);
  for (const card of page.cards) {
    const at = positions.get(card.id) ?? { x: card.at[0], y: card.at[1] };
    rects.set(card.id, { ...at, ...sizeOf(card.id) });
  }
  return rects;
}

/** Shows a page: its board's tab and the page itself. */
function showPage(target: PageTarget): void {
  activateBoard(target.board);
  setActivePage(target.board, target.page);
}

/** Brings an existing card into view and selects it, unfolding it if it was folded to its title. */
export function focusCard(target: PageTarget, cardId: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const card = page ? findCard(page, cardId) : undefined;
  if (!card) {
    return;
  }
  if (card.folded === true) {
    // Following into a folded card opens it; the reader never folds anything by itself.
    editPage(target, (draft) => {
      setCardsFolded(draft, [cardId], false);
    });
  }
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
}

/** The `ref.project` a card from `project` gets on the target's board, or undefined after a refusal toast. */
function refProjectOn(target: PageTarget, project: ProjectId): { readonly project?: ProjectName } | undefined {
  const result = refProjectFor(getProjectIndex(), target.board.project, project);
  if (!result.ok) {
    showToast({
      title: "This card can't be added",
      body: "Its project has the same name as this board's project. Rename one of the projects in its settings.",
      tone: "danger",
    });
    return undefined;
  }
  return result;
}

/**
 * Adds a definition (or any indexed block) of `project` as a full card on the page, or focuses the card
 * that already shows it. Shows the page. Returns the card's id (undefined when it can't be added, V-62).
 */
export function addBlock(target: PageTarget, project: ProjectId, key: Key): string | undefined {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const named = board && page ? refProjectOn(target, project) : undefined;
  if (!board || !page || !named) {
    return undefined;
  }
  showPage(target);
  const label = keyDisplayName(key);
  const existing = findCardByRef(page, key, named.project);
  if (existing) {
    pushFocusMove(target, [{ cardId: existing.id, label }]);
    focusCard(target, existing.id);
    return existing.id;
  }
  const at = newCardPlacement([...displayedRects(board, target.page).values()]);
  const cardId = editPage(target, (draft) => addCard(draft, { ref: refOf("block", key, named.project), at }));
  if (cardId === undefined) {
    return undefined;
  }
  pushFocusMove(target, [{ cardId, label }]);
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
  return cardId;
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

/** An entrance's card model, read through the cache (its card then draws from it at once). */
async function entranceModel(project: ProjectId, entranceKey: Key): Promise<Extract<CardView, { type: "entrance" }>> {
  const view = await queryClient.query(cardQuery(project, entranceKey));
  if (view.type !== "entrance") {
    throw new Error("That key is not an entrance.");
  }
  return view;
}

/**
 * Opens an entrance card and a full card for each resolved handler on the page (or focuses the
 * entrance card if it is already there). Handlers start in a column to its right; once the entrance
 * card is drawn, the canvas moves each level with its handler row (`PendingPlacement`).
 */
export async function openEntrance(target: PageTarget, project: ProjectId, entranceKey: Key): Promise<void> {
  const view = await entranceModel(project, entranceKey);
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const named = board && page ? refProjectOn(target, project) : undefined;
  if (!board || !page || !named) {
    return;
  }
  showPage(target);
  const title = entranceTitle(view);
  const existing = page.cards.find((card) => "entrance" in card.ref && card.ref.entrance === entranceKey && card.ref.project === named.project);
  if (existing) {
    pushFocusMove(target, [{ cardId: existing.id, label: title }]);
    focusCard(target, existing.id);
    return;
  }
  const [x, y] = newCardPlacement([...displayedRects(board, target.page).values()]);
  const onPage = new Set(page.cards.filter((card) => card.ref.project === named.project).map(cardKey));
  // Every resolved handler opens as a full card, in registration order, once each.
  const handlers: Key[] = [];
  for (const handler of view.targets) {
    const key = handler.resolution === "resolved" ? handler.target : null;
    if (key !== null && !onPage.has(key) && !handlers.includes(key)) {
      handlers.push(key);
    }
  }
  const column = x + CARD_MIN_WIDTH + FOLLOW_GAP;
  const handlerHeight = fallbackSize(false).height + STACK_GAP;
  const opened = editPage(target, (draft) => {
    const entranceId = addCard(draft, { ref: refOf("entrance", entranceKey, named.project), at: [x, y] });
    const ids = handlers.map((key, index) => ({
      cardId: addCard(draft, { ref: refOf("block", key, named.project), at: [column, y + index * handlerHeight] }),
      key,
    }));
    return { entranceId, handlers: ids };
  });
  if (!opened) {
    return;
  }
  pushFocusMove(target, [{ cardId: opened.entranceId, label: title }]);
  select(target, { kind: "card", cardId: opened.entranceId, key: null });
  if (opened.handlers.length > 0) {
    setPendingPlacement(target, opened);
  } else {
    requestCardFocus(target, [opened.entranceId], "fit");
  }
}

/** Traces an entrance: a new draft in its project, titled after it, with the entrance and its handlers. */
export async function traceEntrance(project: ProjectId, entranceKey: Key): Promise<BoardRef> {
  const view = await entranceModel(project, entranceKey);
  const ref = await newBoard(project, { title: entranceTitle(view) });
  const page = getOpenBoard(ref)?.activePage;
  if (page !== undefined) {
    await openEntrance({ board: ref, page }, project, entranceKey);
  }
  return ref;
}

const NO_KEYS: ReadonlySet<string> = new Set();

/** The project-qualified keys (`cardRefKey`) of the cards on a page that resolve to a connected project. */
export function usePageKeys(target: PageTarget | null): ReadonlySet<string> {
  const key = target ? boardKey(target.board) : null;
  const pageId = target?.page;
  const cards = useBoards((state) => (key !== null ? state.boards[key]?.document.pages.find((page) => page.id === pageId)?.cards : undefined));
  const index = useProjectIndex();
  const boardProject = target?.board.project;
  return useMemo(() => {
    if (!cards || boardProject === undefined) {
      return NO_KEYS;
    }
    const keys = new Set<string>();
    for (const card of cards) {
      const project = resolveRefProject(index, boardProject, card.ref.project);
      if (project !== null) {
        keys.add(cardRefKey(project, cardKey(card)));
      }
    }
    return keys;
  }, [cards, index, boardProject]);
}

/** The project a card on a board of `ref` reads from, kept current; null when it isn't connected (a placeholder). */
export function useResolvedCardProject(ref: BoardRef, card: Card): ProjectId | null {
  return resolveRefProject(useProjectIndex(), ref.project, card.ref.project);
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

/** Sets or clears (blank text) the Flow label of a block of `project` on the page. */
export function labelNode(target: PageTarget, project: ProjectId, key: Key, text: string, author: string): void {
  const result = refProjectFor(getProjectIndex(), target.board.project, project);
  if (!result.ok) {
    return;
  }
  editPage(
    target,
    (page) => {
      setLabel(page, result.project, key, text, author);
    },
    { coalesce: `label:${project}:${key}` },
  );
}

/** Renames a group, sets its description, or collapses it. */
export function editGroup(target: PageTarget, groupId: string, change: GroupChange): void {
  editPage(
    target,
    (page) => {
      updateGroup(page, groupId, change);
    },
    change.collapsed === undefined ? { coalesce: `group:${groupId}` } : {},
  );
}

export function ungroup(target: PageTarget, groupId: string): void {
  editPage(target, (page) => {
    removeGroup(page, groupId);
  });
}

/** Removes a card from the page, with its groups and links. */
export function removeCard(target: PageTarget, cardId: string): void {
  editPage(target, (page) => {
    removeCardFrom(page, cardId);
  });
}

/** Folds (or unfolds) every card of the page to its title: Fold all and Unfold all. */
export function foldAll(target: PageTarget, folded: boolean): void {
  editPage(target, (page) => {
    setCardsFolded(
      page,
      page.cards.map((card) => card.id),
      folded,
    );
  });
}

/** Draws a link between two cards of the page (V-58). Returns its id, or undefined when it can't be drawn. */
export function addLink(target: PageTarget, from: string, to: string): string | undefined {
  return editPage(target, (page) => addLinkTo(page, from, to));
}

/** Sets or clears (blank text) a link's label. */
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
