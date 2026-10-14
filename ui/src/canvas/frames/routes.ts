/**
 * Arrows and connections at collapsed frames: an end inside a collapsed frame attaches
 * to the frame instead, an arrow or connection with both ends inside one isn't drawn, and the ones that
 * would then draw on top of each other merge into one (a merged connection counts them), as a compact
 * card's calls to one card merge. A line that only moved an end keeps its id and contract. Pure.
 */

import type { Connection, ConnectionKind } from "../../ipc/generated";
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
    const { cards: _cards, ...rest } = first;
    return [
      {
        ...rest,
        id: `${first.source}|${first.sourceHandle}|${first.target}`,
        dashed: group.every((arrow) => arrow.dashed),
        calls: [],
        merged: true,
      },
    ];
  });
}

/** A connection between what is drawn: one of the page's connections, or several of a kind merged (`count`). */
export interface RoutedLink {
  /** The connection's id; a merged one's is `<from>|<kind>|<to>`. */
  readonly id: string;
  readonly from: string;
  readonly to: string;
  /** The connection, when it stands for one. */
  readonly link?: Connection;
  readonly count: number;
  readonly kind: ConnectionKind;
}

/**
 * The page's connections between what is drawn. Connections of one kind that a collapsed frame makes
 * draw on top of each other merge into one. A connection whose ends are both drawn as themselves stays
 * itself (with its own step, columns, and label), even beside another between the same two items.
 */
export function routeLinks(links: readonly Connection[], shownAs: ReadonlyMap<string, string>): RoutedLink[] {
  const moved = links.map((link) => ({ link, from: shownAs.get(link.from) ?? link.from, to: shownAs.get(link.to) ?? link.to }));
  const key = ({ link, from, to }: (typeof moved)[number]) => (from === to ? null : from === link.from && to === link.to ? link.id : `${from}|${link.kind}|${to}`);
  return grouped(moved, key).flatMap((group): RoutedLink[] => {
    const [first] = group;
    if (!first) {
      return [];
    }
    const { from, to } = first;
    const { kind } = first.link;
    if (group.length === 1) {
      return [{ id: first.link.id, from, to, link: first.link, count: 1, kind }];
    }
    return [{ id: `${from}|${kind}|${to}`, from, to, count: group.length, kind }];
  });
}
