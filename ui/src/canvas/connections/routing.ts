/**
 * Where a connection leaves and enters (design §4.7): from its step's line in a card shown as Code and
 * its step in Flow (a `step:` port the card places, as compiled arrows leave from their calling line),
 * from the card's edge otherwise (a Diagram node, a resource card, a frame); a relationship from one
 * table column's row to the other's. An end inside a collapsed frame attaches to the frame's edge.
 * Drawn links leave and enter by the sides facing each other (`linkEnds`). Pure.
 */

import type { CardBody } from "../../code/typeKinds";
import { stepPort } from "../../code/outline";
import type { Link } from "../../ipc/generated";
import type { Rect } from "../layout";
import { linkEnds } from "../linkSides";
import { columnHandle } from "../resources/columns";

/** A line's two handles. */
export interface Handles {
  readonly source: string;
  readonly target: string;
}

/** Whether a card's body places a step's port at its line or Flow step. */
function placesSteps(body: CardBody | undefined): boolean {
  return body === "code" || body === "flow";
}

/** The ends a link is drawn between (`routeLinks`): each its own item, or the collapsed frame shown in its place. */
interface DrawnEnds {
  readonly from: string;
  readonly to: string;
}

/**
 * The handles of a line drawn between `ends` (rectangles `from` and `to`): a connection's step port or
 * columns where its ends are drawn as themselves, else the facing sides.
 */
export function lineHandles(link: Link | undefined, ends: DrawnEnds, from: Rect, to: Rect, bodyOf: (id: string) => CardBody | undefined): Handles {
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
  } else if (link?.step !== undefined && own.from && placesSteps(bodyOf(link.from))) {
    source = stepPort(link.step);
  }
  return { source, target };
}

/** The step ports each card shows a connection leaving from (in Code or Flow, drawn as itself). */
export function stepPorts(links: readonly Link[], shownAs: ReadonlyMap<string, string>, bodyOf: (id: string) => CardBody | undefined): Map<string, string[]> {
  const ports = new Map<string, string[]>();
  for (const link of links) {
    if (link.kind !== undefined && link.kind !== "relates" && link.step !== undefined && !shownAs.has(link.from) && placesSteps(bodyOf(link.from))) {
      const port = stepPort(link.step);
      const list = ports.get(link.from) ?? [];
      if (!list.includes(port)) {
        list.push(port);
      }
      ports.set(link.from, list);
    }
  }
  return ports;
}

/** The columns each table shows a relationship leaving or entering by (drawn as itself). */
export function relatedColumns(links: readonly Link[], shownAs: ReadonlyMap<string, string>): Map<string, string[]> {
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
