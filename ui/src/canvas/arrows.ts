/**
 * Arrows between cards, from what the reader drew (docs/designs/core-rebuild-plan.md, step 11). The
 * board file holds no arrows; they are derived from the cards' drawn flows, start cards, and type
 * definitions.
 *
 * - **Calls.** Every project call box of a card's drawn flow whose definition has a card on the page
 *   draws an arrow to that card. On an expanded card it leaves from the call (`callPort`: the card
 *   places the port level with the call's box, in the calls it opens in place too); on a compact card
 *   (and a type's or compact class's fields) from the card's edge, one arrow per card it calls, standing
 *   for all of those calls in the order they run.
 * - **Starts.** An app's or entrance's card points at the cards for its code, in order: from each code
 *   row when expanded, else from its edge. Code whose file stopped parsing (stale) is dashed.
 * - **Types.** A type card's fields (and a compact class's) point from a field's row to each card on the
 *   page its written type names, one per field and card.
 *
 * A call in a collapsed combined step draws no arrow: the drawn flow names it only by key.
 */

import type { CallBox, CardView, DrawnFlow, Key, ProjectId, Sequence, TypeDefinition, TypeMember, TypeText, View } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";
import { cardShape, type CardShape } from "./cardText";

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
  /** The project both cards are of. */
  readonly project: ProjectId;
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

/** A project call of a drawn flow: its block and the definition it reaches. */
interface ProjectCall {
  readonly call: Key;
  readonly to: Key;
}

/**
 * The project calls a drawn flow draws, in the order they run: its call boxes (in frames' headers,
 * tests, lanes, exits, and open combined steps), and, with `insets`, those of the calls it opens in
 * place.
 */
export function projectCalls(flow: DrawnFlow, insets: boolean): ProjectCall[] {
  const calls: ProjectCall[] = [];
  const box = (call: CallBox) => {
    if (call.to.outcome === "project" && call.to.key !== undefined) {
      calls.push({ call: call.key, to: call.to.key });
    }
    if (insets) {
      if (call.inset) {
        sequence(call.inset.steps);
      }
      for (const callback of call.callbacks) {
        if (callback.inset) {
          sequence(callback.inset.steps);
        }
      }
    }
  };
  function sequence(steps: Sequence): void {
    for (const step of steps.steps) {
      switch (step.kind) {
        case "call":
          box(step);
          break;
        case "decision":
        case "loop":
        case "try":
        case "with":
          step.headerCalls.forEach(box);
          for (const lane of step.lanes) {
            (lane.test?.calls ?? []).forEach(box);
            sequence(lane.steps);
          }
          break;
        case "exit":
          if (step.value?.kind === "call") {
            box(step.value);
          }
          break;
        case "combined":
          if (step.steps) {
            sequence(step.steps);
          }
          break;
        case "thin":
        case "proposed":
          break;
      }
    }
  }
  sequence(flow.steps);
  return calls;
}

/** One row of a type card's fields: its id, the member it shows, and its written type. */
export interface FieldRow {
  /** `=` for an alias's aliased type, `<index>` for a member, `from<g>/<index>` for a member inherited from base g. */
  readonly row: string;
  /** The member; null for an alias's aliased type. */
  readonly member: TypeMember | null;
  readonly type: TypeText | undefined;
  /** For an inherited member, the index of the base it comes from. */
  readonly base?: number;
  /**
   * The member as the code panel opens it: the type that writes it (the card's, or the project base it
   * is inherited from) and its index among that type's written members. Absent for an alias's aliased
   * type.
   */
  readonly field?: FieldRef;
}

/** A written member of a class or type: the type's key and the member's index among its written members. */
export interface FieldRef {
  readonly key: Key;
  readonly index: number;
}

/**
 * The rows a type card lists, in order: an alias's aliased type, its own members, then the members it
 * inherits, base by base.
 */
export function fieldRows(definition: TypeDefinition): FieldRow[] {
  return [
    ...(definition.aliased ? [{ row: "=", member: null, type: definition.aliased }] : []),
    ...definition.members.map((member, index) => ({ row: String(index), member, type: member.type, field: { key: definition.key, index } })),
    ...definition.inherited.flatMap((group, base) => {
      // A base that lists members is a project type, and its one span names its definition.
      const from = group.from.spans[0]?.to.key;
      return group.members.map((member, index) => ({ row: `from${base}/${index}`, member, type: member.type, base, ...(from !== undefined ? { field: { key: from, index } } : {}) }));
    }),
  ];
}

/** The calls of a definition card as arrows: one per call on an expanded card, else one per card reached from its edge. */
function callArrows(card: ArrowCard, shape: CardShape, flow: DrawnFlow, cardOf: (key: Key) => string | undefined): Arrow[] {
  const expanded = shape === "expanded";
  const arrows = new Map<string, { arrow: Omit<Arrow, "calls">; calls: Key[] }>();
  projectCalls(flow, expanded).forEach(({ call, to }, order) => {
    const target = cardOf(to);
    if (target === undefined || target === card.id) {
      return;
    }
    const handle = expanded ? callPort(call) : NODE_PORT;
    const id = `${card.id}|${handle}|${target}`;
    const known = arrows.get(id);
    if (known) {
      if (!known.calls.includes(call)) {
        known.calls.push(call);
      }
      return;
    }
    arrows.set(id, { arrow: { id, source: card.id, sourceHandle: handle, target, project: card.project, kind: "call", dashed: false, order }, calls: [call] });
  });
  return [...arrows.values()].map(({ arrow, calls }) => ({ ...arrow, calls }));
}

/** A start card's arrows: to the card for each of its code, in order. */
function startArrows(card: ArrowCard, shape: CardShape, start: Extract<CardView, { type: "start" }>, cardOf: (key: Key) => string | undefined): Arrow[] {
  const seen = new Set<string>();
  return start.code.flatMap((code, index): Arrow[] => {
    const target = code.state === "found" || code.state === "stale" ? cardOf(code.key) : undefined;
    const handle = shape === "expanded" ? codePort(index) : NODE_PORT;
    const id = `${card.id}|${handle}|${target ?? ""}`;
    if (target === undefined || target === card.id || seen.has(id)) {
      return [];
    }
    seen.add(id);
    return [{ id, source: card.id, sourceHandle: handle, target, project: card.project, kind: "start", dashed: code.state === "stale", order: index, calls: [] }];
  });
}

/** A type card's type arrows: from each field's row to each other card its written type names. */
function typeArrows(card: ArrowCard, definition: TypeDefinition, cardOf: (key: Key) => string | undefined): Arrow[] {
  // A base on the page lists its own fields there, with their arrows.
  const shownBases = new Set(
    definition.inherited.flatMap((group, base) => {
      const to = group.from.spans[0]?.to;
      return to?.outcome === "project" && to.key !== undefined && cardOf(to.key) !== undefined ? [base] : [];
    }),
  );
  const arrows: Arrow[] = [];
  fieldRows(definition).forEach(({ row, type, base }, order) => {
    if (!type || (base !== undefined && shownBases.has(base))) {
      return;
    }
    const reached = new Set<string>();
    for (const { to } of type.spans) {
      const target = to.outcome === "project" && to.key !== undefined ? cardOf(to.key) : undefined;
      if (target === undefined || target === card.id || reached.has(target)) {
        continue;
      }
      reached.add(target);
      const port = fieldPort(row);
      arrows.push({ id: `${card.id}|${port}|${target}`, source: card.id, sourceHandle: port, target, project: card.project, kind: "type", dashed: false, order, calls: [] });
    }
  });
  return arrows;
}

/**
 * Every arrow on a page. `models` maps `cardRefKey(project, key)` to what the reader drew for loaded
 * cards (one not loaded draws no arrows yet); `definitions` maps the same keys to the loaded type
 * definitions of the cards that show fields.
 */
export function computeArrows(cards: readonly ArrowCard[], models: ReadonlyMap<string, CardView>, definitions: ReadonlyMap<string, TypeDefinition> = new Map()): Arrow[] {
  const cardOfRef = new Map<string, string>();
  for (const card of cards) {
    const ref = cardRefKey(card.project, card.key);
    if (models.get(ref)?.type !== "start" && !cardOfRef.has(ref)) {
      cardOfRef.set(ref, card.id);
    }
  }
  return cards.flatMap((card) => {
    const ref = cardRefKey(card.project, card.key);
    const model = models.get(ref);
    if (!model) {
      return [];
    }
    const cardOf = (key: Key) => cardOfRef.get(cardRefKey(card.project, key));
    const shape = cardShape(model, card.view);
    if (model.type === "start") {
      return startArrows(card, shape, model, cardOf);
    }
    const definition = shape === "fields" ? definitions.get(ref) : undefined;
    return [...callArrows(card, shape, model.flow, cardOf), ...(definition ? typeArrows(card, definition, cardOf) : [])];
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
