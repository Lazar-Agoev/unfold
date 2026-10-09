/**
 * Board health in the UI (architecture §10.5, reader.md "Health and repairs"): which item is about
 * what, what each offered repair does in words (shown before it is applied), and the `repair_board`
 * request for it.
 */

import type { BoardDocument, Health, HealthItem, HealthKind, RepairKind, RepairSpec, Target } from "../../ipc/generated";
import { cardKey, keyDisplayName } from "./document";

/** The items about one target (a card, group, or label), in the engine's order. */
export function itemsFor(health: Health | undefined, target: Target): HealthItem[] {
  return (health?.items ?? []).filter((item) => sameTarget(item.target, target));
}

export function sameTarget(a: Target, b: Target): boolean {
  if ("card" in a) return "card" in b && a.card === b.card;
  if ("group" in a) return "group" in b && a.group === b.group;
  return "label" in b && a.label === b.label;
}

const KIND_WORDS: Readonly<Record<HealthKind, string>> = {
  missing: "Missing",
  outdated: "Outdated",
  brokenGroup: "Broken group",
  review: "Needs review",
};

export function kindWord(kind: HealthKind): string {
  return KIND_WORDS[kind];
}

/** What an item is about, in words: the card's name, the group's title, or the labeled node. */
export function targetName(target: Target, document: BoardDocument): string {
  if ("card" in target) {
    const card = document.cards.find((item) => item.id === target.card);
    return card ? keyDisplayName(cardKey(card)) : target.card;
  }
  if ("group" in target) {
    return document.groups.find((item) => item.id === target.group)?.title ?? target.group;
  }
  return `the Flow label of ${keyDisplayName(target.label)}`;
}

/** The reference a reconnect or accept repair is about: the item's key, or its target's own key. */
export function itemReference(item: HealthItem, document: BoardDocument): string | null {
  if (item.key !== undefined) {
    return item.key;
  }
  if ("card" in item.target) {
    const id = item.target.card;
    const card = document.cards.find((entry) => entry.id === id);
    return card ? cardKey(card) : null;
  }
  return "label" in item.target ? item.target.label : null;
}

const REPAIR_LABELS: Readonly<Record<RepairKind, string>> = {
  reconnect: "Reconnect…",
  removeCard: "Remove the card",
  removeLabel: "Remove the label",
  ungroup: "Ungroup",
  removeDescription: "Remove the description",
  resign: "Keep the text",
  accept: "Accept the current code",
};

export function repairLabel(kind: RepairKind): string {
  return REPAIR_LABELS[kind];
}

/** What a repair will do, in a sentence, shown before it is applied. */
export function repairEffect(kind: RepairKind, item: HealthItem, document: BoardDocument): string {
  const name = targetName(item.target, document);
  switch (kind) {
    case "reconnect":
      return `Points ${name} at a definition you choose, keeping its description, labels, and groups.`;
    case "removeCard":
      return `Removes the card ${name} from the board, with the groups inside it.`;
    case "removeLabel":
      return `Removes ${name}; the node shows its name from the code again.`;
    case "ungroup":
      return `Removes the group "${name}". Its statements stay where they are.`;
    case "removeDescription":
      return `Removes the description of ${name}. The ${"group" in item.target ? "group" : "card"} stays.`;
    case "resign":
      return `Keeps the text of ${name} as written and signs it against the current code, so it is no longer outdated.`;
    case "accept":
      return `Accepts the current code for ${name}: its text stays attached and a fresh guard is saved.`;
  }
}

/** The `repair_board` request for one of an item's repairs (`to` is the reconnect target). */
export function repairSpec(kind: RepairKind, item: HealthItem, document: BoardDocument, to?: string): RepairSpec | null {
  const { target } = item;
  switch (kind) {
    case "reconnect": {
      const from = itemReference(item, document);
      return from !== null && to !== undefined && to.length > 0 ? { kind: "reconnect", from, to } : null;
    }
    case "removeCard":
      return "card" in target ? { kind: "remove_card", card: target.card } : null;
    case "removeLabel": {
      const key = "label" in target ? target.label : item.key;
      return key !== undefined ? { kind: "remove_label", key } : null;
    }
    case "ungroup":
      return "group" in target ? { kind: "ungroup", group: target.group } : null;
    case "removeDescription":
      return { kind: "remove_description", target };
    case "resign":
      return { kind: "resign", target };
    case "accept": {
      const key = itemReference(item, document);
      return key !== null ? { kind: "accept", key } : null;
    }
  }
}
