/**
 * The text of a line's closed label (docs/proposals/typed-lines.md §7): `(CartItem[]) → Promise<Hold>`.
 * One input per argument (a passed reference: per parameter of the passed callable), each the type it
 * receives, else the parameter's name in gray, else `…`; a Python keyword keeps its name
 * (`currency=str`). Three or more inputs fold to the first and a count (`(Cart, +2) → Receipt`).
 * A missing output is `?`; inputs the reader can't list are `(…)`. Where a label doesn't fit beside
 * the cards, shorter forms keep the output: `(+2) → Gateway`, then `→ Gateway`.
 *
 * An arrow from a Diagram node that stands for several calls reads `3 calls`, one that joins several
 * at a collapsed frame `3 calls · 1 field`; a type arrow's label is its field's written type
 * (`CartItem[]`), cut short where it doesn't fit.
 */

import type { LabelInput, LineLabel, TypeText } from "../../ipc/generated";

/** How a part reads: a type, a parameter name without a type, punctuation, or something unknown. */
export type LabelTone = "type" | "name" | "punct" | "unknown";

export interface LabelPart {
  readonly text: string;
  readonly tone: LabelTone;
}

/** Inputs from this many on fold to the first and `+N`. */
export const FOLD_AT = 3;

function inputParts(input: LabelInput): LabelPart[] {
  if (input.type !== undefined) {
    const type: LabelPart = { text: input.type.text, tone: "type" };
    return input.keyword !== undefined ? [{ text: `${input.keyword}=`, tone: "name" }, type] : [type];
  }
  // Without a type, a keyword argument's keyword already names what it fills.
  const name = input.keyword ?? input.name;
  return [name !== undefined ? { text: name, tone: "name" } : { text: "…", tone: "unknown" }];
}

/** The label's parts in order; `fold: false` lists every input (the label's full text). */
export function labelParts(label: LineLabel, fold = true): LabelPart[] {
  const parts: LabelPart[] = [{ text: "(", tone: "punct" }];
  if (label.inputsUnknown) {
    parts.push({ text: "…", tone: "unknown" });
  } else {
    const { inputs } = label;
    const shown = fold && inputs.length >= FOLD_AT ? inputs.slice(0, 1) : inputs;
    shown.forEach((input, index) => {
      if (index > 0) {
        parts.push({ text: ", ", tone: "punct" });
      }
      parts.push(...inputParts(input));
    });
    if (shown.length < inputs.length) {
      parts.push({ text: `, +${inputs.length - shown.length}`, tone: "punct" });
    }
  }
  parts.push({ text: ") → ", tone: "punct" });
  parts.push(label.output !== undefined ? { text: label.output.text, tone: "type" } : { text: "?", tone: "unknown" });
  return parts;
}

/** The parts as one string. */
export function partsText(parts: readonly LabelPart[]): string {
  return parts.map((part) => part.text).join("");
}

/** The label as one string: folded as drawn, or in full. */
export function labelText(label: LineLabel, fold = true): string {
  return partsText(labelParts(label, fold));
}

/** The label's forms, longest first: as drawn, then with its inputs as a count, then the output alone. */
export function labelForms(label: LineLabel): LabelPart[][] {
  const output: LabelPart = label.output !== undefined ? { text: label.output.text, tone: "type" } : { text: "?", tone: "unknown" };
  const count: LabelPart = label.inputsUnknown ? { text: "…", tone: "unknown" } : { text: label.inputs.length > 0 ? `+${label.inputs.length}` : "", tone: "punct" };
  const forms = [
    labelParts(label),
    [{ text: "(", tone: "punct" } as const, count, { text: ") → ", tone: "punct" } as const, output],
    [{ text: "→ ", tone: "punct" } as const, output],
  ];
  // A form that reads the same as a longer one adds nothing (`() → void` has no count to drop).
  return forms.filter((form, index) => forms.findIndex((other) => partsText(other) === partsText(form)) === index);
}

/** "3 calls": the label of an arrow that stands for several calls. */
export function countText(count: number): string {
  return `${count} ${count === 1 ? "call" : "calls"}`;
}

/** The one form of a count's label. */
export function countForms(count: number): LabelPart[][] {
  return [[{ text: countText(count), tone: "type" }]];
}

/** "3 calls · 1 field": the label of an arrow that joins several at a collapsed frame. */
export function mergedText(calls: number, fields: number): string {
  return [calls > 0 ? countText(calls) : null, fields > 0 ? `${fields} ${fields === 1 ? "field" : "fields"}` : null].filter((part) => part !== null).join(" · ");
}

/** A written type longer than this many characters has a shorter form, cut with `…`. */
export const TYPE_LABEL_SHORT = 24;

/** A type arrow's forms: its field's written type, then (when long) its start and `…`. */
export function typeForms(type: TypeText): LabelPart[][] {
  const full: LabelPart[] = [{ text: type.text, tone: "type" }];
  if (type.text.length <= TYPE_LABEL_SHORT) {
    return [full];
  }
  return [full, [{ text: type.text.slice(0, TYPE_LABEL_SHORT - 1), tone: "type" }, { text: "…", tone: "punct" }]];
}
