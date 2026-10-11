/**
 * Type declarations and type cards as the rest of the UI names them: `interface Cart`, `type CartId`,
 * `enum Currency`. A type card is a card of a type declaration or a class (design §4.2): its Diagram view
 * lists its fields. A type declaration has no paths, so Flow shows its fields too; a class keeps its Flow
 * (its methods), since its fields are its Diagram view.
 */

import type { Key, TypeKind, TypeMember, View } from "../ipc/generated";

const DECLARATION_WORDS: Readonly<Record<string, string>> = { interface: "interface", type_alias: "type", enum: "enum" };

/** The kinds of block a type card shows. */
const TYPE_KINDS: ReadonlySet<string> = new Set<TypeKind>(["interface", "type_alias", "enum", "class"]);

/** The word a type declaration of block kind `kind` is written with; undefined for any other kind. */
export function declarationWord(kind: string): string | undefined {
  return DECLARATION_WORDS[kind];
}

/** How a type reads before its name: `interface`, `type`, `enum`, or `class`. */
export function typeKindWord(kind: TypeKind): string {
  return declarationWord(kind) ?? "class";
}

/** Whether a block of `kind` is shown as a type card: a type declaration or a class. */
export function isTypeKind(kind: string): kind is TypeKind {
  return TYPE_KINDS.has(kind);
}

/**
 * The block kind a structural key names: its last step's kind (`src/cart.ts#interface:Cart` →
 * `interface`); undefined for an entrance or a file root.
 */
export function keyBlockKind(key: Key): string | undefined {
  if (key.startsWith("entrance:")) {
    return undefined;
  }
  const local = key.slice(key.lastIndexOf("#") + 1);
  const step = local.slice(local.lastIndexOf("/") + 1);
  const kind = /^[^:[]+/.exec(step)?.[0];
  return kind !== undefined && kind.length > 0 ? kind : undefined;
}

/** How a type card's key reads before its name: `interface`, `type`, `enum`, or `class`. */
export function keyTypeWord(key: Key): string {
  const kind = keyBlockKind(key);
  return kind !== undefined && isTypeKind(kind) ? typeKindWord(kind) : "class";
}

/** A member's name as written (`coupon?`, `[key]`, `()`), without `readonly`, which is drawn apart. */
export function memberName(member: TypeMember): string {
  const name =
    member.name ?? (member.kind === "call" ? "()" : member.kind === "construct" ? "new ()" : member.kind === "index" ? "[key]" : "");
  return `${name}${member.optional ? "?" : ""}`;
}

/**
 * What a member shows after its type: an enum member's written value (after ` = ` when it has a type
 * too), or ` = …` for a default (the graph records that one is written, not its text); null for none.
 */
export function memberValue(member: TypeMember): string | null {
  if (member.value !== undefined) {
    return member.type ? ` = ${member.value}` : member.value;
  }
  return member.default ? " = …" : null;
}

/** The view a new card of a block of `kind` opens in: Diagram (its fields) for a type declaration, else the page's lens. */
export function newCardView(kind: string): View | undefined {
  return declarationWord(kind) === undefined ? undefined : "diagram";
}

/**
 * What a card's body shows in `view`: Code, Flow, a Diagram node, a type card's fields, or an entrance's
 * handlers. An entrance is a node in Diagram view and its handlers card otherwise; a type declaration shows
 * its fields in Diagram and Flow; a class shows its fields in Diagram and keeps Flow and Code.
 */
export type CardBody = "code" | "flow" | "node" | "fields" | "handlers";

export function cardBody(key: Key, view: View): CardBody {
  if (key.startsWith("entrance:")) {
    return view === "diagram" ? "node" : "handlers";
  }
  const kind = keyBlockKind(key) ?? "";
  if (isTypeKind(kind) && (view === "diagram" || (view === "flow" && kind !== "class"))) {
    return "fields";
  }
  return view === "diagram" ? "node" : view;
}
