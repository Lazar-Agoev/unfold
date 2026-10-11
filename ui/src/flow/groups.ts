/**
 * A card's step groups as Flow draws them (reader.md "Groups"): the board's groups, each with whether a
 * statement inside can return or throw. That comes from the Code model's outline, as the reader's fold
 * summary computes it (`returns` and `throws`, outside nested callables); a member without a fold (a
 * one-line statement) is read from its own kind and the statements written in it.
 */

import type { BlockKind, Group, OutlineItem } from "../ipc/generated";
import { outlineChildren } from "../code/outline";
import type { StepGroup } from "./layout";

/** Callables: what they return or throw is their own, not their enclosing statement's. */
const CALLABLES: ReadonlySet<BlockKind> = new Set(["function", "method", "constructor", "getter", "setter"]);

interface Exits {
  readonly returns: boolean;
  readonly throws: boolean;
}

/** Whether the outline item at `index` contains a return or a throw outside nested callables. */
function exitsOf(outline: readonly OutlineItem[], children: ReadonlyMap<number, readonly number[]>, index: number): Exits {
  const item = outline[index];
  if (!item) {
    return { returns: false, throws: false };
  }
  if (item.fold) {
    return { returns: item.fold.returns, throws: item.fold.throws };
  }
  let returns = item.kind === "return";
  let throws = item.kind === "throw";
  for (const child of children.get(index) ?? []) {
    const kind = outline[child]?.kind;
    if (kind !== undefined && !CALLABLES.has(kind)) {
      const inner = exitsOf(outline, children, child);
      returns ||= inner.returns;
      throws ||= inner.throws;
    }
  }
  return { returns, throws };
}

/** The step groups of one card, from its groups (unbroken ones) and its Code model's outline. */
export function stepGroups(groups: readonly Group[], outline: readonly OutlineItem[]): StepGroup[] {
  const children = outlineChildren(outline);
  const indexOf = new Map(outline.map((item, index) => [item.key, index]));
  return groups.map((group) => {
    let returns = false;
    let throws = false;
    for (const member of group.members) {
      const index = indexOf.get(member);
      if (index !== undefined) {
        const exits = exitsOf(outline, children, index);
        returns ||= exits.returns;
        throws ||= exits.throws;
      }
    }
    return { id: group.id, title: group.title, members: group.members, collapsed: group.collapsed, returns, throws };
  });
}
