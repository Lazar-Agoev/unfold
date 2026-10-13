/**
 * Pure edits of an `unfold-board` document. Each function is an Immer recipe over a
 * draft document or page, so the store can record one undo step per edit. Optional fields are deleted
 * rather than set to `undefined`, matching what the file holds (`description` and `label` only when
 * there is text).
 *
 * Cards, groups, links, frames, resource cards, notes, and proposed steps share one id space per page; page ids are
 * unique within the board. Both match `/^[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$/`. Removing an item takes
 * what would dangle with it, as the engine does.
 */

import type { BoardDocument, Card, CardRef, Description, Group, Key, LabelEntry, Link, Page, View } from "../../ipc/generated";
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

/** A readable name for a key: `OrderService.place`, or `POST /orders` for an HTTP entrance. */
export function keyDisplayName(key: Key): string {
  if (key.startsWith("entrance:")) {
    try {
      const parts: unknown = JSON.parse(key.slice("entrance:".length));
      if (Array.isArray(parts)) {
        const words = parts.slice(2, 4).filter((part): part is string => typeof part === "string" && part.length > 0);
        if (words.length > 0) {
          return words.join(" ");
        }
      }
    } catch {
      // Not JSON: fall through to the generic name.
    }
    return "entrance";
  }
  // A path may hold `#`; a step never does (names escape it), so the file ends at the last `#`.
  const hash = key.lastIndexOf("#");
  const local = key.slice(hash + 1);
  if (local.length === 0) {
    return key.slice(0, Math.max(hash, 0)).split("/").at(-1) ?? key;
  }
  const names = local
    .split("/")
    .map((step) => step.slice(step.indexOf(":") + 1).replace(/\[\d+\]$/, ""))
    .filter((name) => name.length > 0);
  return names.slice(-2).join(".") || local;
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

/** The ids of the page's items: cards, groups, links, frames, resources, notes, and proposed steps share one id space. */
function takenIds(page: Page): Set<string> {
  const items = [page.cards, page.groups, page.links, page.frames ?? [], page.resources ?? [], page.notes ?? [], page.proposals ?? []];
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
  readonly at: readonly [number, number];
  readonly view?: View;
}

/** Adds a card and returns its id. Positions are rounded: the file holds finite numbers. */
export function addCard(page: Page, card: NewCard): string {
  const key = "block" in card.ref ? card.ref.block : card.ref.entrance;
  const id = uniqueId(page, keyDisplayName(key));
  page.cards.push({
    id,
    ref: card.ref,
    at: [Math.round(card.at[0]), Math.round(card.at[1])],
    ...(card.view ? { view: card.view } : {}),
  });
  return id;
}

/**
 * What removing `ids` takes with it, the engine's removal cascade (`remove_items`): the groups and
 * proposed steps of a removed card, the links that end at a removed item, and a frame whose members are
 * all removed (then its links, and so on). Includes `ids` themselves.
 */
export function removalCascade(page: Page, ids: readonly string[]): Set<string> {
  const removed = new Set(ids);
  for (let before = -1; before !== removed.size; ) {
    before = removed.size;
    for (const group of page.groups) {
      if (removed.has(group.card)) {
        removed.add(group.id);
      }
    }
    for (const proposal of page.proposals ?? []) {
      if (removed.has(proposal.card)) {
        removed.add(proposal.id);
      }
    }
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
  page.groups = kept(page.groups);
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

/** Removes a card, the groups and proposed steps inside it, the links that end at it, and a frame it leaves empty. */
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

/** Sets a card's saved view: `diagram` (compact in Diagram) or `flow` (expanded); a legacy `code` stays readable. */
export function setCardView(page: Page, cardId: string, view: View): void {
  const card = findCard(page, cardId);
  if (card) {
    card.view = view;
  }
}

/** Expands a card in Diagram (its written flow, an entrance's evidence and targets) or makes it compact again. */
export function setCardExpanded(page: Page, cardId: string, expanded: boolean): void {
  setCardView(page, cardId, expanded ? "flow" : "diagram");
}

/** The most calls one card expands in place, and how deep (the board format's limits). */
export const MAX_EXPANDED_CALLS = 64;
export const MAX_EXPANSION_DEPTH = 3;

/**
 * The calls a card expands in place in Diagram: paths of 1 to 3 keys, outermost first, each a call
 * occurrence (or a callback) of the block shown at that level.
 */
export function callExpansions(card: Pick<Card, "expanded">): readonly (readonly Key[])[] {
  return card.expanded ?? [];
}

function samePath(a: readonly Key[], b: readonly Key[]): boolean {
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

/** Whether the card expands the call at `path` in place. */
export function isCallExpanded(card: Pick<Card, "expanded">, path: readonly Key[]): boolean {
  return callExpansions(card).some((item) => samePath(item, path));
}

/**
 * Expands the call at `path` in place, or collapses it (with every expansion inside it). A path
 * deeper than three levels, or one more than the card holds, changes nothing. An empty list is
 * deleted, as the file omits it.
 */
export function toggleCallExpansion(page: Page, cardId: string, path: readonly Key[]): void {
  const card = findCard(page, cardId);
  if (!card || path.length === 0 || path.length > MAX_EXPANSION_DEPTH) {
    return;
  }
  const paths = callExpansions(card);
  let next: Key[][];
  if (paths.some((item) => samePath(item, path))) {
    next = paths.filter((item) => !samePath(item.slice(0, path.length), path)).map((item) => [...item]);
  } else if (paths.length < MAX_EXPANDED_CALLS) {
    next = [...paths.map((item) => [...item]), [...path]];
  } else {
    return;
  }
  if (next.length > 0) {
    card.expanded = next;
  } else {
    delete card.expanded;
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

/** The label texts of a page, by key. */
export function labelTexts(labels: readonly LabelEntry[]): Record<Key, string> {
  return Object.fromEntries(labels.map((label) => [label.key, label.text]));
}

/** Sets or clears (blank text) the Flow label of `key`. Labels sort by key, as the file writes them. */
export function setLabel(page: Page, key: Key, text: string, author: string): void {
  const index = page.labels.findIndex((label) => label.key === key);
  if (text.trim().length === 0) {
    if (index >= 0) {
      page.labels.splice(index, 1);
    }
    return;
  }
  if (index >= 0 && page.labels[index]?.text === text) {
    return;
  }
  const label: LabelEntry = { key, ...description(text, author) };
  if (index >= 0) {
    page.labels[index] = label;
    return;
  }
  const at = page.labels.findIndex((existing) => key < existing.key);
  page.labels.splice(at < 0 ? page.labels.length : at, 0, label);
}

/**
 * Adds a group of consecutive sibling statements inside a card and returns its id: `id` when given
 * (a free one, `isFreeId`), else one from the title.
 */
export function addGroup(
  page: Page,
  cardId: string,
  members: readonly string[],
  title: string,
  options: { readonly id?: string; readonly collapsed?: boolean; readonly author?: string } = {},
): string {
  const id = options.id ?? uniqueId(page, title.length > 0 ? title : "group");
  const group: Group = {
    id,
    card: cardId,
    title,
    members: [...members],
    collapsed: options.collapsed ?? false,
    ...(options.author !== undefined ? { author: options.author } : {}),
  };
  page.groups.push(group);
  return id;
}

export interface GroupChange {
  readonly title?: string;
  /** Who wrote a new title; a title changed without one has no author (it is no longer anyone else's). */
  readonly author?: string;
  readonly collapsed?: boolean;
  /** Blank text clears the description. */
  readonly description?: { readonly text: string; readonly author: string };
  /** The statements it holds now (consecutive siblings, as `addGroup` takes them). */
  readonly members?: readonly string[];
}

export function updateGroup(page: Page, groupId: string, change: GroupChange): void {
  const group = page.groups.find((item) => item.id === groupId);
  if (!group) {
    return;
  }
  if (change.title !== undefined && change.title.trim().length > 0 && change.title !== group.title) {
    group.title = change.title;
    if (change.author !== undefined) {
      group.author = change.author;
    } else {
      delete group.author;
    }
  }
  if (change.collapsed !== undefined) {
    group.collapsed = change.collapsed;
  }
  if (change.members !== undefined && change.members.join("\u0000") !== group.members.join("\u0000")) {
    group.members = [...change.members];
  }
  if (change.description) {
    if (change.description.text.trim().length === 0) {
      delete group.description;
    } else if (group.description?.text !== change.description.text) {
      group.description = description(change.description.text, change.description.author);
    }
  }
}

export function removeGroup(page: Page, groupId: string): void {
  page.groups = page.groups.filter((group) => group.id !== groupId);
}

/** Draws a link between two different cards, resource cards, or frames of the page and returns its id (undefined if it can't). */
export function addLink(page: Page, from: string, to: string): string | undefined {
  const end = (id: string) => findCard(page, id) !== undefined || [...(page.frames ?? []), ...(page.resources ?? [])].some((item) => item.id === id);
  if (from === to || !end(from) || !end(to)) {
    return undefined;
  }
  const id = uniqueId(page, `${from}-to-${to}`);
  const link: Link = { id, from, to };
  page.links.push(link);
  return id;
}

/** Sets or clears (blank text) a link's label. */
export function setLinkLabel(page: Page, linkId: string, text: string, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (!link) {
    return;
  }
  if (text.trim().length === 0) {
    delete link.label;
  } else if (link.label?.text !== text) {
    link.label = description(text, author);
  }
}

/** Removes a link; a note attached to it stays where it was. */
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

function emptyPage(id: PageId, title: string, defaultView: View): Page {
  return { id, title, defaultView, cards: [], groups: [], labels: [], guards: [], links: [] };
}

/** Adds an empty page after `after` (else last), titled "Page N" unless given a title. Returns its id. */
export function addPage(document: BoardDocument, options: { readonly title?: string; readonly after?: PageId } = {}): PageId {
  const id = nextPageId(document);
  const index = options.after === undefined ? -1 : document.pages.findIndex((page) => page.id === options.after);
  const at = index < 0 ? document.pages.length : index + 1;
  const title = options.title !== undefined && options.title.trim().length > 0 ? options.title : `Page ${document.pages.length + 1}`;
  // New pages show Diagram, as the engine's new pages do.
  document.pages.splice(at, 0, emptyPage(id, title, "diagram"));
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
