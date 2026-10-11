/**
 * What a connection's chip says (design §4.7): its kind's word, then what travels: the channel (a
 * route, a queue, an event; for reads and writes, the table when it names none), the type it carries,
 * and its label (`queue · orders · OrderJob`, `writes · orders`, `runtime call`). A relationship's
 * chip is its cardinality (`N : 1`); an `other` connection's is its label. Chips have shorter forms
 * for the label layout: without the label, then without the type, then the word alone. Pure.
 */

import type { Cardinality, LinkKind } from "../../ipc/generated";
import { manyEnds } from "./ends";

/** One part of a chip: the kind's word, code as written (a channel, a type), words (a label), or a separator. */
export interface ChipPart {
  readonly text: string;
  readonly tone: "word" | "code" | "text" | "sep";
  /** The carried type's name: a click opens its definition. */
  readonly carries?: true;
}

/** Each kind's word on its chip and in menus. */
export const KIND_WORDS: Readonly<Record<LinkKind, string>> = {
  http: "HTTP",
  rpc: "RPC",
  queue: "queue",
  event: "event",
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
  readonly kind: LinkKind;
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

const SEP: ChipPart = { text: " · ", tone: "sep" };

/** Parts joined by separators. */
function joined(parts: readonly (ChipPart | null)[]): ChipPart[] {
  return parts.flatMap((part, index) => (part === null ? [] : index === 0 ? [part] : [SEP, part]));
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

/** The chip's forms, longest first, none the same: all of it, then without the label, then without the type, then the word. */
export function chipForms(input: ChipInput): ChipPart[][] {
  const word = wordOf(input);
  if (input.count !== undefined && input.count > 1) {
    return [joined([word, { text: `${input.count} connections`, tone: "text" }]), [word]];
  }
  const through = input.channel ?? (input.kind === "reads" || input.kind === "writes" ? input.subject : undefined);
  const channel: ChipPart | null = through !== undefined ? { text: through, tone: "code" } : null;
  const carries: ChipPart | null = input.carries !== undefined ? { text: input.carries, tone: "code", carries: true } : null;
  const label: ChipPart | null = input.label !== undefined && input.kind !== "other" ? { text: input.label, tone: "text" } : null;
  const forms = [joined([word, channel, carries, label]), joined([word, channel, carries]), joined([word, channel]), [word]];
  return forms.filter((form, index) => forms.findIndex((other) => chipText(other) === chipText(form)) === index);
}

/** A form as plain text. */
export function chipText(parts: readonly ChipPart[]): string {
  return parts.map((part) => part.text).join("");
}
