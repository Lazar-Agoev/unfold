/**
 * What a connection says. At rest its chip says its kind's word, then what it touches: the channel (a
 * route, a queue, an event; for reads and writes, the table when it names none) — `queue ingest`,
 * `writes jobs`, `runtime call`. An `other` connection's chip is its label; a relationship has none (its
 * ends' marks say how many). Hovered or selected, its card says all of it: the word, what it touches,
 * the type it carries, and its label (`connectionTitle`), with its description under them. Chips have a
 * shorter form for the label layout: the word alone. Pure.
 */

import type { Cardinality, ConnectionKind } from "../../ipc/generated";
import { manyEnds } from "./ends";

/** One part of a chip: the kind's word, code as written (a channel, a type), or words (a label). */
export interface ChipPart {
  readonly text: string;
  readonly tone: "word" | "code" | "text";
}

/** Each kind's word on its chip and where the page and Selected list connections (menus use `CONNECTION_WORDS`). */
const KIND_WORDS: Readonly<Record<ConnectionKind, string>> = {
  http: "HTTP",
  queue: "queue",
  writes: "writes",
  reads: "reads",
  calls: "runtime call",
  relates: "relates",
  other: "other",
};

/** A relationship's cardinality as marks: `1 : 1`, `1 : N`, `N : 1`, `N : N`. */
export function cardinalityText(cardinality: Cardinality): string {
  const many = manyEnds(cardinality);
  return `${many.from ? "N" : "1"} : ${many.to ? "N" : "1"}`;
}

/** What a chip reads. */
export interface ChipInput {
  readonly kind: ConnectionKind;
  readonly channel?: string;
  /** The carried type's name as written (`OrderJob`). */
  readonly carries?: string;
  readonly label?: string;
  /** What a reads or writes connection touches when it names no channel: its resource card's name. */
  readonly subject?: string;
  readonly cardinality?: Cardinality;
  /** How many connections of the kind a merged chip stands for (at a collapsed frame). */
  readonly count?: number;
}

/** The chip's word: the kind's, a relationship's cardinality, or an `other` connection's label. */
function wordOf(input: ChipInput): ChipPart {
  if (input.kind === "other") {
    return { text: input.label ?? KIND_WORDS.other, tone: input.label !== undefined ? "text" : "word" };
  }
  if (input.kind === "relates" && input.cardinality !== undefined) {
    return { text: cardinalityText(input.cardinality), tone: "word" };
  }
  return { text: KIND_WORDS[input.kind], tone: "word" };
}

/** What it touches: the channel, or a read's or write's table. */
function touches(input: ChipInput): ChipPart | null {
  const through = input.channel ?? (input.kind === "reads" || input.kind === "writes" ? input.subject : undefined);
  return through !== undefined ? { text: through, tone: "code" } : null;
}

/** The chip's forms at rest, longest first, none the same: the word and what it touches, then the word; none for a relationship. */
export function chipForms(input: ChipInput): ChipPart[][] {
  const word = wordOf(input);
  if (input.count !== undefined && input.count > 1) {
    return [[word, { text: `${input.count} connections`, tone: "text" }], [word]];
  }
  if (input.kind === "relates") {
    return [];
  }
  const target = touches(input);
  return target !== null ? [[word, target], [word]] : [[word]];
}

/** All a connection says in a line, for its card and its accessible name: the word, what it touches, the type it carries, and its label. */
export function connectionTitle(input: ChipInput): ChipPart[] {
  const word = wordOf(input);
  if (input.count !== undefined && input.count > 1) {
    return [word, { text: `${input.count} connections`, tone: "text" }];
  }
  const parts: (ChipPart | null)[] = [
    word,
    touches(input),
    input.carries !== undefined ? { text: input.carries, tone: "code" } : null,
    input.label !== undefined && input.kind !== "other" ? { text: input.label, tone: "text" } : null,
  ];
  return parts.filter((part): part is ChipPart => part !== null);
}

/** Parts as plain text: the word and what it touches with a space, anything after with " · ". */
export function chipText(parts: readonly ChipPart[]): string {
  return parts.map((part, index) => (index === 0 ? part.text : index === 1 && part.tone === "code" ? ` ${part.text}` : ` · ${part.text}`)).join("");
}
