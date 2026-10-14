/**
 * The blocks an agent names (docs/reader.md#agent-text). A short label (`try[0]/except[0]`,
 * `call:save_order`) maps to its full key by lookup in the definition's label map, which `code_text`
 * and `flow_text` return; anything else is taken as a full key, as written. The server never joins or
 * splits key strings: what a key names comes from the engine's answers.
 *
 * Also what a drawn flow lets the overlay do to each of its blocks, so a refusal can list the blocks an
 * agent can choose from instead.
 */

import type { Actions, CallBox, CodeLabel, DrawnFlow, OtherCall, Sequence } from "./engine-types";

/** A definition's blocks by short label, and back. */
export class Labels {
  private readonly keys: ReadonlyMap<string, string>;
  private readonly labels: ReadonlyMap<string, string>;

  constructor(labels: readonly CodeLabel[]) {
    this.keys = new Map(labels.map(({ label, key }) => [label, key]));
    this.labels = new Map(labels.map(({ label, key }) => [key, label]));
  }

  /** The full key of the block an agent named: a short label's key, else the text as written. */
  key(block: string): string {
    return this.keys.get(block) ?? block;
  }

  /** Whether text is one of the definition's short labels. */
  has(block: string): boolean {
    return this.keys.has(block);
  }

  /** A block's short label, when it is one of the definition's. */
  label(key: string): string | undefined {
    return this.labels.get(key);
  }

  /** Every short label, in the order the definition's blocks are written. */
  all(): string[] {
    return [...this.keys.keys()];
  }
}

/** A block a drawn flow draws, with what the overlay may do to it, and the flow it opens in place. */
interface DrawnPart {
  readonly key: string;
  readonly actions: Actions;
  readonly inset?: DrawnFlow;
}

function otherParts(calls: readonly OtherCall[]): DrawnPart[] {
  return calls.flatMap((call) => [{ key: call.key, actions: call.actions }, ...otherParts(call.inside)]);
}

function boxParts(box: CallBox): DrawnPart[] {
  return [
    { key: box.key, actions: box.actions, ...(box.inset ? { inset: box.inset } : {}) },
    ...otherParts(box.calls),
    ...box.callbacks.map((callback) => ({ key: callback.key, actions: callback.actions, ...(callback.inset ? { inset: callback.inset } : {}) })),
  ];
}

function sequenceParts(sequence: Sequence): DrawnPart[] {
  return sequence.steps.flatMap((step): DrawnPart[] => {
    switch (step.kind) {
      case "call":
        return boxParts(step);
      case "thin":
        return otherParts(step.calls);
      case "decision":
      case "loop":
      case "try":
      case "with":
        return [
          { key: step.key, actions: step.actions },
          ...step.headerCalls.flatMap(boxParts),
          ...step.lanes.flatMap((lane) => [{ key: lane.key, actions: lane.actions }, ...(lane.test?.calls ?? []).flatMap(boxParts), ...sequenceParts(lane.steps)]),
        ];
      case "exit":
        return [{ key: step.key, actions: step.actions }, ...(step.value?.kind === "call" ? boxParts(step.value) : [])];
      case "combined":
        return step.steps ? sequenceParts(step.steps) : [];
      case "proposed":
        return [];
    }
  });
}

/** The blocks a drawn flow draws of its own definition, in the order they run (not those of the flows it opens in place). */
function drawnParts(flow: DrawnFlow): DrawnPart[] {
  return [{ key: flow.header.key, actions: flow.header.actions }, ...otherParts(flow.header.definedWith), ...sequenceParts(flow.steps)];
}

/** The flow that block `key` opens in place, wherever the flow draws it (inside the flows it opens too). */
export function openedFlow(flow: DrawnFlow, key: string): DrawnFlow | undefined {
  for (const part of drawnParts(flow)) {
    if (part.key === key && part.inset) {
      return part.inset;
    }
    const inner = part.inset ? openedFlow(part.inset, key) : undefined;
    if (inner) {
      return inner;
    }
  }
  return undefined;
}

/** The definitions a drawn flow opens in place, at any depth, by key. */
export function openedDefinitions(flow: DrawnFlow): string[] {
  return drawnParts(flow).flatMap((part) => (part.inset ? [part.inset.header.key, ...openedDefinitions(part.inset)] : []));
}

/** The most blocks one list of a refusal names. */
const LISTED = 40;

function listed(items: readonly string[]): string {
  return items.length > LISTED ? `${items.slice(0, LISTED).join(", ")}, and ${items.length - LISTED} more (read_code lists them all)` : items.join(", ");
}

/**
 * The blocks of a card's drawn flow an agent can choose from, by short label: any block takes a name,
 * steps combine with their neighbours in one body (or alone), and project calls and callbacks open in
 * place.
 */
export function choiceLines(flow: DrawnFlow, labels: Labels): string[] {
  const own = drawnParts(flow).filter((part) => part.key !== flow.header.key);
  const shown = (key: string) => labels.label(key) ?? key;
  const bodies = new Map<string, string[]>();
  for (const part of own) {
    if (part.actions.combine?.with === "steps") {
      bodies.set(part.actions.combine.body, [...(bodies.get(part.actions.combine.body) ?? []), shown(part.key)]);
    }
  }
  const body = (key: string) => (key === flow.header.key ? "its own body" : shown(key));
  const alone = own.filter((part) => part.actions.combine?.with === "alone").map((part) => shown(part.key));
  const open = own.filter((part) => part.actions.open).map((part) => shown(part.key));
  return [
    `The blocks of ${flow.header.name ?? flow.header.label} you can choose from, by short label:`,
    `- names: any block, such as ${listed(labels.all())}; title names the card itself`,
    ...(bodies.size > 0 ? [`- combine: consecutive steps of one body: ${[...bodies].map(([key, steps]) => `in ${body(key)}: ${listed(steps)}`).join("; ")}`] : ["- combine: nothing in this card combines"]),
    ...(alone.length > 0 ? [`- combine alone: ${listed(alone)}`] : []),
    open.length > 0 ? `- open: ${listed(open)} (a path's next blocks are calls or callbacks of what the one before opens)` : "- open: no call of this card opens in place",
  ];
}
