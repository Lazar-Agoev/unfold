/**
 * Arrows and drawn links at collapsed frames (design §4.4): an end inside a collapsed frame attaches
 * to the frame instead, an arrow or link with both ends inside one isn't drawn, and the ones that would
 * then draw on top of each other merge into one (a merged link counts them), as a Diagram node's calls
 * to one card merge. A line that only moved an end keeps its id and contract. Pure.
 */

import type { Link, LinkKind } from "../../ipc/generated";
import { NODE_PORT, type Arrow } from "../arrows";

/** Groups items by key, in the order each key first appears. */
function grouped<T>(items: readonly T[], key: (item: T) => string | null): T[][] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const id = key(item);
    if (id !== null) {
      groups.set(id, [...(groups.get(id) ?? []), item]);
    }
  }
  return [...groups.values()];
}

/** The page's arrows between what is drawn (`shownAs`: each hidden item and the collapsed frame shown in its place). */
export function routeArrows(arrows: readonly Arrow[], shownAs: ReadonlyMap<string, string>): readonly Arrow[] {
  if (shownAs.size === 0) {
    return arrows;
  }
  const moved = arrows.map((arrow): Arrow => {
    const source = shownAs.get(arrow.source) ?? arrow.source;
    const target = shownAs.get(arrow.target) ?? arrow.target;
    // A collapsed frame's arrows leave from its node's edge.
    return source === arrow.source && target === arrow.target
      ? arrow
      : { ...arrow, source, target, sourceHandle: source === arrow.source ? arrow.sourceHandle : NODE_PORT, cards: { from: arrow.source, to: arrow.target } };
  });
  return grouped(moved, (arrow) => (arrow.source === arrow.target ? null : `${arrow.source}|${arrow.sourceHandle}|${arrow.target}`)).flatMap((group): Arrow[] => {
    const [first] = group;
    if (!first || group.length === 1) {
      return group;
    }
    const { calls: _calls, cards: _cards, ...rest } = first;
    return [
      {
        ...rest,
        id: `${first.source}|${first.sourceHandle}|${first.target}`,
        dashed: group.every((arrow) => arrow.dashed),
        line: null,
        merged: true,
      },
    ];
  });
}

/** A link between what is drawn: one of the page's links, or several merged (`count`). */
export interface RoutedLink {
  /** The link's id; a merged one's is `<from>|links|<to>` (drawn links) or `<from>|<kind>|<to>` (connections). */
  readonly id: string;
  readonly from: string;
  readonly to: string;
  /** The link, when it stands for one. */
  readonly link?: Link;
  readonly count: number;
  /** The kind of the connections a merged one stands for; absent for drawn links. */
  readonly kind?: LinkKind;
}

/**
 * The page's links between what is drawn. Drawn links that a collapsed frame makes draw on top of each
 * other merge into one; connections merge only with connections of their kind. A link whose ends are
 * both drawn as themselves stays itself (with its own step, columns, and label), even beside another
 * between the same two items.
 */
export function routeLinks(links: readonly Link[], shownAs: ReadonlyMap<string, string>): RoutedLink[] {
  const moved = links.map((link) => ({ link, from: shownAs.get(link.from) ?? link.from, to: shownAs.get(link.to) ?? link.to }));
  const key = ({ link, from, to }: (typeof moved)[number]) =>
    from === to ? null : from === link.from && to === link.to ? link.id : `${from}|${link.kind ?? "links"}|${to}`;
  return grouped(moved, key).flatMap((group): RoutedLink[] => {
    const [first] = group;
    if (!first) {
      return [];
    }
    const { from, to } = first;
    const { kind } = first.link;
    if (group.length === 1) {
      return [{ id: first.link.id, from, to, link: first.link, count: 1 }];
    }
    return [{ id: `${from}|${kind ?? "links"}|${to}`, from, to, count: group.length, ...(kind !== undefined ? { kind } : {}) }];
  });
}
