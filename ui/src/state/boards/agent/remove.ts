/**
 * `remove_items`: takes cards, step groups, links and connections, frames, resource cards, and notes
 * off a page, with what would dangle (a card's step groups, the links that end at what goes, a frame
 * left with no members); a note attached to what goes stays where it was, free. A removed frame's
 * members stay, in the frame that held it, unless `keepMembers` is false. Removing what a person drew
 * or wrote (a description, a group's or frame's title, a link's label, a connection, a resource card, a
 * note) is refused unless the call says the person asked for it.
 */

import type { AgentItemResult, RemoveItemsOp } from "@unfold/desktop/agent-api";
import type { Page } from "../../../ipc/generated";
import { findCard, removeItems } from "../document";
import { findFrame, removeFrame } from "../frames";
import { findNote } from "../notes";
import { findResource } from "../resources";
import { linkResult } from "./connections";
import { cardName, cardResult, guardPeopleText, invalid, type OpContext } from "./context";
import { frameResult } from "./frames";
import { noteName, noteResult } from "./notes";
import { resourceName, resourceResult } from "./resources";

/** Removes item `id` as asked and returns the ids that went with it. */
function remove(page: Page, id: string, keepMembers: boolean): string[] {
  return findFrame(page, id) ? removeFrame(page, id, keepMembers) : removeItems(page, [id]);
}

/** Refuses when a removed item carries a person's words, unless the person asked. */
function guard(ctx: OpContext, page: Page, id: string, replace: boolean | undefined): void {
  const card = findCard(page, id);
  if (card?.description) {
    guardPeopleText(`the description of ${cardName(ctx, card)}`, card.description.author, replace);
  }
  const group = page.groups.find((item) => item.id === id);
  if (group) {
    guardPeopleText(`the step group “${group.title}”`, group.author, replace);
    if (group.description) {
      guardPeopleText(`the description of the step group “${group.title}”`, group.description.author, replace);
    }
  }
  const link = page.links.find((item) => item.id === id);
  if (link?.label) {
    guardPeopleText(`the label “${link.label.text}”`, link.label.author, replace);
  }
  if (link?.kind !== undefined) {
    guardPeopleText(`the ${link.kind} connection`, link.author, replace);
    if (link.description) {
      guardPeopleText(`the description of the ${link.kind} connection`, link.description.author, replace);
    }
  }
  const resource = findResource(page, id);
  if (resource) {
    guardPeopleText(resourceName(resource), resource.author, replace);
    if (resource.description) {
      guardPeopleText(`the description of ${resourceName(resource)}`, resource.description.author, replace);
    }
  }
  const note = findNote(page, id);
  if (note) {
    guardPeopleText(`the note “${noteName(note)}”`, note.author, replace);
  }
  const frame = findFrame(page, id);
  if (frame) {
    guardPeopleText(`the frame “${frame.title}”`, frame.author, replace);
    if (frame.description) {
      guardPeopleText(`the description of the frame “${frame.title}”`, frame.description.author, replace);
    }
  }
}

/** What a removed item was, as its result entry (read before it goes). */
function removedResult(ctx: OpContext, page: Page, id: string): AgentItemResult[] {
  const card = findCard(page, id);
  if (card) {
    return [cardResult(ctx, card, "removed")];
  }
  const group = page.groups.find((item) => item.id === id);
  if (group) {
    return [{ kind: "group", id, status: "removed", card: group.card, name: group.title }];
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    return [linkResult(link, "removed")];
  }
  const resource = findResource(page, id);
  if (resource) {
    return [resourceResult(ctx, page, resource, "removed")];
  }
  const note = findNote(page, id);
  if (note) {
    return [noteResult(note, "removed", false)];
  }
  const frame = findFrame(page, id);
  return frame ? [frameResult(ctx, page, frame, "removed")] : [];
}

export function applyRemoveItems(page: Page, op: RemoveItemsOp, ctx: OpContext): AgentItemResult[] {
  const keepMembers = op.keepMembers !== false;
  // Items an earlier removal took with it (a card's groups and links) need no second removal.
  const gone = new Set<string>();
  return op.ids.flatMap((id): AgentItemResult[] => {
    if (gone.has(id)) {
      return [];
    }
    const known = [page.cards, page.groups, page.links, page.frames ?? [], page.resources ?? [], page.notes ?? []].some((list) => list.some((item) => item.id === id));
    if (!known) {
      throw invalid(`There is no card, step group, link, frame, resource card, or note with id “${id}” on this page. read_page lists the page's ids.`);
    }
    // What goes with it, found on a copy first, so a person's words are checked before anything changes.
    const going = remove(JSON.parse(JSON.stringify(page)) as Page, id, keepMembers).sort((a, b) => Number(b === id) - Number(a === id));
    for (const item of going) {
      guard(ctx, page, item, op.replacePeopleText);
    }
    const results = going.flatMap((item) => removedResult(ctx, page, item));
    remove(page, id, keepMembers);
    for (const item of going) {
      gone.add(item);
    }
    return results;
  });
}
