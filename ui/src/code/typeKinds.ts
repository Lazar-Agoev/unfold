/**
 * Type declarations as the rest of the UI names them: `interface Cart`, `type CartId`, `enum Currency`
 * (classes are definitions with code of their own). A type declaration has no paths to draw, so a new
 * card of one opens in Code whatever the page's lens.
 */

import type { View } from "../ipc/generated";

const DECLARATION_WORDS: Readonly<Record<string, string>> = { interface: "interface", type_alias: "type", enum: "enum" };

/** The word a type declaration of block kind `kind` is written with; undefined for any other kind. */
export function declarationWord(kind: string): string | undefined {
  return DECLARATION_WORDS[kind];
}

/** The view a new card of a block of `kind` opens in: Code for a type declaration, else the page's lens. */
export function newCardView(kind: string): View | undefined {
  return declarationWord(kind) === undefined ? undefined : "code";
}
