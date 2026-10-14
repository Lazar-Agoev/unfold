/**
 * Board health in the UI: which item is about what on which page, what each offered
 * repair does in words (shown before it is applied), and the `repair_board` request for it.
 */

import type { Health, HealthItem, HealthKind, Key, Page, RepairKind, RepairSpec, Target } from "../../ipc/generated";
import { cardKey, findCard, findCombined } from "./document";
import type { PageId } from "./types";

/** The items about one target (a card, combined step, name, connection, resource card, frame, or proposed step) of a page, in the engine's order. */
export function itemsFor(health: Health | undefined, page: PageId, target: Target): HealthItem[] {
  return (health?.items ?? []).filter((item) => item.page === page && sameTarget(item.target, target));
}

function sameTarget(a: Target, b: Target): boolean {
  if ("card" in a) return "card" in b && a.card === b.card;
  if ("combined" in a) return "combined" in b && a.combined === b.combined;
  if ("name" in a) return "name" in b && a.name === b.name;
  if ("connection" in a) return "connection" in b && a.connection === b.connection;
  if ("resource" in a) return "resource" in b && a.resource === b.resource;
  if ("frame" in a) return "frame" in b && a.frame === b.frame;
  return "proposal" in b && a.proposal === b.proposal;
}

const KIND_WORDS: Readonly<Record<HealthKind, string>> = {
  missing: "Missing",
  outdated: "Outdated",
  brokenCombined: "Broken combined step",
  review: "Needs review",
  brokenLink: "Broken connection",
};

export function kindWord(kind: HealthKind): string {
  return KIND_WORDS[kind];
}

/** What a target is called on the page: an item's own words where it has them. */
function targetName(target: Target, page: Page): string {
  if ("card" in target) {
    return "the card";
  }
  if ("combined" in target) {
    const combined = findCombined(page, target.combined);
    return combined ? `the combined step "${combined.title}"` : "the combined step";
  }
  if ("name" in target) {
    const name = page.names.find((item) => item.key === target.name);
    return name ? `the name "${name.text}"` : "the name";
  }
  if ("connection" in target) {
    const link = page.links.find((item) => item.id === target.connection);
    const to = link ? ((page.resources ?? []).find((item) => item.id === link.to)?.name ?? (page.frames ?? []).find((item) => item.id === link.to)?.title) : undefined;
    const what = link ? `the ${link.kind} connection` : "the connection";
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
  const proposal = (page.proposals ?? []).find((item) => item.id === target.proposal);
  return proposal ? `the proposed step "${proposal.title}"` : "the proposed step";
}

/** The kind of item a target is, in words, for "The … stays". */
function targetNoun(target: Target): string {
  if ("combined" in target) return "combined step";
  if ("name" in target) return "name";
  if ("connection" in target) return "connection";
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
  return "name" in item.target ? item.target.name : null;
}

/**
 * What a repair is called (core-scope.html's A CARD TO REVIEW). Re-signing text written for changed
 * code is Accept too: no problem offers both, and the tooltip (`repairEffect`) says which it does.
 */
const REPAIR_LABELS: Readonly<Record<RepairKind, string>> = {
  reconnect: "Reconnect…",
  resign: "Accept",
  accept: "Accept",
  remove: "Remove from page",
  removeDescription: "Remove the description",
};

export function repairLabel(kind: RepairKind): string {
  return REPAIR_LABELS[kind];
}

/** What removing a target takes with it, in words. */
function removalEffect(target: Target, name: string): string {
  if ("card" in target) return `Removes ${name} from the page, with its connections.`;
  if ("combined" in target) return `Removes ${name}. Its blocks are drawn one by one again.`;
  if ("name" in target) return `Removes ${name}; the block shows its name from the code again.`;
  if ("connection" in target) return `Removes ${name}. What it joins stays on the page.`;
  if ("resource" in target) return `Removes ${name} from the page, with its connections.`;
  if ("frame" in target) return `Removes ${name}. What it holds stays on the page.`;
  return `Removes ${name}. The code it goes with stays as it is.`;
}

/** What a repair will do, in a sentence, shown before it is applied (`page` is the item's page). */
export function repairEffect(kind: RepairKind, item: HealthItem, page: Page): string {
  const name = targetName(item.target, page);
  switch (kind) {
    case "reconnect":
      return "proposal" in item.target
        ? `Moves ${name} to a block you choose. Its title, reason, and author stay; nothing is matched by name.`
        : `Points ${name} at a block you choose, keeping what was written about it.`;
    case "remove":
      return removalEffect(item.target, name);
    case "removeDescription":
      return `Removes the description of ${name}. The ${targetNoun(item.target)} stays.`;
    case "resign":
      return `Keeps the text of ${name} as written and signs it against the current code, so it is no longer outdated.`;
    case "accept":
      return `Accepts the current code for ${name}: what was written stays attached and a fresh guard is saved.`;
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
      const claim = "resource" in target ? target.resource : "connection" in target ? target.connection : undefined;
      return claim !== undefined && item.anchor !== undefined ? { ...at, kind: "reconnect_anchor", item: claim, anchor: item.anchor, to: { key: to } } : null;
    }
    case "remove":
      return { ...at, kind: "remove", target };
    case "removeDescription":
      return { ...at, kind: "remove_description", target };
    case "resign":
      return { ...at, kind: "resign", target };
    case "accept": {
      const reference = itemReference(item, page);
      return reference !== null ? { ...at, kind: "accept", key: reference } : null;
    }
  }
}
