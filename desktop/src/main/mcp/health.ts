/**
 * Health items as agents meet them (design §4.11, §6.8, §7.7): what an item is about, in words and by
 * the id `repair` takes; the repairs it offers, by the names `repair` takes; the `repair_board`
 * request for one of them; and, for D6, who wrote what re-signing or accepting it vouches for: an
 * agent may re-sign or accept only what agents wrote.
 */

import type { Anchor, AnchorSpec, HealthItem, Page, RepairKind, RepairSpec, Target } from "./engine-types";
import { cardKey } from "./page-code";
import type { ToolContext } from "./context";
import { cant, ToolError } from "./results";

/** The repairs `repair` applies, by the names it takes. */
export const REPAIRS = ["reconnect", "remove", "remove_description", "resign", "accept"] as const;

export type RepairName = (typeof REPAIRS)[number];

const NAMES: Readonly<Record<RepairKind, RepairName>> = {
  reconnect: "reconnect",
  remove: "remove",
  removeDescription: "remove_description",
  resign: "resign",
  accept: "accept",
};

/** The repairs a health item offers, by the names `repair` takes. */
export function repairNames(item: HealthItem): RepairName[] {
  return item.repairs.map((kind) => NAMES[kind]);
}

/** The id `repair` takes for what an item is about: a card's, combined step's, connection's, resource card's, frame's, or proposed step's id, or a name's block key. */
export function targetId(target: Target): string {
  if ("card" in target) {
    return target.card;
  }
  if ("combined" in target) {
    return target.combined;
  }
  if ("name" in target) {
    return target.name;
  }
  if ("connection" in target) {
    return target.connection;
  }
  if ("proposal" in target) {
    return target.proposal;
  }
  return "resource" in target ? target.resource : target.frame;
}

/** What an item is about, in words: `the card createOrder`, `the name “Check the order”`, `the table “orders”`. */
export function targetText(page: Page, target: Target): string {
  if ("card" in target) {
    return `the card ${target.card}`;
  }
  if ("combined" in target) {
    const combined = page.combined.find((item) => item.id === target.combined);
    return combined ? `the combined step “${combined.title}”` : `the combined step ${target.combined}`;
  }
  if ("name" in target) {
    const name = page.names.find((item) => item.key === target.name);
    return name ? `the name “${name.text}” on ${target.name}` : `the name on ${target.name}`;
  }
  if ("connection" in target) {
    const link = page.links.find((item) => item.id === target.connection);
    return link ? `the ${link.kind} connection ${link.from} → ${link.to}` : `the connection ${target.connection}`;
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

/** A text someone wrote: what it is, and its author. */
export interface Words {
  readonly what: string;
  readonly author: string | undefined;
}

type Card = Page["cards"][number];

function describe(what: string, description: { readonly author: string } | undefined): Words[] {
  return description ? [{ what: `the description of ${what}`, author: description.author }] : [];
}

function combinedWords(combined: Page["combined"][number]): Words[] {
  return [{ what: `the combined step “${combined.title}”`, author: combined.author }, ...describe(`“${combined.title}”`, combined.description)];
}

function nameWords(name: Page["names"][number]): Words[] {
  return [{ what: `the name “${name.text}”`, author: name.author }];
}

function linkWords(link: Page["links"][number]): Words[] {
  return [
    { what: `the ${link.kind} connection`, author: link.author },
    ...(link.label ? [{ what: `the label “${link.label.text}”`, author: link.label.author }] : []),
    ...describe("the connection", link.description),
  ];
}

function resourceWords(resource: NonNullable<Page["resources"]>[number]): Words[] {
  return [{ what: `the ${resource.kind} “${resource.name}”`, author: resource.author }, ...describe(`“${resource.name}”`, resource.description)];
}

/**
 * A card's words: its description, and the combined steps and names on `blocks` (the blocks its
 * definition owns, when accepting its code vouches for them; none when re-signing).
 */
function cardWords(page: Page, card: Card, blocks: ReadonlySet<string>): Words[] {
  const combined = page.combined.filter((entry) => entry.members.some((key) => blocks.has(key))).flatMap(combinedWords);
  const names = page.names.filter((name) => blocks.has(name.key)).flatMap(nameWords);
  return [...describe(`the card ${card.id}`, card.description), ...combined, ...names];
}

/** The texts re-signing or accepting an item's target vouches for, with their authors. */
function targetWords(page: Page, target: Target, blocks: ReadonlySet<string>): Words[] {
  if ("card" in target) {
    const card = page.cards.find((entry) => entry.id === target.card);
    return card ? cardWords(page, card, blocks) : [];
  }
  if ("combined" in target) {
    const combined = page.combined.find((entry) => entry.id === target.combined);
    return combined ? combinedWords(combined) : [];
  }
  if ("name" in target) {
    const name = page.names.find((entry) => entry.key === target.name);
    return name ? nameWords(name) : [];
  }
  if ("connection" in target) {
    const link = page.links.find((entry) => entry.id === target.connection);
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
 * accepting it for one item accepts it for every card, combined step, name, resource card, and
 * connection that references the key.
 */
function heldWords(page: Page, key: string, blocks: ReadonlySet<string>): Words[] {
  const cites = (anchors: readonly Anchor[] | undefined) => (anchors ?? []).some((anchor) => "key" in anchor && anchor.key === key);
  return [
    ...page.cards.filter((card) => cardKey(card) === key).flatMap((card) => cardWords(page, card, blocks)),
    ...page.combined.filter((combined) => combined.members.includes(key)).flatMap(combinedWords),
    ...page.names.filter((name) => name.key === key).flatMap(nameWords),
    ...(page.resources ?? []).filter((resource) => resource.type === key || cites(resource.anchors)).flatMap(resourceWords),
    ...page.links.filter((link) => link.step === key || link.carries === key || cites(link.anchors)).flatMap(linkWords),
  ];
}

/**
 * The blocks whose names and combined steps accepting an item's code vouches for: those its
 * definition owns (by their keys, from `code_text`), when the repair is an accept of a definition's
 * code; none otherwise.
 */
export async function acceptedBlocks(context: ToolContext, project: string, page: Page, item: HealthItem, repair: "resign" | "accept"): Promise<ReadonlySet<string>> {
  const key = repair === "accept" ? reference(page, item) : null;
  if (key === null) {
    return new Set();
  }
  try {
    return new Set((await context.engine("code_text", { project, key })).labels.map((label) => label.key));
  } catch (error) {
    if (error instanceof ToolError) {
      return new Set();
    }
    throw error;
  }
}

/**
 * A person's words among what re-signing or accepting an item vouches for (D6), or null when only
 * agents wrote it. `blocks` are the blocks accepting it vouches for ({@link acceptedBlocks}).
 */
export function personWords(page: Page, item: HealthItem, repair: "resign" | "accept", blocks: ReadonlySet<string>): Words | null {
  const accepted = repair === "accept" ? reference(page, item) : null;
  const words = [...targetWords(page, item.target, blocks), ...(accepted !== null ? heldWords(page, accepted, blocks) : [])];
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
  return "name" in target ? target.name : null;
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
    case "remove":
      return { ...at, kind: "remove", target };
    case "remove_description":
      return { ...at, kind: "remove_description", target };
    case "resign":
      return { ...at, kind: "resign", target };
  }
  throw cant(`${repair.replaceAll("_", " ")} ${what}`, "that repair doesn't apply to it", `Its problems offer: ${repairNames(item).join(", ")}.`);
}
