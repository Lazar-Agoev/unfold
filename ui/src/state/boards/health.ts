/**
 * Board health in the UI: which item is about what on which page, what each offered
 * repair does in words (shown before it is applied), and the `repair_board` request for it.
 */

import type { Health, HealthItem, HealthKind, Key, Page, RepairKind, RepairSpec, Target } from "../../ipc/generated";
import { cardKey, findCard, keyDisplayName } from "./document";
import type { PageId } from "./types";

/** The items about one target (a card, group, label, link, resource card, or frame) of a page, in the engine's order. */
export function itemsFor(health: Health | undefined, page: PageId, target: Target): HealthItem[] {
  return (health?.items ?? []).filter((item) => item.page === page && sameTarget(item.target, target));
}

function sameTarget(a: Target, b: Target): boolean {
  if ("card" in a) return "card" in b && a.card === b.card;
  if ("group" in a) return "group" in b && a.group === b.group;
  if ("link" in a) return "link" in b && a.link === b.link;
  if ("resource" in a) return "resource" in b && a.resource === b.resource;
  if ("frame" in a) return "frame" in b && a.frame === b.frame;
  if ("proposal" in a) return "proposal" in b && a.proposal === b.proposal;
  return "label" in b && a.label === b.label;
}

const KIND_WORDS: Readonly<Record<HealthKind, string>> = {
  missing: "Missing",
  outdated: "Outdated",
  brokenGroup: "Broken group",
  review: "Needs review",
  brokenLink: "Broken link",
};

export function kindWord(kind: HealthKind): string {
  return KIND_WORDS[kind];
}

/** What an item is about, in words: the card's name, the group's title, the labeled node, the link, the resource card, or the frame. */
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
    const card = link ? findCard(page, link.to) : undefined;
    const to = card ? keyDisplayName(cardKey(card)) : link ? ((page.resources ?? []).find((item) => item.id === link.to)?.name ?? (page.frames ?? []).find((item) => item.id === link.to)?.title) : undefined;
    const what = link?.kind !== undefined ? `the ${link.kind} connection` : "the link";
    return to !== undefined ? `${what} to ${to}` : what;
  }
  if ("resource" in target) {
    const resource = (page.resources ?? []).find((item) => item.id === target.resource);
    return resource ? `the ${resource.kind} "${resource.name}"` : "the resource card";
  }
  if ("frame" in target) {
    const frame = (page.frames ?? []).find((item) => item.id === target.frame);
    return frame ? `the frame "${frame.title}"` : "the frame";
  }
  if ("proposal" in target) {
    const proposal = (page.proposals ?? []).find((item) => item.id === target.proposal);
    return proposal ? `the proposed step "${proposal.title}"` : "the proposed step";
  }
  return `the Flow label of ${keyDisplayName(target.label)}`;
}

/** The kind of item a target is, in words, for "The … stays". */
function targetNoun(target: Target): string {
  if ("group" in target) return "group";
  if ("link" in target) return "link";
  if ("resource" in target) return "resource card";
  if ("frame" in target) return "frame";
  if ("proposal" in target) return "proposed step";
  return "card";
}

/** The reference a reconnect or accept repair is about: the item's key, or its target's own key. */
function itemReference(item: HealthItem, page: Page): Key | null {
  if (item.key !== undefined) {
    return item.key;
  }
  if ("card" in item.target) {
    const card = findCard(page, item.target.card);
    return card ? cardKey(card) : null;
  }
  return "label" in item.target ? item.target.label : null;
}

const REPAIR_LABELS: Readonly<Record<RepairKind, string>> = {
  reconnect: "Reconnect…",
  removeCard: "Remove the card",
  removeLabel: "Remove the label",
  ungroup: "Ungroup",
  removeLink: "Remove the link",
  removeResource: "Remove the resource card",
  removeDescription: "Remove the description",
  resign: "Keep the text",
  accept: "Accept the current code",
  removeProposal: "Remove the proposed step",
};

export function repairLabel(kind: RepairKind): string {
  return REPAIR_LABELS[kind];
}

/** What a repair will do, in a sentence, shown before it is applied (`page` is the item's page). */
export function repairEffect(kind: RepairKind, item: HealthItem, page: Page): string {
  const name = targetName(item.target, page);
  switch (kind) {
    case "reconnect":
      return "proposal" in item.target
        ? `Moves ${name} to go after a step of its card you choose. Its title, reason, and author stay; nothing is matched by name.`
        : `Points ${name} at a definition you choose, keeping its description, labels, and groups.`;
    case "removeCard":
      return `Removes the card ${name} from the page, with the groups inside it and its links.`;
    case "removeLabel":
      return `Removes ${name}; the node shows its name from the code again.`;
    case "ungroup":
      return `Removes the group "${name}". Its statements stay where they are.`;
    case "removeLink":
      return `Removes ${name}. What it joins stays on the page.`;
    case "removeResource":
      return `Removes ${name} from the page, with the links to it.`;
    case "removeDescription":
      return `Removes the description of ${name}. The ${targetNoun(item.target)} stays.`;
    case "resign":
      return `Keeps the text of ${name} as written and signs it against the current code, so it is no longer outdated.`;
    case "accept":
      return "proposal" in item.target
        ? `Accepts the current code of the step ${name} goes after: it stays there and a fresh guard is saved.`
        : `Accepts the current code for ${name}: its text stays attached and a fresh guard is saved.`;
    case "removeProposal":
      return `Removes ${name}. The code it goes after stays as it is.`;
  }
}

/** The `repair_board` request for one of an item's repairs (`to` is the reconnect target). */
export function repairSpec(kind: RepairKind, item: HealthItem, page: Page, to?: Key): RepairSpec | null {
  const { target } = item;
  const at = { page: item.page };
  switch (kind) {
    case "reconnect": {
      if (to === undefined || to.length === 0) {
        return null;
      }
      const from = itemReference(item, page);
      if (from !== null) {
        return { ...at, kind: "reconnect", from, to };
      }
      // An evidence range has no key: its claim's anchor is pointed at the key chosen, by position.
      const claim = "resource" in target ? target.resource : "link" in target ? target.link : undefined;
      return claim !== undefined && item.anchor !== undefined ? { ...at, kind: "reconnect_anchor", item: claim, anchor: item.anchor, to: { key: to } } : null;
    }
    case "removeCard":
      return "card" in target ? { ...at, kind: "remove_card", card: target.card } : null;
    case "removeLabel": {
      const key = "label" in target ? target.label : item.key;
      return key !== undefined ? { ...at, kind: "remove_label", key } : null;
    }
    case "ungroup":
      return "group" in target ? { ...at, kind: "ungroup", group: target.group } : null;
    case "removeLink":
      return "link" in target ? { ...at, kind: "remove_link", link: target.link } : null;
    case "removeResource":
      return "resource" in target ? { ...at, kind: "remove_resource", resource: target.resource } : null;
    case "removeDescription":
      return { ...at, kind: "remove_description", target };
    case "resign":
      return { ...at, kind: "resign", target };
    case "accept": {
      const reference = itemReference(item, page);
      return reference !== null ? { ...at, kind: "accept", key: reference } : null;
    }
    case "removeProposal":
      return "proposal" in target ? { ...at, kind: "remove_proposal", proposal: target.proposal } : null;
  }
}
