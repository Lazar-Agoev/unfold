/**
 * Pure edits of an `unfold-board` document. Each function is an Immer recipe over a
 * draft document or page, so the store can record one undo step per edit. Optional fields are deleted
 * rather than set to `undefined`, matching what the file holds (`folded` only when true, `description`
 * and `label` only when there is text).
 *
 * Card, group, and link ids are unique together within their page; page ids within the board. Both
 * match `/^[A-Za-z0-9][A-Za-z0-9_.-]{0,99}$/`.
 */

import type { BoardDocument, Card, CardRef, Description, Group, Key, LabelEntry, Link, Page, ProjectName, View } from "../../ipc/generated";
import type { PageId } from "./types";

/** The structural key a card shows. */
export function cardKey(card: Card): Key {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

export function findPage(document: BoardDocument, pageId: PageId): Page | undefined {
  return document.pages.find((page) => page.id === pageId);
}

export function findCard(page: Page, cardId: string): Card | undefined {
  return page.cards.find((card) => card.id === cardId);
}

/** The first card of the page that shows `key` from the project `ref.project` names (undefined: the board's own). */
export function findCardByRef(page: Page, key: Key, project: ProjectName | undefined): Card | undefined {
  return page.cards.find((card) => cardKey(card) === key && card.ref.project === project);
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
  const local = key.slice(key.indexOf("#") + 1);
  if (local.length === 0) {
    return key.split("/").at(-1) ?? key;
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

/** An id unique among the page's cards, groups, and links, derived from `base`. */
export function uniqueId(page: Page, base: string): string {
  const taken = new Set([...page.cards.map((card) => card.id), ...page.groups.map((group) => group.id), ...page.links.map((link) => link.id)]);
  return firstFree(slug(base, "card"), taken);
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

/** Removes a card, the groups inside it, and the links that end at it. */
export function removeCard(page: Page, cardId: string): void {
  page.cards = page.cards.filter((card) => card.id !== cardId);
  page.groups = page.groups.filter((group) => group.card !== cardId);
  page.links = page.links.filter((link) => link.from !== cardId && link.to !== cardId);
}

/** Moves cards; ids not on the page are ignored. */
export function moveCards(page: Page, positions: Readonly<Record<string, readonly [number, number]>>): void {
  for (const card of page.cards) {
    const at = positions[card.id];
    if (at && Number.isFinite(at[0]) && Number.isFinite(at[1])) {
      card.at = [Math.round(at[0]), Math.round(at[1])];
    }
  }
}

/** Sets a card's own Code/Flow switch (saved with the board). */
export function setCardView(page: Page, cardId: string, view: View): void {
  const card = findCard(page, cardId);
  if (card) {
    card.view = view;
  }
}

/** Folds or unfolds cards to their title; `folded` is written only when true. */
export function setCardsFolded(page: Page, cardIds: readonly string[], folded: boolean): void {
  const wanted = new Set(cardIds);
  for (const card of page.cards) {
    if (!wanted.has(card.id)) {
      continue;
    }
    if (folded) {
      card.folded = true;
    } else {
      delete card.folded;
    }
  }
}

/** A new, unsigned description (the next save signs it from verified source). */
function description(text: string, author: string): Description {
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

/** Labels and guards sort by (project, key), as the file writes them. */
function labelOrder(a: Pick<LabelEntry, "project" | "key">, b: Pick<LabelEntry, "project" | "key">): number {
  const pa = a.project ?? "";
  const pb = b.project ?? "";
  if (pa !== pb) {
    return pa < pb ? -1 : 1;
  }
  return a.key < b.key ? -1 : a.key > b.key ? 1 : 0;
}

/** The label texts of a page that apply to cards from `project` (undefined: the board's own), by key. */
export function labelsFor(labels: readonly LabelEntry[], project: ProjectName | undefined): Record<Key, string> {
  return Object.fromEntries(labels.filter((label) => label.project === project).map((label) => [label.key, label.text]));
}

/** Sets or clears (blank text) the Flow label of `key` in the project `project` names. */
export function setLabel(page: Page, project: ProjectName | undefined, key: Key, text: string, author: string): void {
  const index = page.labels.findIndex((label) => label.project === project && label.key === key);
  if (text.trim().length === 0) {
    if (index >= 0) {
      page.labels.splice(index, 1);
    }
    return;
  }
  if (index >= 0 && page.labels[index]?.text === text) {
    return;
  }
  const label: LabelEntry = { ...(project !== undefined ? { project } : {}), key, ...description(text, author) };
  if (index >= 0) {
    page.labels[index] = label;
    return;
  }
  const at = page.labels.findIndex((existing) => labelOrder(label, existing) < 0);
  page.labels.splice(at < 0 ? page.labels.length : at, 0, label);
}

/** Adds a group of consecutive sibling statements inside a card and returns its id. */
export function addGroup(page: Page, cardId: string, members: readonly string[], title: string): string {
  const id = uniqueId(page, title.length > 0 ? title : "group");
  const group: Group = { id, card: cardId, title, members: [...members], collapsed: false };
  page.groups.push(group);
  return id;
}

export interface GroupChange {
  readonly title?: string;
  readonly collapsed?: boolean;
  /** Blank text clears the description. */
  readonly description?: { readonly text: string; readonly author: string };
}

export function updateGroup(page: Page, groupId: string, change: GroupChange): void {
  const group = page.groups.find((item) => item.id === groupId);
  if (!group) {
    return;
  }
  if (change.title !== undefined && change.title.trim().length > 0) {
    group.title = change.title;
  }
  if (change.collapsed !== undefined) {
    group.collapsed = change.collapsed;
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

/** Draws a link between two different cards of the page and returns its id (undefined if it can't). */
export function addLink(page: Page, from: string, to: string): string | undefined {
  if (from === to || !findCard(page, from) || !findCard(page, to)) {
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

export function removeLink(page: Page, linkId: string): void {
  page.links = page.links.filter((link) => link.id !== linkId);
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
  const neighbour = document.pages[at - 1] ?? document.pages[0];
  const title = options.title !== undefined && options.title.trim().length > 0 ? options.title : `Page ${document.pages.length + 1}`;
  document.pages.splice(at, 0, emptyPage(id, title, neighbour?.defaultView ?? "code"));
  return id;
}

export function renamePage(document: BoardDocument, pageId: PageId, title: string): void {
  const page = findPage(document, pageId);
  if (page && title.trim().length > 0) {
    page.title = title;
  }
}

/** Copies a page (cards, groups, labels, guards, links) right after it, titled "<title> copy". */
export function duplicatePage(document: BoardDocument, pageId: PageId): PageId | undefined {
  const index = document.pages.findIndex((page) => page.id === pageId);
  const source = document.pages[index];
  if (!source) {
    return undefined;
  }
  const id = nextPageId(document);
  // A JSON copy: `source` may be an Immer draft, which structured cloning refuses.
  const copy = JSON.parse(JSON.stringify(source)) as Page;
  document.pages.splice(index + 1, 0, { ...copy, id, title: `${source.title} copy` });
  return id;
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
