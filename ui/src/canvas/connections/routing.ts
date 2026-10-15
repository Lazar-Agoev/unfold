/**
 * Where a connection's edge attaches: its step's port on an expanded card that draws the step (where
 * the card's line from the call leaves it), a relationship's columns' rows, else the sides the page's
 * router drew it on (`routePageLines`, which also picks those sides). An end inside a collapsed frame
 * attaches to the frame. The edge draws the routed line; its handles only say where it attaches. Pure.
 */

import type { Connection, Key } from "../../ipc/generated";
import { stepPort, type CardPort } from "../arrows";
import type { Side } from "../lines/route";
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
 * The handles of a line drawn between `ends` on `sides` (the router's; until it is routed, right to
 * left): a connection's step port or columns where its ends are drawn as themselves, else the sides.
 */
export function lineHandles(link: Connection | undefined, ends: DrawnEnds, sides: { readonly from: Side; readonly to: Side } | undefined, drawsStep: DrawsStep): Handles {
  const { from, to } = sides ?? { from: "right", to: "left" };
  const own = { from: link?.from === ends.from, to: link?.to === ends.to };
  let source = `link-out-${from}`;
  let target = `link-in-${to}`;
  if (link?.kind === "relates") {
    if (own.from && link.fromColumn !== undefined) {
      source = columnHandle(link.fromColumn, "out", from === "left" ? "left" : "right");
    }
    if (own.to && link.toColumn !== undefined) {
      target = columnHandle(link.toColumn, "in", to === "right" ? "right" : "left");
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
