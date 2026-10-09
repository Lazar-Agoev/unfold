/**
 * Pure edits of an `unfold-board` v1 document (architecture §9.1). Each function is an Immer recipe
 * over a draft document, so the store can record one undo step per edit. Optional fields are
 * deleted rather than set to `undefined`, matching what the file holds (`folded` only when true,
 * `description` only when there is text).
 */

import type { BoardDocument, Card, CardRef, Description, Group, View } from "../../ipc/generated";

/** The structural key a card shows. */
export function cardKey(card: Card): string {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

/** The first card that shows `key`, if any. */
export function findCardByKey(document: BoardDocument, key: string): Card | undefined {
  return document.cards.find((card) => cardKey(card) === key);
}

export function findCard(document: BoardDocument, cardId: string): Card | undefined {
  return document.cards.find((card) => card.id === cardId);
}

/** `createOrder` → `create-order`; anything else non-alphanumeric becomes one dash. */
function slug(text: string): string {
  const value = text
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return value.length > 0 ? value.slice(0, 48).replace(/-+$/, "") : "card";
}

/** A readable name for a key: `OrderService.place`, or `POST /orders` for an HTTP entrance. */
export function keyDisplayName(key: string): string {
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

/** An id unique among the document's cards and groups, derived from `base`. */
export function uniqueId(document: BoardDocument, base: string): string {
  const taken = new Set([...document.cards.map((card) => card.id), ...document.groups.map((group) => group.id)]);
  const stem = slug(base);
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

export interface NewCard {
  readonly ref: CardRef;
  readonly at: readonly [number, number];
  readonly view?: View;
}

/** Adds a card and returns its id. Positions are rounded: the file holds finite numbers. */
export function addCard(document: BoardDocument, card: NewCard): string {
  const key = "block" in card.ref ? card.ref.block : card.ref.entrance;
  const id = uniqueId(document, keyDisplayName(key));
  document.cards.push({
    id,
    ref: card.ref,
    at: [Math.round(card.at[0]), Math.round(card.at[1])],
    ...(card.view ? { view: card.view } : {}),
  });
  return id;
}

/** Removes a card and the groups inside it. */
export function removeCard(document: BoardDocument, cardId: string): void {
  document.cards = document.cards.filter((card) => card.id !== cardId);
  document.groups = document.groups.filter((group) => group.card !== cardId);
}

/** Moves cards; ids not on the board are ignored. */
export function moveCards(document: BoardDocument, positions: Readonly<Record<string, readonly [number, number]>>): void {
  for (const card of document.cards) {
    const at = positions[card.id];
    if (at && Number.isFinite(at[0]) && Number.isFinite(at[1])) {
      card.at = [Math.round(at[0]), Math.round(at[1])];
    }
  }
}

/** Sets a card's own Code/Flow switch (saved with the board). */
export function setCardView(document: BoardDocument, cardId: string, view: View): void {
  const card = findCard(document, cardId);
  if (card) {
    card.view = view;
  }
}

/** Folds or unfolds cards to their title; `folded` is written only when true. */
export function setCardsFolded(document: BoardDocument, cardIds: readonly string[], folded: boolean): void {
  const wanted = new Set(cardIds);
  for (const card of document.cards) {
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
export function description(text: string, author: string): Description {
  return { text, author, signature: "" };
}

/** Sets or clears (blank text) a card's description. */
export function setCardDescription(document: BoardDocument, cardId: string, text: string, author: string): void {
  const card = findCard(document, cardId);
  if (!card) {
    return;
  }
  if (text.trim().length === 0) {
    delete card.description;
  } else if (card.description?.text !== text) {
    card.description = description(text, author);
  }
}

/** Sets or clears (blank text) a Flow label. */
export function setLabel(document: BoardDocument, key: string, text: string, author: string): void {
  if (text.trim().length === 0) {
    // Rebuild without the key: the labels map is a plain record.
    document.labels = Object.fromEntries(Object.entries(document.labels).filter(([labeled]) => labeled !== key));
  } else if (document.labels[key]?.text !== text) {
    document.labels[key] = description(text, author);
  }
}

/** Adds a group of consecutive sibling statements inside a card and returns its id. */
export function addGroup(document: BoardDocument, cardId: string, members: readonly string[], title: string): string {
  const id = uniqueId(document, title.length > 0 ? title : "group");
  const group: Group = { id, card: cardId, title, members: [...members], collapsed: false };
  document.groups.push(group);
  return id;
}

export interface GroupChange {
  readonly title?: string;
  readonly collapsed?: boolean;
  /** Blank text clears the description. */
  readonly description?: { readonly text: string; readonly author: string };
}

export function updateGroup(document: BoardDocument, groupId: string, change: GroupChange): void {
  const group = document.groups.find((item) => item.id === groupId);
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

export function removeGroup(document: BoardDocument, groupId: string): void {
  document.groups = document.groups.filter((group) => group.id !== groupId);
}

export function setTitle(document: BoardDocument, title: string): void {
  if (title.trim().length > 0) {
    document.title = title;
  }
}

