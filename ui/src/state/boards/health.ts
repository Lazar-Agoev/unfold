/**
 * Board health in the UI (contract §6.4): which item is about what on which page, what each offered
 * repair does in words (shown before it is applied), and the `repair_board` request for it.
 */

import type { Health, HealthItem, HealthKind, Key, Page, ProjectKey, ProjectName, RepairKind, RepairSpec, Target } from "../../ipc/generated";
import { cardKey, findCard, keyDisplayName } from "./document";
import type { PageId } from "./types";

/** The items about one target (a card, group, label, or link) of a page, in the engine's order. */
export function itemsFor(health: Health | undefined, page: PageId, target: Target): HealthItem[] {
  return (health?.items ?? []).filter((item) => item.page === page && sameTarget(item.target, target));
}

function sameLabel(a: ProjectKey, b: ProjectKey): boolean {
  return a.key === b.key && a.project === b.project;
}

function sameTarget(a: Target, b: Target): boolean {
  if ("card" in a) return "card" in b && a.card === b.card;
  if ("group" in a) return "group" in b && a.group === b.group;
  if ("link" in a) return "link" in b && a.link === b.link;
  return "label" in b && sameLabel(a.label, b.label);
}

const KIND_WORDS: Readonly<Record<HealthKind, string>> = {
  missing: "Missing",
  outdated: "Outdated",
  brokenGroup: "Broken group",
  review: "Needs review",
  notConnected: "Not connected",
  brokenLink: "Broken link",
};

export function kindWord(kind: HealthKind): string {
  return KIND_WORDS[kind];
}

/** What an item is about, in words: the card's name, the group's title, the labeled node, or the link. */
function targetName(target: Target, page: Page): string {
  if ("card" in target) {
    const card = findCard(page, target.card);
    return card ? keyDisplayName(cardKey(card)) : target.card;
  }
  if ("group" in target) {
    return page.groups.find((item) => item.id === target.group)?.title ?? target.group;
  }
  if ("link" in target) {
    const link = page.links.find((item) => item.id === target.link);
    const to = link ? findCard(page, link.to) : undefined;
    return to ? `the link to ${keyDisplayName(cardKey(to))}` : "the link";
  }
  return `the Flow label of ${keyDisplayName(target.label.key)}`;
}

/** The reference a reconnect or accept repair is about, with its project: the item's key, or its target's own key. */
function itemReference(item: HealthItem, page: Page): { readonly key: Key; readonly project: ProjectName | undefined } | null {
  if (item.key !== undefined) {
    return { key: item.key, project: item.project };
  }
  if ("card" in item.target) {
    const card = findCard(page, item.target.card);
    return card ? { key: cardKey(card), project: card.ref.project } : null;
  }
  return "label" in item.target ? { key: item.target.label.key, project: item.target.label.project } : null;
}

const REPAIR_LABELS: Readonly<Record<RepairKind, string>> = {
  reconnect: "Reconnect…",
  removeCard: "Remove the card",
  removeLabel: "Remove the label",
  ungroup: "Ungroup",
  removeLink: "Remove the link",
  removeDescription: "Remove the description",
  resign: "Keep the text",
  accept: "Accept the current code",
};

export function repairLabel(kind: RepairKind): string {
  return REPAIR_LABELS[kind];
}

/** What a repair will do, in a sentence, shown before it is applied (`page` is the item's page). */
export function repairEffect(kind: RepairKind, item: HealthItem, page: Page): string {
  const name = targetName(item.target, page);
  switch (kind) {
    case "reconnect":
      return `Points ${name} at a definition you choose, keeping its description, labels, and groups.`;
    case "removeCard":
      return `Removes the card ${name} from the page, with the groups inside it and its links.`;
    case "removeLabel":
      return `Removes ${name}; the node shows its name from the code again.`;
    case "ungroup":
      return `Removes the group "${name}". Its statements stay where they are.`;
    case "removeLink":
      return `Removes ${name}. Both cards stay on the page.`;
    case "removeDescription":
      return `Removes the description of ${name}. The ${"group" in item.target ? "group" : "card"} stays.`;
    case "resign":
      return `Keeps the text of ${name} as written and signs it against the current code, so it is no longer outdated.`;
    case "accept":
      return `Accepts the current code for ${name}: its text stays attached and a fresh guard is saved.`;
  }
}

function named(project: ProjectName | undefined): { readonly project?: ProjectName } {
  return project !== undefined ? { project } : {};
}

/** The `repair_board` request for one of an item's repairs (`to` is the reconnect target). */
export function repairSpec(kind: RepairKind, item: HealthItem, page: Page, to?: Key): RepairSpec | null {
  const { target } = item;
  const at = { page: item.page };
  switch (kind) {
    case "reconnect": {
      const from = itemReference(item, page);
      return from !== null && to !== undefined && to.length > 0 ? { ...at, kind: "reconnect", ...named(from.project), from: from.key, to } : null;
    }
    case "removeCard":
      return "card" in target ? { ...at, kind: "remove_card", card: target.card } : null;
    case "removeLabel": {
      const label = "label" in target ? target.label : item.key !== undefined ? { key: item.key, project: item.project } : null;
      return label ? { ...at, kind: "remove_label", ...named(label.project), key: label.key } : null;
    }
    case "ungroup":
      return "group" in target ? { ...at, kind: "ungroup", group: target.group } : null;
    case "removeLink":
      return "link" in target ? { ...at, kind: "remove_link", link: target.link } : null;
    case "removeDescription":
      return { ...at, kind: "remove_description", target };
    case "resign":
      return { ...at, kind: "resign", target };
    case "accept": {
      const reference = itemReference(item, page);
      return reference !== null ? { ...at, kind: "accept", ...named(reference.project), key: reference.key } : null;
    }
  }
}
