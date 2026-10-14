/**
 * Pure edits of an `unfold-board` document. Each function is an Immer recipe over a
 * draft document or page, so the store can record one undo step per edit. Optional fields are deleted
 * rather than set to `undefined`, matching what the file holds (`description` and `label` only when
 * there is text).
 *
 * Cards, combined steps, connections, frames, resource cards, notes, and proposed steps share one id space
 * per page; page ids are unique within the board. Both match `/^[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$/`. Removing an item takes
 * what would dangle with it, as the engine does.
 */

import type { BoardDocument, Card, CardRef, Combined, Description, Key, NameEntry, Page, View } from "../../ipc/generated";
import type { PageId } from "./types";

/** The structural key a card shows. */
export function cardKey(card: Pick<Card, "ref">): Key {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

export function findPage(document: BoardDocument, pageId: PageId): Page | undefined {
  return document.pages.find((page) => page.id === pageId);
}

export function findCard(page: Page, cardId: string): Card | undefined {
  return page.cards.find((card) => card.id === cardId);
}

/** The first card of the page that shows `key`. */
export function findCardByKey(page: Page, key: Key): Card | undefined {
  return page.cards.find((card) => cardKey(card) === key);
}

/** `createOrder` → `create-order`; anything else non-alphanumeric becomes one dash. */
function slug(text: string, fallback: string): string {
  const value = text
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return value.length > 0 ? value.slice(0, 48).replace(/-+$/, "") : fallback;
}

/** `stem`, or `stem-2`, `stem-3`, … : the first not in `taken`. */
function firstFree(stem: string, taken: ReadonlySet<string>): string {
  if (!taken.has(stem)) {
    return stem;
  }
  for (let suffix = 2; ; suffix += 1) {
    const candidate = `${stem}-${suffix}`;
    if (!taken.has(candidate)) {
      return candidate;
    }
  }
}

/** The ids of the page's items: cards, combined steps, connections, frames, resources, notes, and proposed steps share one id space. */
function takenIds(page: Page): Set<string> {
  const items = [page.cards, page.combined, page.links, page.frames ?? [], page.resources ?? [], page.notes ?? [], page.proposals ?? []];
  return new Set(items.flatMap((list) => list.map((item) => item.id)));
}

/** An id unique among the page's items, derived from `base`. */
export function uniqueId(page: Page, base: string): string {
  return firstFree(slug(base, "card"), takenIds(page));
}

/** Whether `id` is a well-formed item id: letters, digits, `_`, `.`, and `-`, starting with a letter or digit, at most 100. */
export function isItemId(id: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$/.test(id);
}

/** Whether `id` is a well-formed item id that no item of the page has. */
export function isFreeId(page: Page, id: string): boolean {
  return isItemId(id) && !takenIds(page).has(id);
}

export interface NewCard {
  readonly ref: CardRef;
  /** What the card shows as its name (from the graph's records), which its id is made from. */
  readonly name: string;
  readonly at: readonly [number, number];
  readonly view?: View;
}

/** Adds a card and returns its id. Positions are rounded: the file holds finite numbers. */
export function addCard(page: Page, card: NewCard): string {
  const id = uniqueId(page, card.name);
  page.cards.push({
    id,
    ref: card.ref,
    at: [Math.round(card.at[0]), Math.round(card.at[1])],
    ...(card.view ? { view: card.view } : {}),
  });
  return id;
}

/**
 * What removing `ids` takes with it, the engine's removal cascade (`remove_items`): the connections that
 * end at a removed item, and a frame whose members are all removed (then its connections, and so on).
 * Names, combined steps, and proposed steps apply page-wide by key, so removing a card keeps them.
 * Includes `ids` themselves.
 */
export function removalCascade(page: Page, ids: readonly string[]): Set<string> {
  const removed = new Set(ids);
  for (let before = -1; before !== removed.size; ) {
    before = removed.size;
    for (const link of page.links) {
      if (removed.has(link.from) || removed.has(link.to)) {
        removed.add(link.id);
      }
    }
    for (const frame of page.frames ?? []) {
      if (frame.members.every((member) => removed.has(member))) {
        removed.add(frame.id);
      }
    }
  }
  return removed;
}

/** The most items a position walks through: a link's end frames, and their nested frames. */
const POSITION_DEPTH = 5;

/**
 * Where item `id` is as far as the file says (the engine's rule): a card's or resource card's
 * position, the top-left of a frame's cards, resource cards, and nested frames, or the midpoint of a
 * link's ends; undefined for anything else.
 */
function itemPosition(page: Page, id: string, depth = 0): [number, number] | undefined {
  if (depth > POSITION_DEPTH) {
    return undefined;
  }
  const placed = page.cards.find((card) => card.id === id) ?? (page.resources ?? []).find((resource) => resource.id === id);
  if (placed) {
    return [placed.at[0], placed.at[1]];
  }
  const frame = (page.frames ?? []).find((item) => item.id === id);
  if (frame) {
    const corners = frame.members.flatMap((member) => {
      const at = itemPosition(page, member, depth + 1);
      return at ? [at] : [];
    });
    return corners.length > 0 ? [Math.min(...corners.map(([x]) => x)), Math.min(...corners.map(([, y]) => y))] : undefined;
  }
  const link = page.links.find((item) => item.id === id);
  const from = link ? itemPosition(page, link.from, depth + 1) : undefined;
  const to = link ? itemPosition(page, link.to, depth + 1) : undefined;
  return from && to ? [(from[0] + to[0]) / 2, (from[1] + to[1]) / 2] : undefined;
}

/**
 * Removes items with everything that would dangle (`removalCascade`), and takes them out of the
 * frames that hold them. A note attached to a removed item stays where it was, unattached. Returns
 * the ids removed, in the order of the page's lists.
 */
export function removeItems(page: Page, ids: readonly string[]): string[] {
  const removed = removalCascade(page, ids);
  for (const note of page.notes ?? []) {
    if (note.attach !== undefined && removed.has(note.attach) && !removed.has(note.id)) {
      const [x, y] = itemPosition(page, note.attach) ?? [0, 0];
      note.at = [x + note.at[0], y + note.at[1]];
      delete note.attach;
    }
  }
  const gone: string[] = [];
  const kept = <T extends { readonly id: string }>(items: readonly T[]): T[] =>
    items.filter((item) => {
      if (removed.has(item.id)) {
        gone.push(item.id);
        return false;
      }
      return true;
    });
  page.cards = kept(page.cards);
  page.combined = kept(page.combined);
  page.links = kept(page.links);
  if (page.frames) {
    page.frames = kept(page.frames);
    for (const frame of page.frames) {
      frame.members = frame.members.filter((member) => !removed.has(member));
    }
  }
  if (page.resources) {
    page.resources = kept(page.resources);
  }
  if (page.notes) {
    page.notes = kept(page.notes);
  }
  if (page.proposals) {
    page.proposals = kept(page.proposals);
  }
  // The file leaves out empty lists of these.
  if (page.frames?.length === 0) {
    delete page.frames;
  }
  if (page.resources?.length === 0) {
    delete page.resources;
  }
  if (page.notes?.length === 0) {
    delete page.notes;
  }
  if (page.proposals?.length === 0) {
    delete page.proposals;
  }
  return gone;
}

/** Removes a card, the connections that end at it, and a frame it leaves empty. */
export function removeCard(page: Page, cardId: string): void {
  removeItems(page, [cardId]);
}

/**
 * Moves cards, resource cards, and notes; other ids are ignored. Positions are rounded. A note attached
 * to an item goes to the canvas position given: its offset is taken from where its item is once the
 * others moved (the engine's rule, `itemPosition`).
 */
export function moveItems(page: Page, positions: Readonly<Record<string, readonly [number, number]>>): void {
  const wanted = (id: string) => {
    const at = positions[id];
    return at && Number.isFinite(at[0]) && Number.isFinite(at[1]) ? at : undefined;
  };
  const attached = (page.notes ?? []).filter((note) => note.attach !== undefined);
  for (const item of [...page.cards, ...(page.resources ?? []), ...(page.notes ?? []).filter((note) => note.attach === undefined)]) {
    const at = wanted(item.id);
    if (at) {
      item.at = [Math.round(at[0]), Math.round(at[1])];
    }
  }
  for (const note of attached) {
    const at = wanted(note.id);
    const from = at && note.attach !== undefined ? itemPosition(page, note.attach) : undefined;
    if (at && from) {
      note.at = [Math.round(at[0] - from[0]), Math.round(at[1] - from[1])];
    }
  }
}

/** The view a card shows: its own, else its page's default. */
export function cardView(card: Pick<Card, "view">, defaultView: View): View {
  return card.view ?? defaultView;
}

/** Sets a card's view: compact, or expanded. */
export function setCardView(page: Page, cardId: string, view: View): void {
  const card = findCard(page, cardId);
  if (card) {
    card.view = view;
  }
}

/** The most calls one card opens in place, and how deep (the board format's limits). */
const MAX_OPEN_PATHS = 64;
export const MAX_OPEN_DEPTH = 3;

/**
 * The calls a card opens in place: paths of 1 to 3 call or callback keys, outermost first, each a block
 * of the flow drawn at that level.
 */
export function openPaths(card: Pick<Card, "open">): readonly (readonly Key[])[] {
  return card.open ?? [];
}

function samePath(a: readonly Key[], b: readonly Key[]): boolean {
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

/** Whether the card opens the call at `path` in place. */
export function isOpen(card: Pick<Card, "open">, path: readonly Key[]): boolean {
  return openPaths(card).some((item) => samePath(item, path));
}

/**
 * Opens the call at `path` in place, or closes it (with every call opened inside it). A path deeper
 * than three levels, or one more than the card holds, changes nothing. An empty list is deleted, as the
 * file omits it.
 */
export function toggleOpen(page: Page, cardId: string, path: readonly Key[]): void {
  const card = findCard(page, cardId);
  if (!card || path.length === 0 || path.length > MAX_OPEN_DEPTH) {
    return;
  }
  const paths = openPaths(card);
  let next: Key[][];
  if (paths.some((item) => samePath(item, path))) {
    next = paths.filter((item) => !samePath(item.slice(0, path.length), path)).map((item) => [...item]);
  } else if (paths.length < MAX_OPEN_PATHS) {
    next = [...paths.map((item) => [...item]), [...path]];
  } else {
    return;
  }
  if (next.length > 0) {
    card.open = next;
  } else {
    delete card.open;
  }
}

/** A new, unsigned description (the next save signs it from verified source). */
export function description(text: string, author: string): Description {
  return { text, author, signature: "" };
}

/** Sets or clears (blank text) a card's description. */
export function setCardDescription(page: Page, cardId: string, text: string, author: string): void {
  const card = findCard(page, cardId);
  if (!card) {
    return;
  }
  if (text.trim().length === 0) {
    delete card.description;
  } else if (card.description?.text !== text) {
    card.description = description(text, author);
  }
}

/** The page's names, by block key. */
export function nameTexts(names: readonly NameEntry[]): Record<Key, string> {
  return Object.fromEntries(names.map((name) => [name.key, name.text]));
}

/** Sets or clears (blank text) the page's name for block `key`. Names sort by key, as the file writes them. */
export function setName(page: Page, key: Key, text: string, author: string): void {
  const index = page.names.findIndex((name) => name.key === key);
  if (text.trim().length === 0) {
    if (index >= 0) {
      page.names.splice(index, 1);
    }
    return;
  }
  if (index >= 0 && page.names[index]?.text === text) {
    return;
  }
  const name: NameEntry = { key, ...description(text, author) };
  if (index >= 0) {
    page.names[index] = name;
    return;
  }
  const at = page.names.findIndex((existing) => key < existing.key);
  page.names.splice(at < 0 ? page.names.length : at, 0, name);
}

export function findCombined(page: Page, combinedId: string): Combined | undefined {
  return page.combined.find((item) => item.id === combinedId);
}

export interface CombinedChange {
  readonly title?: string;
  /** Who wrote a new title. */
  readonly author?: string;
  readonly collapsed?: boolean;
  /** Blank text clears the description. */
  readonly description?: { readonly text: string; readonly author: string };
}

/** Renames, describes, collapses, or opens a combined step. A new title makes its writer the author. */
export function updateCombined(page: Page, combinedId: string, change: CombinedChange): void {
  const combined = findCombined(page, combinedId);
  if (!combined) {
    return;
  }
  if (change.title !== undefined && change.title.trim().length > 0 && change.title !== combined.title) {
    combined.title = change.title;
    if (change.author !== undefined) {
      combined.author = change.author;
    }
  }
  if (change.collapsed !== undefined) {
    combined.collapsed = change.collapsed;
  }
  if (change.description) {
    if (change.description.text.trim().length === 0) {
      delete combined.description;
    } else if (combined.description?.text !== change.description.text) {
      combined.description = description(change.description.text, change.description.author);
    }
  }
}

/** Removes a combined step: its blocks are drawn one by one again. */
export function removeCombined(page: Page, combinedId: string): void {
  removeItems(page, [combinedId]);
}

/** Removes a connection; a note attached to it stays where it was. */
export function removeLink(page: Page, linkId: string): void {
  removeItems(page, [linkId]);
}

/** Sets the view of the page's cards that have none of their own. */
export function setDefaultView(page: Page, view: View): void {
  page.defaultView = view;
}

export function setTitle(document: BoardDocument, title: string): void {
  if (title.trim().length > 0) {
    document.title = title;
  }
}

/** `page-<n>` with the smallest free `n`. */
export function nextPageId(document: BoardDocument): PageId {
  const taken = new Set(document.pages.map((page) => page.id));
  for (let n = 1; ; n += 1) {
    const id = `page-${n}`;
    if (!taken.has(id)) {
      return id;
    }
  }
}

function emptyPage(id: PageId, title: string): Page {
  return { id, title, defaultView: "compact", cards: [], names: [], combined: [], guards: [], links: [] };
}

/** Adds an empty page after `after` (else last), titled "Page N" unless given a title. Returns its id. */
export function addPage(document: BoardDocument, options: { readonly title?: string; readonly after?: PageId } = {}): PageId {
  const id = nextPageId(document);
  const index = options.after === undefined ? -1 : document.pages.findIndex((page) => page.id === options.after);
  const at = index < 0 ? document.pages.length : index + 1;
  const title = options.title !== undefined && options.title.trim().length > 0 ? options.title : `Page ${document.pages.length + 1}`;
  document.pages.splice(at, 0, emptyPage(id, title));
  return id;
}

export function renamePage(document: BoardDocument, pageId: PageId, title: string): void {
  const page = findPage(document, pageId);
  if (page && title.trim().length > 0) {
    page.title = title;
  }
}

/** Moves a page one place up (-1) or down (1); nothing at either end. */
export function movePage(document: BoardDocument, pageId: PageId, delta: -1 | 1): void {
  const index = document.pages.findIndex((page) => page.id === pageId);
  const to = index + delta;
  const page = document.pages[index];
  if (!page || to < 0 || to >= document.pages.length) {
    return;
  }
  document.pages.splice(index, 1);
  document.pages.splice(to, 0, page);
}

/** Deletes a page, unless it is the only one. */
export function deletePage(document: BoardDocument, pageId: PageId): void {
  if (document.pages.length > 1) {
    document.pages = document.pages.filter((page) => page.id !== pageId);
  }
}
