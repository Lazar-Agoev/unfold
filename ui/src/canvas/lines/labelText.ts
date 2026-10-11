/**
 * The text of a line's closed label (docs/proposals/typed-lines.md §7): `(CartItem[]) → Promise<Hold>`.
 * One input per argument (a passed reference: per parameter of the passed callable), each the type it
 * receives, else the parameter's name in gray, else `…`; a Python keyword keeps its name
 * (`currency=str`). Three or more inputs fold to the first and a count (`(Cart, +2) → Receipt`).
 * A missing output is `?`; inputs the reader can't list are `(…)`. Where a label doesn't fit beside
 * the cards, shorter forms keep the output: `(+2) → Gateway`, then `→ Gateway`.
 */

import type { LabelInput, LineLabel } from "../../ipc/generated";

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
