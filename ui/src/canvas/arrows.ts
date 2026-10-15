/**
 * Arrows between cards on the canvas (docs/ui-shell.md#the-canvas). Which arrows a page draws is
 * `pageArrows`, the rule the window and the MCP server share (`@unfold/desktop/drawing`): from call
 * boxes, collapsed combined steps, and the calls a card opens in place; from an app's or entrance's
 * card to its code; from a type's fields. The canvas only places them: each leaves its card from a
 * port (`callPort`, `codePort`, `fieldPort`, or the edge, `NODE_PORT`). The board file holds no
 * arrows.
 */

import { pageArrows, type ArrowPort, type DrawnCard } from "@unfold/desktop/drawing";
import type { CardView, Key, ProjectId, TypeDefinition, View } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";

export interface ArrowCard {
  readonly id: string;
  readonly key: Key;
  /** The project the card's drawn flow is read from: its board's. */
  readonly project: ProjectId;
  /** The view it shows: its own, or its page's default. */
  readonly view: View;
}

export interface Arrow {
  /** Stable: source card, port, target card. */
  readonly id: string;
  readonly source: string;
  /**
   * The source card's port: a call's (`callPort`) on an expanded card, a code row's (`codePort`) on an
   * expanded start card, a field's (`fieldPort`), or the card's edge (`NODE_PORT`).
   */
  readonly sourceHandle: string;
  readonly target: string;
  /** A call's arrow, a start's to its code, or a field's to its type. */
  readonly kind: "call" | "start" | "type";
  readonly dashed: boolean;
  /** The order it leaves its card in. */
  readonly order: number;
  /** The call blocks it stands for, in the order they run (a call's arrow only). */
  readonly calls: readonly Key[];
  /** When it joins several arrows at a collapsed frame (`routeArrows`). */
  readonly merged?: true;
  /** The cards it joins, when an end attaches to a collapsed frame instead (`routeArrows`). */
  readonly cards?: { readonly from: string; readonly to: string };
}

/** The port at a card's edge, level with its header: a compact card's arrows leave from it. */
export const NODE_PORT = "node";

/** The port of a call block on an expanded card. */
export function callPort(call: Key): string {
  return `call:${call}`;
}

/** The port a connection leaves from at its step (a call block) on an expanded card. */
export function stepPort(step: Key): string {
  return `step:${step}`;
}

/** The port of a start card's code row `index`. */
export function codePort(index: number): string {
  return `code:${index}`;
}

/** The port of a type card's field in row `row` (`0`, `from1/2`, `=`: see `fieldRows`). */
export function fieldPort(row: string): string {
  return `field:${row}`;
}

/** The port an arrow leaves its card from. */
function handle(port: ArrowPort): string {
  switch (port.at) {
    case "edge":
      return NODE_PORT;
    case "call":
      return callPort(port.call);
    case "code":
      return codePort(port.index);
    case "field":
      return fieldPort(port.row);
  }
}

/**
 * Every arrow on a page (`pageArrows`), at its ports. `models` maps `cardRefKey(project, key)` to what
 * the reader drew for loaded cards (one not loaded draws no arrows yet); `definitions` maps the same
 * keys to the loaded type definitions of the cards that show fields.
 */
export function computeArrows(cards: readonly ArrowCard[], models: ReadonlyMap<string, CardView>, definitions: ReadonlyMap<string, TypeDefinition> = new Map()): Arrow[] {
  const drawn = cards.map((card): DrawnCard => {
    const ref = cardRefKey(card.project, card.key);
    return { id: card.id, key: card.key, view: card.view, model: models.get(ref), definition: definitions.get(ref) };
  });
  return pageArrows(drawn).map((arrow) => {
    const sourceHandle = handle(arrow.port);
    return {
      id: `${arrow.source}|${sourceHandle}|${arrow.target}`,
      source: arrow.source,
      sourceHandle,
      target: arrow.target,
      kind: arrow.kind,
      dashed: arrow.dashed,
      order: arrow.order,
      calls: arrow.kind === "call" ? arrow.calls.map((call) => call.call) : [],
    };
  });
}

/** Each card's ports (handle ids with an arrow), sorted, for the cards to draw. */
export function portsByCard(arrows: readonly Arrow[]): Map<string, string[]> {
  const ports = new Map<string, string[]>();
  for (const arrow of arrows) {
    const list = ports.get(arrow.source) ?? [];
    if (!list.includes(arrow.sourceHandle)) {
      list.push(arrow.sourceHandle);
    }
    ports.set(arrow.source, list);
  }
  for (const list of ports.values()) {
    list.sort();
  }
  return ports;
}
