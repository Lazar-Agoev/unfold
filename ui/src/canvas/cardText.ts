/**
 * What a card shows, read from what the reader drew (never from its key): what its header says before
 * its name, where it is, and the code rows of an app's or entrance's card. How it is drawn is
 * `cardShape` (`@unfold/desktop/drawing`).
 */

import { drawsBlock, type CardShape } from "@unfold/desktop/drawing";
import { shortPath } from "../code/text";
import { isTypeKind, typeKindWord } from "../code/typeKinds";
import type { Header, StartCard, StartTarget } from "../ipc/generated";

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

/** One code row of a start card: the code, and its place among the start's code (its port). */
export interface StartRow {
  readonly code: StartTarget;
  readonly index: number;
}

/** The code rows a start card lists when expanded: the code that has a block, in order. */
export function startRows(start: StartCard, shape: CardShape): StartRow[] {
  return shape === "expanded" ? start.code.flatMap((code, index) => (drawsBlock(code) ? [{ code, index }] : [])) : [];
}

/** A start's code that isn't found (missing, not analyzed, or not a start), each drawn as its own dashed card beside the start's. */
export function unfoundCode(start: StartCard): StartRow[] {
  return start.code.flatMap((code, index) => (drawsBlock(code) ? [] : [{ code, index }]));
}

/** What a code row is called: the definition's label, else its key as written in `unfold.toml`. */
export function startRowName(code: StartTarget): string {
  return code.label ?? code.key;
}

/** Where a code row's block is (`shop/api.py:11`), when it names one. */
export function startRowPlace(code: StartTarget): string | null {
  return code.path !== undefined ? `${shortPath(code.path)}${code.line !== undefined ? `:${code.line}` : ""}` : null;
}
