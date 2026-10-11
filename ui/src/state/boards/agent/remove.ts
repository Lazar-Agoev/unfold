/**
 * `remove_items`: takes cards (with their step groups and the links that end at them), step groups,
 * and links off a page. Removing text a person wrote (a description, a group's title, a link's label)
 * is refused unless the call says the person asked for it.
 */

import type { AgentItemResult, RemoveItemsOp } from "@unfold/desktop/agent-api";
import type { Group, Link, Page } from "../../../ipc/generated";
import { findCard, removeCard, removeGroup, removeLink } from "../document";
import { cardName, cardResult, guardPeopleText, invalid, type OpContext } from "./context";

function guardGroup(group: Group, replace: boolean | undefined): void {
  guardPeopleText(`the step group “${group.title}”`, group.author, replace);
  if (group.description) {
    guardPeopleText(`the description of the step group “${group.title}”`, group.description.author, replace);
  }
}

function guardLink(link: Link, replace: boolean | undefined): void {
  if (link.label) {
    guardPeopleText(`the label “${link.label.text}”`, link.label.author, replace);
  }
}

function groupRemoved(group: Group): AgentItemResult {
  return { kind: "group", id: group.id, status: "removed", card: group.card, name: group.title };
}

function linkRemoved(link: Link): AgentItemResult {
  return { kind: "link", id: link.id, status: "removed", from: link.from, to: link.to, ...(link.label ? { name: link.label.text } : {}) };
}

export function applyRemoveItems(page: Page, op: RemoveItemsOp, ctx: OpContext): AgentItemResult[] {
  const replace = op.replacePeopleText;
  // Items an earlier removal took with it (a card's groups and links) need no second removal.
  const gone = new Set<string>();
  return op.ids.flatMap((id): AgentItemResult[] => {
    if (gone.has(id)) {
      return [];
    }
    const card = findCard(page, id);
    if (card) {
      const name = cardName(ctx, card);
      if (card.description) {
        guardPeopleText(`the description of ${name}`, card.description.author, replace);
      }
      const groups = page.groups.filter((group) => group.card === id);
      const links = page.links.filter((link) => link.from === id || link.to === id);
      groups.forEach((group) => {
        guardGroup(group, replace);
      });
      links.forEach((link) => {
        guardLink(link, replace);
      });
      const removed = [cardResult(ctx, card, "removed"), ...groups.map(groupRemoved), ...links.map(linkRemoved)];
      removeCard(page, id);
      for (const item of [...groups, ...links]) {
        gone.add(item.id);
      }
      return removed;
    }
    const group = page.groups.find((item) => item.id === id);
    if (group) {
      guardGroup(group, replace);
      const removed = groupRemoved(group);
      removeGroup(page, id);
      return [removed];
    }
    const link = page.links.find((item) => item.id === id);
    if (link) {
      guardLink(link, replace);
      const removed = linkRemoved(link);
      removeLink(page, id);
      return [removed];
    }
    throw invalid(`There is no card, step group, or link with id “${id}” on this page. read_page lists the page's ids.`);
  });
}
