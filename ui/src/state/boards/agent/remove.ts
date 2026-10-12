/**
 * `remove_items`: takes cards, step groups, links and connections, frames, resource cards, notes, and
 * proposed steps off a page, with what would dangle (a card's step groups and proposed steps, the
 * links that end at what goes, a frame left with no members); a note attached to what goes stays
 * where it was, free. A removed frame's members stay, in the frame that held it, unless `keepMembers`
 * is false. Removing a proposed step never edits code. Removing what a person drew or wrote
 * (`itemWords`: a description, a group's or frame's title, a link's label, a connection or drawn
 * link, a resource card, a note, a proposed step), what goes with an item included, is refused unless
 * the call says the person asked for it.
 */

import type { AgentItemResult, RemoveItemsOp } from "@unfold/desktop/agent-api";
import type { Page } from "../../../ipc/generated";
import { findCard, removalCascade, removeItems } from "../document";
import { findFrame, frameRemoval, removeFrame } from "../frames";
import { findNote } from "../notes";
import { findProposal } from "../proposals";
import { findResource } from "../resources";
import { linkResult } from "./connections";
import { cardResult, guardRemoval, invalid, type OpContext } from "./context";
import { frameResult } from "./frames";
import { noteResult } from "./notes";
import { resourceResult } from "./resources";

/** Removes item `id` as asked and returns the ids that went with it. */
function remove(page: Page, id: string, keepMembers: boolean): string[] {
  return findFrame(page, id) ? removeFrame(page, id, keepMembers) : removeItems(page, [id]);
}

/** What removing item `id` as asked takes, `id` first: what would dangle, and a frame's members unless they are kept. */
function going(page: Page, id: string, keepMembers: boolean): string[] {
  return [...(findFrame(page, id) ? frameRemoval(page, id, keepMembers) : removalCascade(page, [id]))];
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
  const proposal = findProposal(page, id);
  if (proposal) {
    return [
      {
        kind: "proposal",
        id,
        status: "removed",
        card: proposal.card,
        after: proposal.after,
        ...(proposal.path !== undefined ? { path: proposal.path } : {}),
        title: proposal.title,
        ...(proposal.reason !== undefined ? { reason: proposal.reason } : {}),
      },
    ];
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
    const known = [page.cards, page.groups, page.links, page.frames ?? [], page.resources ?? [], page.notes ?? [], page.proposals ?? []].some((list) => list.some((item) => item.id === id));
    if (!known) {
      throw invalid(`There is no card, step group, link, frame, resource card, note, or proposed step with id “${id}” on this page. read_page lists the page's ids.`);
    }
    // What goes with it, found first, so a person's words are checked before anything changes.
    const items = going(page, id, keepMembers);
    guardRemoval(ctx, page, items, op.replacePeopleText);
    const results = items.flatMap((item) => removedResult(ctx, page, item));
    remove(page, id, keepMembers);
    for (const item of items) {
      gone.add(item);
    }
    return results;
  });
}
