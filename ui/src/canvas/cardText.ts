/**
 * What a card shows, read from what the reader drew (never from its key): how it is drawn (compact,
 * expanded, or as a type's fields), what its header says before its name, where it is, and the code
 * rows of an app's or entrance's card.
 */

import { shortPath } from "../code/text";
import { isTypeKind, typeKindWord } from "../code/typeKinds";
import type { CardView, Header, StartCard, StartTarget, TargetState, View } from "../ipc/generated";

/**
 * How a card is drawn: compact (its header; arrows leave its edge), expanded (its drawn flow, or an
 * app's or entrance's code rows; arrows leave from what makes them), or as a type's fields (a type's
 * card always, a class's while compact; calls leave its edge, type arrows its fields' rows).
 */
export type CardShape = "compact" | "expanded" | "fields";

/** How a card showing `view` is drawn in `shown` (its own view or its page's default). */
export function cardShape(view: CardView | undefined, shown: View): CardShape {
  if (view?.type === "definition") {
    const { kind } = view.flow.header;
    if (kind === "type" || (kind === "class" && shown === "compact")) {
      return "fields";
    }
  }
  return shown;
}

/** What a definition card's header shows before its name: a glyph (`ƒ`, `λ`, `M`), or its kind's word. */
export interface HeaderMark {
  readonly text: string;
  /** A word (`class`, `interface`, `type`, `enum`) rather than a glyph. */
  readonly word: boolean;
}

export function headerMark(header: Pick<Header, "kind" | "form">): HeaderMark {
  switch (header.kind) {
    case "class":
      return { text: "class", word: true };
    case "type":
      return { text: isTypeKind(header.form) ? typeKindWord(header.form) : header.form, word: true };
    case "module":
      return { text: "M", word: false };
    default:
      return { text: header.form === "lambda" ? "λ" : "ƒ", word: false };
  }
}

/** Where a definition is: `shop/service.py:9`. */
export function headerLocation(header: Pick<Header, "path" | "line">): string {
  return `${shortPath(header.path)}:${header.line}`;
}

/** Each state of a start's code in words. */
export const TARGET_STATE_WORDS: Readonly<Record<TargetState, string>> = {
  found: "found",
  missing: "missing",
  not_analyzed: "not analyzed",
  not_a_start: "not a start",
  stale: "stale",
};

/** One code row of a start card: the code, and its place among the start's code (its port). */
export interface StartRow {
  readonly code: StartTarget;
  readonly index: number;
}

/** The code rows a start card lists: all of its code when expanded, only the code that isn't found when compact. */
export function startRows(start: StartCard, shape: CardShape): StartRow[] {
  return start.code.flatMap((code, index) => (shape === "expanded" || code.state !== "found" ? [{ code, index }] : []));
}

/** What a code row is called: the definition's label, else its key as written in `unfold.toml`. */
export function startRowName(code: StartTarget): string {
  return code.label ?? code.key;
}

/** Where a code row's block is (`shop/api.py:11`), when it names one. */
export function startRowPlace(code: StartTarget): string | null {
  return code.path !== undefined ? `${shortPath(code.path)}${code.line !== undefined ? `:${code.line}` : ""}` : null;
}
