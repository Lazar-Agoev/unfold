/**
 * Health items as agents meet them (design §4.11, §6.8, §7.7): what an item is about, in words and by
 * the id `repair` takes; the repairs it offers, by the names `repair` takes; the `repair_board`
 * request for one of them; and, for D6, who wrote what re-signing or accepting it vouches for: an
 * agent may re-sign or accept only what agents wrote.
 */

import type { Anchor, AnchorSpec, HealthItem, Page, RepairKind, RepairSpec, Target } from "./engine-types";
import { keyName } from "./format";
import { cardKey } from "./page-code";
import { cant } from "./results";

/** The repairs `repair` applies, by the names it takes. */
export const REPAIRS = ["reconnect", "remove_card", "remove_label", "ungroup", "remove_link", "remove_resource", "remove_description", "resign", "accept", "remove_proposal"] as const;

export type RepairName = (typeof REPAIRS)[number];

const NAMES: Readonly<Record<RepairKind, RepairName>> = {
  reconnect: "reconnect",
  removeCard: "remove_card",
  removeLabel: "remove_label",
  ungroup: "ungroup",
  removeLink: "remove_link",
  removeResource: "remove_resource",
  removeDescription: "remove_description",
  resign: "resign",
  accept: "accept",
  removeProposal: "remove_proposal",
};

/** The repairs a health item offers, by the names `repair` takes. */
export function repairNames(item: HealthItem): RepairName[] {
  return item.repairs.map((kind) => NAMES[kind]);
}

/** The id `repair` takes for what an item is about: a card's, step group's, link's, resource card's, or frame's id, or a Flow label's step key. */
export function targetId(target: Target): string {
  if ("card" in target) {
    return target.card;
  }
  if ("group" in target) {
    return target.group;
  }
  if ("label" in target) {
    return target.label;
  }
  if ("link" in target) {
    return target.link;
  }
  if ("proposal" in target) {
    return target.proposal;
  }
  return "resource" in target ? target.resource : target.frame;
}

/** What an item is about, in words: `the card createOrder`, `the Flow label of createOrder.if`, `the table “orders”`. */
export function targetText(page: Page, target: Target): string {
  if ("card" in target) {
    const card = page.cards.find((item) => item.id === target.card);
    return card ? `the card ${keyName(cardKey(card))}` : `the card ${target.card}`;
  }
  if ("group" in target) {
    const group = page.groups.find((item) => item.id === target.group);
    return group ? `the step group “${group.title}”` : `the step group ${target.group}`;
  }
  if ("label" in target) {
    return `the Flow label of ${keyName(target.label)}`;
  }
  if ("link" in target) {
    const link = page.links.find((item) => item.id === target.link);
    return link ? `the ${link.kind === undefined ? "drawn link" : `${link.kind} connection`} ${link.from} → ${link.to}` : `the link ${target.link}`;
  }
  if ("resource" in target) {
    const resource = page.resources?.find((item) => item.id === target.resource);
    return resource ? `the ${resource.kind} “${resource.name}”` : `the resource card ${target.resource}`;
  }
  if ("proposal" in target) {
    const proposal = page.proposals?.find((item) => item.id === target.proposal);
    return proposal ? `the proposed step “${proposal.title}”` : `the proposed step ${target.proposal}`;
  }
  const frame = page.frames?.find((item) => item.id === target.frame);
  return frame ? `the frame “${frame.title}”` : `the frame ${target.frame}`;
}

/** A text someone wrote: what it is, and its author (absent: a person, before titles were signed). */
export interface Words {
  readonly what: string;
  readonly author: string | undefined;
}

type Card = Page["cards"][number];

function describe(what: string, description: { readonly author: string } | undefined): Words[] {
  return description ? [{ what: `the description of ${what}`, author: description.author }] : [];
}

function groupWords(group: Page["groups"][number]): Words[] {
  return [{ what: `the step group “${group.title}”`, author: group.author }, ...describe(`“${group.title}”`, group.description)];
}

function labelWords(label: Page["labels"][number]): Words[] {
  return [{ what: `the Flow label “${label.text}”`, author: label.author }];
}

function linkWords(link: Page["links"][number]): Words[] {
  return [
    ...(link.kind !== undefined ? [{ what: `the ${link.kind} connection`, author: link.author }] : []),
    ...(link.label ? [{ what: `the label “${link.label.text}”`, author: link.label.author }] : []),
    ...describe("the link", link.description),
  ];
}

function resourceWords(resource: NonNullable<Page["resources"]>[number]): Words[] {
  return [{ what: `the ${resource.kind} “${resource.name}”`, author: resource.author }, ...describe(`“${resource.name}”`, resource.description)];
}

/** A card's words: its description; accepting its code also keeps its step groups and Flow labels attached. */
function cardWords(page: Page, card: Card, repair: "resign" | "accept"): Words[] {
  const own = describe(keyName(cardKey(card)), card.description);
  if (repair === "resign") {
    return own;
  }
  const key = cardKey(card);
  const groups = page.groups.filter((group) => group.card === card.id).flatMap(groupWords);
  const labels = page.labels.filter((label) => label.key.startsWith(`${key}/`)).flatMap(labelWords);
  return [...own, ...groups, ...labels];
}

/** The texts re-signing or accepting an item's target vouches for, with their authors. */
function targetWords(page: Page, target: Target, repair: "resign" | "accept"): Words[] {
  if ("card" in target) {
    const card = page.cards.find((entry) => entry.id === target.card);
    return card ? cardWords(page, card, repair) : [];
  }
  if ("group" in target) {
    const group = page.groups.find((entry) => entry.id === target.group);
    return group ? groupWords(group) : [];
  }
  if ("label" in target) {
    const label = page.labels.find((entry) => entry.key === target.label);
    return label ? labelWords(label) : [];
  }
  if ("link" in target) {
    const link = page.links.find((entry) => entry.id === target.link);
    return link ? linkWords(link) : [];
  }
  if ("resource" in target) {
    const resource = page.resources?.find((entry) => entry.id === target.resource);
    return resource ? resourceWords(resource) : [];
  }
  if ("proposal" in target) {
    const proposal = page.proposals?.find((entry) => entry.id === target.proposal);
    return proposal ? [{ what: `the proposed step “${proposal.title}”`, author: proposal.author }] : [];
  }
  const frame = page.frames?.find((entry) => entry.id === target.frame);
  return frame ? describe(`the frame “${frame.title}”`, frame.description) : [];
}

/**
 * The texts the page's guard of `key` holds for review: a guard is one per reference on the page, so
 * accepting it for one item accepts it for every card, step group, Flow label, resource card, and
 * connection that references the key.
 */
function heldWords(page: Page, key: string): Words[] {
  const cites = (anchors: readonly Anchor[] | undefined) => (anchors ?? []).some((anchor) => "key" in anchor && anchor.key === key);
  return [
    ...page.cards.filter((card) => cardKey(card) === key).flatMap((card) => cardWords(page, card, "accept")),
    ...page.groups.filter((group) => group.members.includes(key)).flatMap(groupWords),
    ...page.labels.filter((label) => label.key === key).flatMap(labelWords),
    ...(page.resources ?? []).filter((resource) => resource.type === key || cites(resource.anchors)).flatMap(resourceWords),
    ...page.links.filter((link) => link.kind !== undefined && (link.step === key || link.carries === key || cites(link.anchors))).flatMap(linkWords),
  ];
}

/**
 * A person's words among what re-signing or accepting an item vouches for (D6), or null when only
 * agents wrote it. A title written before titles were signed counts as a person's.
 */
export function personWords(page: Page, item: HealthItem, repair: "resign" | "accept"): Words | null {
  const accepted = repair === "accept" ? reference(page, item) : null;
  const words = [...targetWords(page, item.target, repair), ...(accepted !== null ? heldWords(page, accepted) : [])];
  return words.find((entry) => entry.author?.startsWith("agent:") !== true) ?? null;
}

/** The key a reconnect or accept is about: the item's key, or its target's own key. */
function reference(page: Page, item: HealthItem): string | null {
  if (item.key !== undefined) {
    return item.key;
  }
  const { target } = item;
  if ("card" in target) {
    const card = page.cards.find((entry) => entry.id === target.card);
    return card ? cardKey(card) : null;
  }
  return "label" in target ? target.label : null;
}

/**
 * The `repair_board` request for one repair an item offers. `to` is where a reconnect points: a key,
 * or for an anchor, a key or an evidence file's lines.
 */
export function repairSpec(page: Page, item: HealthItem, repair: RepairName, to: string | AnchorSpec | undefined): RepairSpec {
  const at = { page: item.page };
  const { target } = item;
  const what = targetText(page, target);
  switch (repair) {
    case "reconnect": {
      if (to === undefined) {
        throw cant(`reconnect ${what}`, "to is missing", "Pass to: the code's new key (search or get_relations finds it), or for an anchor an evidence file's lines.");
      }
      if (item.anchor !== undefined) {
        return { ...at, kind: "reconnect_anchor", item: targetId(target), anchor: item.anchor, to: typeof to === "string" ? { key: to } : to };
      }
      const from = reference(page, item);
      if (typeof to !== "string" || from === null) {
        throw cant(`reconnect ${what}`, "it points at a key, so to is a key", "Pass to: the code's new key, from search or get_relations.");
      }
      return { ...at, kind: "reconnect", from, to };
    }
    case "accept": {
      const key = reference(page, item);
      if (key !== null) {
        return { ...at, kind: "accept", key };
      }
      break;
    }
    case "remove_label": {
      const key = "label" in target ? target.label : item.key;
      if (key !== undefined) {
        return { ...at, kind: "remove_label", key };
      }
      break;
    }
    case "remove_card":
      if ("card" in target) {
        return { ...at, kind: "remove_card", card: target.card };
      }
      break;
    case "ungroup":
      if ("group" in target) {
        return { ...at, kind: "ungroup", group: target.group };
      }
      break;
    case "remove_link":
      if ("link" in target) {
        return { ...at, kind: "remove_link", link: target.link };
      }
      break;
    case "remove_resource":
      if ("resource" in target) {
        return { ...at, kind: "remove_resource", resource: target.resource };
      }
      break;
    case "remove_description":
      return { ...at, kind: "remove_description", target };
    case "resign":
      return { ...at, kind: "resign", target };
    case "remove_proposal":
      if ("proposal" in target) {
        return { ...at, kind: "remove_proposal", proposal: target.proposal };
      }
      break;
  }
  throw cant(`${repair.replaceAll("_", " ")} ${what}`, "that repair doesn't apply to it", `Its problem offers: ${repairNames(item).join(", ")}.`);
}
