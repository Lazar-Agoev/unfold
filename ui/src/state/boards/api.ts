/**
 * What screens do to a page's cards: add a definition or an entrance (from any
 * connected project), bring a card, resource card, or note into view, author descriptions, labels,
 * groups, links and connections, and notes, and read which project-qualified keys a page shows (for the
 * on-page marks). Each change is one undo step and
 * autosaves.
 *
 * A card's project is its `ref.project` resolved against the current workspace (U-40); a card from the
 * board's own project has no `ref.project`.
 */

import { useMemo } from "react";
import { queryClient } from "../../ipc/client";
import type { Card, CardRef, CardView, Key, LinkKind, Page, ProjectId, ProjectName, View } from "../../ipc/generated";
import { cardQuery } from "../../canvas/cardData";
import { fallbackItemSize, pageGeometry } from "../../canvas/frames/geometry";
import { handlerPlacement, newCardPlacement, type Rect } from "../../canvas/layout";
import { measureText } from "../../canvas/measure";
import { entranceCardText, entranceTitle } from "../../canvas/entranceText";
import { entranceHandlerRows, estimatedCardSize } from "../../canvas/sizing";
import { cardBody } from "../../code/typeKinds";
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
  removeItems,
  removeLink as removeLinkFrom,
  setCardDescription,
  setCardsFolded,
  setLabel,
  setLinkLabel as setLinkLabelOf,
  updateGroup,
  type GroupChange,
} from "./document";
import { setConnectionKind, setLinkDescription } from "./connections";
import { findFrame, frameChain, removeFrame as removeFrameFrom, updateFrame, type FrameChange } from "./frames";
import { findNote, setNoteText } from "./notes";
import { findResource, setResourceDescription } from "./resources";
import { cardRefKey, boardKey } from "./keys";
import { newBoard } from "./open";
import { editPage, getOpenBoard, getPageView, pushFocusMove, requestCardFocus, select, setActivePage, useBoards } from "./store";
import type { BoardRef, OpenBoard, PageId, PageTarget } from "./types";

/** A card ref for `key`, naming `project` when the card isn't from the board's own project. */
function refOf(kind: "block" | "entrance", key: Key, project: ProjectName | undefined): CardRef {
  const named = project !== undefined ? { project } : {};
  return kind === "block" ? { block: key, ...named } : { entrance: key, ...named };
}

/**
 * Every drawn item's rectangle on a page as the canvas shows it (saved positions reflowed by measured
 * sizes): its cards, frames' boxes, and collapsed frames' nodes (`pageGeometry`).
 */
export function displayedRects(board: OpenBoard, pageId: PageId): ReadonlyMap<string, Rect> {
  const page = findPage(board.document, pageId);
  const sizes = board.pages[pageId]?.sizes ?? {};
  return page ? pageGeometry(page, (id) => sizes[id] ?? fallbackItemSize(page, id)).rects : new Map();
}

/** Shows a page: its board's tab and the page itself. */
function showPage(target: PageTarget): void {
  activateBoard(target.board);
  setActivePage(target.board, target.page);
}

/**
 * Brings an existing card into view and selects it, unfolding it if it was folded to its title and
 * expanding the collapsed frames around it.
 */
export function focusCard(target: PageTarget, cardId: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const card = page ? findCard(page, cardId) : undefined;
  if (!page || !card) {
    return;
  }
  const around = frameChain(page, cardId).filter((frame) => frame.collapsed === true);
  if (card.folded === true || around.length > 0) {
    // Following into a folded card opens it; the reader never folds anything by itself.
    editPage(target, (draft) => {
      setCardsFolded(draft, [cardId], false);
      for (const frame of around) {
        updateFrame(draft, frame.id, { collapsed: false });
      }
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
 * Adds a definition (or any indexed block) of `project` as a full card on the page, in its own `view`
 * when given, or focuses the card that already shows it. Shows the page. Returns the card's id
 * (undefined when it can't be added).
 */
export function addBlock(target: PageTarget, project: ProjectId, key: Key, view?: View): string | undefined {
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
  const cardId = editPage(target, (draft) => addCard(draft, { ref: refOf("block", key, named.project), at, ...(view ? { view } : {}) }));
  if (cardId === undefined) {
    return undefined;
  }
  pushFocusMove(target, [{ cardId, label }]);
  select(target, { kind: "card", cardId, key: null });
  requestCardFocus(target, [cardId], "center");
  return cardId;
}

export { entranceCardText, entranceTitle };

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
 * entrance card if it is already there), in one step: the handlers stand in a column to its right,
 * each level with its handler row, placed from the cards' models before they are drawn, so nothing
 * moves them afterwards (measuring them only fits the camera).
 */
export async function openEntrance(target: PageTarget, project: ProjectId, entranceKey: Key): Promise<void> {
  const view = await entranceModel(project, entranceKey);
  // Every resolved handler opens as a full card, in registration order, once each. Their models size
  // them, and the cards draw from the same cache at once.
  const resolved = [...new Set(view.targets.flatMap((handler) => (handler.resolution === "resolved" && handler.target !== null ? [handler.target] : [])))];
  const models = await Promise.all(resolved.map((key) => queryClient.query(cardQuery(project, key)).catch(() => undefined)));
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
  const handlers = resolved.flatMap((key, index) => (onPage.has(key) ? [] : [{ key, model: models[index] }]));
  const foreign = named.project !== undefined;
  // The cards show the page's lens, else its default view: an entrance's handler rows, or a Diagram node's edge.
  const shown = getPageView(target).lens ?? page.defaultView;
  const { width } = estimatedCardSize({ key: entranceKey, model: view, view: shown, monogram: foreign }, measureText);
  const rows = cardBody(entranceKey, shown) === "handlers" ? entranceHandlerRows(entranceCardText(view), foreign, width, measureText) : [];
  const positions = handlerPlacement(
    { x, y, width },
    handlers.map(({ key }) => rows[view.targets.findIndex((handler) => handler.target === key)]),
    handlers.map(({ key, model }) => estimatedCardSize({ key, model, view: shown, monogram: foreign }, measureText)),
  );
  const opened = editPage(target, (draft) => [
    addCard(draft, { ref: refOf("entrance", entranceKey, named.project), at: [x, y] }),
    ...handlers.map(({ key }, index) => addCard(draft, { ref: refOf("block", key, named.project), at: positions[index] ?? [x, y] })),
  ]);
  const entranceId = opened?.[0];
  if (!opened || entranceId === undefined) {
    return;
  }
  pushFocusMove(target, [{ cardId: entranceId, label: title }]);
  select(target, { kind: "card", cardId: entranceId, key: null });
  requestCardFocus(target, opened, "fit");
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

/** Removes a card from the page, with its groups and links (and a frame it leaves empty). */
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

/** Draws a link between two cards of the page. Returns its id, or undefined when it can't be drawn. */
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

/** Gives a link a kind (a drawn link becomes a connection), which makes you the one who drew it. */
export function setLinkKind(target: PageTarget, linkId: string, kind: LinkKind, author: string): void {
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
