/**
 * How types read: a declaration's word (`interface Cart`, `type CartId`, `enum Currency`, `class Order`),
 * and a member's name and value as written.
 */

import type { TypeKind, TypeMember } from "../ipc/generated";

const DECLARATION_WORDS: Readonly<Record<string, string>> = { interface: "interface", type_alias: "type", enum: "enum" };

/** The kinds of block a type card shows. */
const TYPE_KINDS: ReadonlySet<string> = new Set<TypeKind>(["interface", "type_alias", "enum", "class"]);

/** How a type reads before its name: `interface`, `type`, `enum`, or `class`. */
export function typeKindWord(kind: TypeKind): string {
  return DECLARATION_WORDS[kind] ?? "class";
}

/** Whether a block of `kind` is shown as a type card: a type declaration or a class. */
export function isTypeKind(kind: string): kind is TypeKind {
  return TYPE_KINDS.has(kind);
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
