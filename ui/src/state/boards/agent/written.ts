/**
 * What an agent's edit wrote on each item it changed, from the page before and after it: the fields
 * whose words are new (`WrittenField`). An item it only moved, opened, or added from code wrote nothing.
 * Pure.
 */

import type { Card, Page } from "../../../ipc/generated";
import { cardKey } from "../document";
import type { ChangedItem, WrittenField } from "./working";

/** Whether a key is a block inside the definition `key` (its own steps, not the card itself). */
function inside(key: string, card: string): boolean {
  return key.startsWith(`${card}/`);
}

/** The words a card's flow carries on a page: its blocks' names, its combined steps, and its proposed steps. */
function stepWords(page: Page | undefined, card: string): string {
  if (!page) {
    return "";
  }
  const names = page.names.filter((entry) => inside(entry.key, card)).map((entry) => `${entry.key}=${entry.text}`);
  const combined = page.combined.filter((step) => step.members.some((member) => inside(member, card))).map((step) => `${step.title}:${step.description?.text ?? ""}:${step.members.join(",")}`);
  const proposals = (page.proposals ?? [])
    .filter((proposal) => inside("after" in proposal ? proposal.after : proposal.start, card) || ("start" in proposal && proposal.start === card))
    .map((proposal) => `${proposal.title}:${proposal.reason ?? ""}`);
  return JSON.stringify([names.sort(), combined.sort(), proposals.sort()]);
}

/** A card's title on a page: the name its definition's key has there. */
function cardTitle(page: Page | undefined, card: Card): string | undefined {
  const key = cardKey(card);
  return page?.names.find((entry) => entry.key === key)?.text;
}

/** The fields of one item an edit wrote, comparing it on the page before (`was`) and after (`now`). */
function fieldsOf(was: Page | undefined, now: Page, id: string): WrittenField[] {
  const changed = (field: WrittenField, before: string | undefined, after: string | undefined): WrittenField[] =>
    after !== undefined && after !== before ? [field] : [];
  const card = now.cards.find((item) => item.id === id);
  if (card) {
    const old = was?.cards.find((item) => item.id === id);
    const key = cardKey(card);
    return [
      ...changed("title", cardTitle(was, card), cardTitle(now, card)),
      ...changed("description", old?.description?.text, card.description?.text),
      ...(stepWords(was, key) !== stepWords(now, key) ? (["steps"] as const) : []),
    ];
  }
  const link = now.links.find((item) => item.id === id);
  if (link) {
    const old = was?.links.find((item) => item.id === id);
    const says = (item: typeof link | undefined) =>
      item ? JSON.stringify([item.kind, item.channel, item.carries, item.label?.text, item.description?.text, item.cardinality]) : undefined;
    return changed("target", says(old), says(link));
  }
  const frame = (now.frames ?? []).find((item) => item.id === id);
  if (frame) {
    const old = (was?.frames ?? []).find((item) => item.id === id);
    return [...changed("title", old?.title, frame.title), ...changed("description", old?.description?.text, frame.description?.text)];
  }
  const resource = (now.resources ?? []).find((item) => item.id === id);
  if (resource) {
    const old = (was?.resources ?? []).find((item) => item.id === id);
    return [...changed("name", old?.name, resource.name), ...changed("description", old?.description?.text, resource.description?.text)];
  }
  const note = (now.notes ?? []).find((item) => item.id === id);
  if (note) {
    return changed("text", (was?.notes ?? []).find((item) => item.id === id)?.text, note.text);
  }
  return [];
}

/** The items an edit changed (`ids`), each with the fields it wrote there. */
export function writtenItems(was: Page | undefined, now: Page, ids: readonly string[]): ChangedItem[] {
  return ids.map((id) => ({ id, fields: fieldsOf(was, now, id) }));
}
