/**
 * What a page's cards draw, read one way for the window and for the MCP server
 * (docs/ui-shell.md#the-canvas, docs/ipc.md#agents). The reader decides what a card draws
 * (docs/reader.md#the-drawn-flow); this module only reads its answers, and never reads keys:
 *
 * - how a card shows (`cardShape`), which cards draw no flow (`flowless`), and which code an app's
 *   or entrance's card draws (`drawsBlock`);
 * - the one walk of a drawn flow (`visitFlow`), into the calls it opens in place, and what it gives:
 *   every block drawn (`flowBlocks`) and every project call drawn (`projectCalls`);
 * - the arrows a page draws between its cards (`pageArrows`): calls, starts, and type arrows.
 *
 * The canvas places these arrows' ports; `read_page` lists them.
 */

import type {
  Actions,
  CallBox,
  Callback,
  CardRef,
  CardView,
  DrawnFlow,
  Form,
  GapTick,
  HiddenBlock,
  Key,
  Lane,
  LinkEnd,
  OtherCall,
  Sequence,
  StartTarget,
  Step,
  TypeDefinition,
  TypeMember,
  TypeText,
  View,
} from "../../../ui/src/ipc/generated";

// ---------------------------------------------------------------------------------------------
// How a card shows

/**
 * How a card is drawn: compact (its header; arrows leave its edge), expanded (its drawn flow, or an
 * app's or entrance's code rows; arrows leave from what makes them), or as a type's fields (a type's
 * card always, a class's while compact; calls leave its edge, type arrows its fields' rows).
 */
export type CardShape = "compact" | "expanded" | "fields";

/**
 * How a card that draws `view` is drawn in `shown` (its own view or its page's default): a type's
 * card shows its fields in every view, a class's while it is compact (expanded, its class-level code).
 */
export function cardShape(view: CardView | undefined, shown: View): CardShape {
  if (view?.type === "definition") {
    const { kind } = view.flow.header;
    if (kind === "type" || (kind === "class" && shown === "compact")) {
      return "fields";
    }
  }
  return shown;
}

/**
 * Why a card draws no flow, so takes only a description: it is an app's or entrance's card
 * (`start`), or a type's card, which shows fields (`type`). Undefined for a card that draws a flow.
 */
export function flowless(ref: CardRef, view: CardView | undefined): "start" | "type" | undefined {
  if (!("block" in ref)) {
    return "start";
  }
  return view?.type === "definition" && view.flow.header.kind === "type" ? "type" : undefined;
}

/** Whether an app's or entrance's code has a block to draw, and an arrow to its card: found, or stale (its last good block). */
export function drawsBlock(code: StartTarget): boolean {
  return code.state === "found" || code.state === "stale";
}

// ---------------------------------------------------------------------------------------------
// The walk

/**
 * Where a part is drawn: the flow it is a part of (the card's own, or one a call or callback opens
 * in place), and the calls and callbacks opened around it, outermost first (none in the card's own).
 */
interface DrawnIn {
  readonly flow: DrawnFlow;
  readonly around: readonly Key[];
}

/**
 * What `visitFlow` hands each part to, with where it is drawn. A sequence comes with the body it is:
 * a definition's or a part's key, or an open combined step's id.
 */
interface FlowVisitor {
  /** A flow: the card's own first, then each one opened in place, where it opens. */
  readonly flow?: (at: DrawnIn) => void;
  readonly sequence?: (sequence: Sequence, body: string, at: DrawnIn) => void;
  /** Every step, before its parts. */
  readonly step?: (step: Step, at: DrawnIn) => void;
  /** A call box: a step, in a frame's header, in a test, or an exit's value. */
  readonly box?: (box: CallBox, at: DrawnIn) => void;
  readonly callback?: (callback: Callback, at: DrawnIn) => void;
  /** A library, unknown, or builtin call: in a thin row, listed on a box or after a test or header, or run when a definition is defined. */
  readonly other?: (call: OtherCall, at: DrawnIn) => void;
  readonly lane?: (lane: Lane, at: DrawnIn) => void;
  /** A control block with nothing drawn inside, named in a gap tick. */
  readonly hidden?: (block: HiddenBlock, at: DrawnIn) => void;
}

/**
 * Walks every part a drawn flow draws, in the order they run, into the calls and callbacks it opens
 * in place: the calls run when its definition is defined, then its body. A sequence's gap ticks come
 * before the step they sit on; a call box's listed calls, the flow it opens, then its callbacks; a
 * frame's header calls and unknown calls, then each lane with its test's calls and its body; an
 * exit's value call; an open combined step's steps.
 */
export function visitFlow(flow: DrawnFlow, visitor: FlowVisitor): void {
  walk(flow, visitor, []);
}

function walk(flow: DrawnFlow, visitor: FlowVisitor, around: readonly Key[]): void {
  const at: DrawnIn = { flow, around };
  const others = (calls: readonly OtherCall[]): void => {
    for (const call of calls) {
      visitor.other?.(call, at);
      others(call.inside);
    }
  };
  const box = (call: CallBox): void => {
    visitor.box?.(call, at);
    others(call.calls);
    if (call.inset) {
      walk(call.inset, visitor, [...around, call.key]);
    }
    for (const callback of call.callbacks) {
      visitor.callback?.(callback, at);
      if (callback.inset) {
        walk(callback.inset, visitor, [...around, callback.key]);
      }
    }
  };
  const hidden = (gap: GapTick | undefined): void => {
    for (const block of gap?.hidden ?? []) {
      visitor.hidden?.(block, at);
    }
  };
  const sequence = (steps: Sequence, body: string): void => {
    visitor.sequence?.(steps, body, at);
    for (const step of steps.steps) {
      hidden(step.gap);
      visitor.step?.(step, at);
      switch (step.kind) {
        case "call":
          box(step);
          break;
        case "thin":
          others(step.calls);
          break;
        case "decision":
        case "loop":
        case "try":
        case "with":
          step.headerCalls.forEach(box);
          others(step.headerUnknown);
          for (const lane of step.lanes) {
            visitor.lane?.(lane, at);
            (lane.test?.calls ?? []).forEach(box);
            others(lane.test?.unknown ?? []);
            sequence(lane.steps, lane.key);
          }
          break;
        case "exit":
          if (step.value?.kind === "call") {
            box(step.value);
          }
          break;
        case "combined":
          if (step.steps) {
            sequence(step.steps, step.item);
          }
          break;
        case "proposed":
          break;
      }
    }
    hidden(steps.endGap);
  };
  visitor.flow?.(at);
  others(flow.header.definedWith);
  sequence(flow.steps, flow.header.key);
}

/**
 * A block drawn in a card: its key, its form, its words from the code (a part's: the text that opens
 * it), the page's name for it, what the overlay may do to it, for a call, where it goes, and for a
 * part, the lines it marks in the code panel (first and last).
 */
export interface DrawnBlock {
  readonly key: Key;
  readonly form: Form;
  readonly label: string;
  readonly name?: string;
  readonly actions?: Actions;
  readonly to?: LinkEnd;
  readonly lines?: readonly [number, number];
}

function block(key: Key, form: Form, label: string, name: string | undefined, actions: Actions | undefined, to?: LinkEnd): DrawnBlock {
  return { key, form, label, ...(name !== undefined ? { name } : {}), ...(actions ? { actions } : {}), ...(to ? { to } : {}) };
}

/**
 * Every block a drawn flow draws, its definition first, then its steps in the order they run; with
 * `insets`, those of the calls and callbacks it opens in place too, where they open.
 */
export function flowBlocks(flow: DrawnFlow, insets = true): DrawnBlock[] {
  const blocks: DrawnBlock[] = [];
  const add = (at: DrawnIn, drawn: DrawnBlock) => {
    if (insets || at.around.length === 0) {
      blocks.push(drawn);
    }
  };
  visitFlow(flow, {
    flow: (at) => {
      const { header } = at.flow;
      add(at, block(header.key, header.form, header.label, header.name, header.actions));
    },
    hidden: (item, at) => {
      add(at, block(item.key, item.form, item.label, undefined, undefined));
    },
    step: (step, at) => {
      switch (step.kind) {
        case "decision":
        case "loop":
        case "try":
        case "with":
          add(at, block(step.key, step.form, step.header ?? step.form, step.name, step.actions));
          break;
        case "exit":
          add(at, block(step.key, step.form, step.value?.kind === "text" ? `${step.form} ${step.value.text}` : step.form, step.name, step.actions));
          break;
        default:
          break;
      }
    },
    box: (box, at) => {
      add(at, block(box.key, box.form, box.callee, box.name, box.actions, box.to));
    },
    callback: (callback, at) => {
      add(at, block(callback.key, "lambda", callback.label, callback.name, callback.actions));
    },
    other: (call, at) => {
      add(at, block(call.key, call.form, call.callee, undefined, call.actions, call.to));
    },
    lane: (lane, at) => {
      add(at, { ...block(lane.key, lane.form, lane.label, lane.name, lane.actions), lines: lane.lines });
    },
  });
  return blocks;
}

/** A project call a card draws: its block, its written callee, and the definition it reaches. */
interface ProjectCall {
  readonly call: Key;
  readonly callee: string;
  readonly to: Key;
}

/**
 * The project calls a drawn flow draws, in the order they run: its call boxes (in frames' headers,
 * tests, lanes, exits, and open combined steps), the calls a collapsed combined step holds, and,
 * with `insets`, those of the calls and callbacks it opens in place.
 */
export function projectCalls(flow: DrawnFlow, insets: boolean): ProjectCall[] {
  const calls: ProjectCall[] = [];
  visitFlow(flow, {
    box: (box, at) => {
      if ((insets || at.around.length === 0) && box.to.outcome === "project" && box.to.key !== undefined) {
        calls.push({ call: box.key, callee: box.callee, to: box.to.key });
      }
    },
    step: (step, at) => {
      if ((insets || at.around.length === 0) && step.kind === "combined" && !step.steps) {
        calls.push(...step.calls.map((held) => ({ call: held.key, callee: held.callee, to: held.to })));
      }
    },
  });
  return calls;
}

// ---------------------------------------------------------------------------------------------
// Arrows

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

/**
 * A card of a page, as its arrows read it: its id and key, the view it shows (its own, or its page's
 * default), what the reader drew for it (none while it loads or can't be read), and its type
 * definition while it shows fields.
 */
export interface DrawnCard {
  readonly id: string;
  readonly key: Key;
  readonly view: View;
  readonly model: CardView | undefined;
  readonly definition: TypeDefinition | undefined;
}

/**
 * Where an arrow leaves its card: its edge (a compact card's), a call (an expanded card's), a code
 * row (an expanded app's or entrance's card's), or a field's row.
 */
export type ArrowPort = { readonly at: "edge" } | { readonly at: "call"; readonly call: Key } | { readonly at: "code"; readonly index: number } | { readonly at: "field"; readonly row: string };

/**
 * An arrow a page draws between two of its cards: a call's, standing for the project calls it leaves
 * from in the order they run; a start's, to the card of the code it starts at; or a field's, to the
 * card of a type its written type names.
 */
export type PageArrow = {
  readonly source: string;
  readonly port: ArrowPort;
  readonly target: string;
  /** A start's arrow to code whose file stopped parsing, drawn to its last good code. */
  readonly dashed: boolean;
  /** The order it leaves its card in. */
  readonly order: number;
} & (
  | { readonly kind: "call"; readonly calls: readonly ProjectCall[] }
  | { readonly kind: "start"; readonly code: StartTarget }
  | { readonly kind: "type"; readonly field: FieldRow & { readonly type: TypeText } }
);

/** The card a key reaches on the page: the first card of the key, never an app's or entrance's. */
type CardOf = (key: Key) => string | undefined;

/** The calls of a definition card as arrows: one per call on an expanded card, else one per card reached from its edge. */
function callArrows(card: DrawnCard, shape: CardShape, flow: DrawnFlow, cardOf: CardOf): PageArrow[] {
  const expanded = shape === "expanded";
  const arrows = new Map<string, { readonly port: ArrowPort; readonly target: string; readonly order: number; readonly calls: ProjectCall[] }>();
  projectCalls(flow, expanded).forEach((call, order) => {
    const target = cardOf(call.to);
    if (target === undefined || target === card.id) {
      return;
    }
    const port: ArrowPort = expanded ? { at: "call", call: call.call } : { at: "edge" };
    const id = JSON.stringify([port, target]);
    const known = arrows.get(id);
    if (known) {
      if (!known.calls.some((item) => item.call === call.call)) {
        known.calls.push(call);
      }
      return;
    }
    arrows.set(id, { port, target, order, calls: [call] });
  });
  return [...arrows.values()].map(({ port, target, order, calls }) => ({ source: card.id, port, target, kind: "call", dashed: false, order, calls }));
}

/** An app's or entrance's arrows: to the card of each of its code, in order; dashed to code whose file stopped parsing. */
function startArrows(card: DrawnCard, shape: CardShape, start: Extract<CardView, { type: "start" }>, cardOf: CardOf): PageArrow[] {
  const seen = new Set<string>();
  return start.code.flatMap((code, index): PageArrow[] => {
    const target = drawsBlock(code) ? cardOf(code.key) : undefined;
    const port: ArrowPort = shape === "expanded" ? { at: "code", index } : { at: "edge" };
    const id = JSON.stringify([port, target]);
    if (target === undefined || target === card.id || seen.has(id)) {
      return [];
    }
    seen.add(id);
    return [{ source: card.id, port, target, kind: "start", dashed: code.state === "stale", order: index, code }];
  });
}

/** A type card's type arrows: from each field's row to each other card its written type names. */
function typeArrows(card: DrawnCard, definition: TypeDefinition, cardOf: CardOf): PageArrow[] {
  // A base on the page lists its own fields there, with their arrows.
  const shownBases = new Set(
    definition.inherited.flatMap((group, base) => {
      const to = group.from.spans[0]?.to;
      return to?.outcome === "project" && to.key !== undefined && cardOf(to.key) !== undefined ? [base] : [];
    }),
  );
  const arrows: PageArrow[] = [];
  fieldRows(definition).forEach((row, order) => {
    const { type } = row;
    if (!type || (row.base !== undefined && shownBases.has(row.base))) {
      return;
    }
    const field = { ...row, type };
    const reached = new Set<string>();
    for (const { to } of type.spans) {
      const target = to.outcome === "project" && to.key !== undefined ? cardOf(to.key) : undefined;
      if (target === undefined || target === card.id || reached.has(target)) {
        continue;
      }
      reached.add(target);
      arrows.push({ source: card.id, port: { at: "field", row: field.row }, target, kind: "type", dashed: false, order, field });
    }
  });
  return arrows;
}

/**
 * Every arrow a page draws, card by card, each card's in the order they leave it (docs/ui-shell.md#the-canvas):
 *
 * - **Calls.** Every project call a definition card draws (`projectCalls`) whose definition has a card
 *   on the page: from the call on an expanded card, its insets' calls and its collapsed combined steps'
 *   included; from the edge on a compact card (and a fields card), one arrow per card it calls,
 *   standing for its calls in the order they run.
 * - **Starts.** An app's or entrance's card points at the card of each of its code, found or stale (its
 *   file stopped parsing: dashed), from each code row when expanded, else from its edge.
 * - **Types.** A card showing fields points from a field's row to the card of each type its written
 *   type names, one per field and card; a base with its own card on the page draws its own fields'.
 *
 * A call or a type reaches the first card of its key. A card not loaded yet draws no arrows.
 */
export function pageArrows(cards: readonly DrawnCard[]): PageArrow[] {
  const first = new Map<Key, string>();
  for (const card of cards) {
    if (card.model?.type !== "start" && !first.has(card.key)) {
      first.set(card.key, card.id);
    }
  }
  const cardOf: CardOf = (key) => first.get(key);
  return cards.flatMap((card) => {
    const { model } = card;
    if (!model) {
      return [];
    }
    const shape = cardShape(model, card.view);
    if (model.type === "start") {
      return startArrows(card, shape, model, cardOf);
    }
    return [...callArrows(card, shape, model.flow, cardOf), ...(shape === "fields" && card.definition ? typeArrows(card, card.definition, cardOf) : [])];
  });
}
