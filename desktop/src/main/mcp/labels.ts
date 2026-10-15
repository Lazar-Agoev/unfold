/**
 * The blocks an agent names (docs/reader.md#agent-text). A short label (`try[0]/except[0]`,
 * `call:save_order`) maps to its full key by lookup in the definition's label map, which `code_text`
 * and a card's agent text return; anything else is taken as a full key, as written. The server never
 * joins or splits key strings: what a key names comes from the engine's answers.
 *
 * Also what a drawn flow (read through the walk the window shares) opens in place, and what it lets the
 * overlay do to each of its blocks, so a refusal can list the blocks an agent can choose from instead.
 */

import { flowBlocks, visitFlow } from "../../shared/drawing";
import type { CodeLabel, DrawnFlow } from "./engine-types";

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

/** The flow that call or callback `key` opens in place, wherever the flow draws it (inside the flows it opens too). */
export function openedFlow(flow: DrawnFlow, key: string): DrawnFlow | undefined {
  let found: DrawnFlow | undefined;
  const at = (own: string, inset: DrawnFlow | undefined) => {
    if (found === undefined && own === key) {
      found = inset;
    }
  };
  visitFlow(flow, {
    box: (box) => {
      at(box.key, box.inset);
    },
    callback: (callback) => {
      at(callback.key, callback.inset);
    },
  });
  return found;
}

/** The definitions a drawn flow opens in place, at any depth, by key. */
export function openedDefinitions(flow: DrawnFlow): string[] {
  const opened: string[] = [];
  visitFlow(flow, {
    flow: (at) => {
      if (at.around.length > 0) {
        opened.push(at.flow.header.key);
      }
    },
  });
  return opened;
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
  const own = flowBlocks(flow, false).filter((block) => block.key !== flow.header.key);
  const shown = (key: string) => labels.label(key) ?? key;
  const bodies = new Map<string, string[]>();
  for (const block of own) {
    if (block.actions?.combine?.with === "steps") {
      bodies.set(block.actions.combine.body, [...(bodies.get(block.actions.combine.body) ?? []), shown(block.key)]);
    }
  }
  const body = (key: string) => (key === flow.header.key ? "its own body" : shown(key));
  const alone = own.filter((block) => block.actions?.combine?.with === "alone").map((block) => shown(block.key));
  const open = own.filter((block) => block.actions?.open === true).map((block) => shown(block.key));
  return [
    `The blocks of ${flow.header.name ?? flow.header.label} you can choose from, by short label:`,
    `- names: any block, such as ${listed(labels.all())}; title names the card itself`,
    ...(bodies.size > 0 ? [`- combine: consecutive steps of one body: ${[...bodies].map(([key, steps]) => `in ${body(key)}: ${listed(steps)}`).join("; ")}`] : ["- combine: nothing in this card combines"]),
    ...(alone.length > 0 ? [`- combine alone: ${listed(alone)}`] : []),
    open.length > 0 ? `- open: ${listed(open)} (a path's next blocks are calls or callbacks of what the one before opens)` : "- open: no call of this card opens in place",
  ];
}
