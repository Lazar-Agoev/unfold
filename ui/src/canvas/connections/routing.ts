/**
 * Where a connection leaves and enters: from its step's call on an expanded card that
 * draws it (a `stepPort` the card places level with the call, as arrows leave from their calls), from
 * the card's edge otherwise (a compact card, a resource card, a frame); a relationship from one table
 * column's row to the other's. An end inside a collapsed frame attaches to the frame's edge. Otherwise
 * it leaves and enters by the sides facing each other (`linkEnds`). Pure.
 */

import type { Connection, Key } from "../../ipc/generated";
import { stepPort, type CardPort } from "../arrows";
import type { Rect } from "../layout";
import { linkEnds } from "../linkSides";
import { columnHandle } from "../resources/columns";

/** A line's two handles. */
export interface Handles {
  readonly source: string;
  readonly target: string;
}

/** The ends a connection is drawn between (`routeLinks`): each its own item, or the collapsed frame shown in its place. */
interface DrawnEnds {
  readonly from: string;
  readonly to: string;
}

/** Whether card `cardId` draws block `step` where a connection can leave from it (expanded, in its drawn flow). */
export type DrawsStep = (cardId: string, step: Key) => boolean;

/**
 * The handles of a line drawn between `ends` (rectangles `from` and `to`): a connection's step port or
 * columns where its ends are drawn as themselves, else the facing sides.
 */
export function lineHandles(link: Connection | undefined, ends: DrawnEnds, from: Rect, to: Rect, drawsStep: DrawsStep): Handles {
  const sides = linkEnds(from, to);
  const own = { from: link?.from === ends.from, to: link?.to === ends.to };
  let source = `link-out-${sides.from}`;
  let target = `link-in-${sides.to}`;
  if (link?.kind === "relates") {
    const rightward = to.x + to.width / 2 >= from.x + from.width / 2;
    if (own.from && link.fromColumn !== undefined) {
      source = columnHandle(link.fromColumn, "out", rightward ? "right" : "left");
    }
    if (own.to && link.toColumn !== undefined) {
      target = columnHandle(link.toColumn, "in", rightward ? "left" : "right");
    }
  } else if (link?.step !== undefined && own.from && drawsStep(link.from, link.step)) {
    source = stepPort(link.step);
  }
  return { source, target };
}

/**
 * The step ports each card shows a connection leaving from (drawn as itself, expanded, drawing the
 * step), each with its connection's look (`selected`: the selected connection's id).
 */
export function stepPorts(links: readonly Connection[], shownAs: ReadonlyMap<string, string>, drawsStep: DrawsStep, selected: string | null = null): Map<string, CardPort[]> {
  const ports = new Map<string, CardPort[]>();
  for (const link of links) {
    if (link.kind !== "relates" && link.step !== undefined && !shownAs.has(link.from) && drawsStep(link.from, link.step)) {
      const port = stepPort(link.step);
      const list = ports.get(link.from) ?? [];
      const known = list.findIndex((item) => item.id === port);
      if (known < 0) {
        list.push({ id: port, line: link.kind, dashed: false, selected: link.id === selected });
      } else if (link.id === selected) {
        list[known] = { id: port, line: link.kind, dashed: false, selected: true };
      }
      ports.set(link.from, list);
    }
  }
  return ports;
}

/** The columns each table shows a relationship leaving or entering by (drawn as itself). */
export function relatedColumns(links: readonly Connection[], shownAs: ReadonlyMap<string, string>): Map<string, string[]> {
  const columns = new Map<string, string[]>();
  const add = (table: string, column: string | undefined) => {
    if (column === undefined || shownAs.has(table)) {
      return;
    }
    const list = columns.get(table) ?? [];
    if (!list.includes(column)) {
      list.push(column);
    }
    columns.set(table, list);
  };
  for (const link of links) {
    if (link.kind === "relates") {
      add(link.from, link.fromColumn);
      add(link.to, link.toColumn);
    }
  }
  return columns;
}
