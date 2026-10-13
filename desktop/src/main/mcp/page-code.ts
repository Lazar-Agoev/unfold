/**
 * What the compiler says about a page's cards (design §6.5, `read_page`): each card's model (its name,
 * location, Code, and Flow) and each type card's definition, and from them what the board draws
 * between cards: one arrow per call occurrence whose target has a card on the page, an entrance's
 * arrows to its handlers, and type arrows from a type card's field to the card of each type its
 * written type names. The rules are copies of the canvas's (`ui/src/canvas/arrows.ts`, `cardBody`,
 * and the store's `shownView`), so change both together: a page opens in
 * Diagram or Code, a card is compact or expanded in Diagram, a type declaration's card keeps its
 * fields in every view while a class's shows them compact (its Flow expanded, its Code in Code), and
 * arrows reach the first card of a key. Two differences are deliberate: calls are listed one by one
 * where a Diagram node merges them, and a block whose source changed reads its Flow's chips (the
 * canvas keeps the last verified Code, which only a window has).
 */

import type { ToolContext } from "./context";
import type { Card, CardView, EdgeKind, Page, ProjectRow, TypeDefinition, TypeMember, TypeText, View } from "./engine-types";
import { keyKind } from "./format";
import { ToolError } from "./results";

/** Block kinds of type declarations, whose cards show only their fields. */
const DECLARATION_KINDS: ReadonlySet<string> = new Set(["interface", "type_alias", "enum"]);

/** One card with what the engine says of it. */
export interface CardCode {
  readonly card: Card;
  /** The key it shows. */
  readonly key: string;
  readonly model: CardView | null;
  /** A type card's definition, when the card shows its fields. */
  readonly definition: TypeDefinition | null;
  /** Why its code can't be read now, when it can't. */
  readonly problem?: string;
}

/** An arrow the board draws for a call or a handler. */
interface CallArrow {
  readonly from: string;
  readonly to: string;
  readonly kind: EdgeKind | "handler";
  /** The call as written, or the handler as registered. */
  readonly text: string;
  readonly line: number | null;
  /** The occurrence and edge `read_call` takes (none for a handler). */
  readonly source?: string;
  readonly seq?: number;
}

/** A type arrow: from a field of a type card to the card of a type its written type names. */
interface TypeArrow {
  readonly from: string;
  readonly to: string;
  /** The field's name (`=` for an alias's aliased type), with `from <Base>` for an inherited one. */
  readonly field: string;
  readonly type: TypeText;
}

/** A page's cards with their code, and what the board draws from it. */
export interface PageCode {
  readonly cards: ReadonlyMap<string, CardCode>;
  readonly arrows: readonly CallArrow[];
  readonly typeArrows: readonly TypeArrow[];
}

/** The key a card shows. */
export function cardKey(card: Pick<Card, "ref">): string {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

/**
 * Whether a key's card shows only its fields, in every view, and takes no expansion: a type
 * declaration's (the canvas's `fieldsOnly`); a class's expands to its detail like a function's.
 */
export function fieldsOnlyKey(key: string): boolean {
  return DECLARATION_KINDS.has(keyKind(key));
}

/** The lens a page opens in: Code for a page whose default is Code, else Diagram (a legacy Flow default too). */
export function pageLens(page: Pick<Page, "defaultView">): "diagram" | "code" {
  return page.defaultView === "code" ? "code" : "diagram";
}

/** Whether a card shows its detail in Diagram: its own view, else the page's default, isn't Diagram (a legacy Code card is expanded). */
export function cardExpanded(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): boolean {
  return (card.view ?? page.defaultView) !== "diagram";
}

/** The view a card shows as its page opens (no lens picked): Code in the Code lens; in Diagram, Flow when expanded, else a compact node. */
function shownView(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): View {
  if (pageLens(page) === "code") {
    return "code";
  }
  return cardExpanded(page, card) ? "flow" : "diagram";
}

/** Whether a card shows a type card's fields (the canvas's `cardBody`): a type declaration always, a class while compact in Diagram. */
function showsFields(page: Pick<Page, "defaultView">, card: Pick<Card, "view">, key: string): boolean {
  return fieldsOnlyKey(key) || (keyKind(key) === "class" && shownView(page, card) === "diagram");
}

/** A failed read's reason, for the card's line. */
function why(error: unknown): string {
  if (error instanceof ToolError) {
    return error.message;
  }
  throw error;
}

async function loadCard(context: ToolContext, project: ProjectRow, page: Page, card: Card): Promise<CardCode> {
  const key = cardKey(card);
  const definition = showsFields(page, card, key)
    ? context.engine("type_definition", { project: project.id, key }).then(
        (found) => found,
        () => null,
      )
    : Promise.resolve(null);
  try {
    const model = await context.engine("card", { project: project.id, key });
    return { card, key, model, definition: await definition };
  } catch (error) {
    return { card, key, model: null, definition: await definition, problem: why(error) };
  }
}

/** The resolved occurrences a block card shows: its Code's call links, else its Flow's chips (as the canvas reads them). */
function occurrences(model: Extract<CardView, { type: "block" }>): { source: string; seq: number; target: string; kind: EdgeKind; text: string; line: number }[] {
  const code = model.code?.source;
  if (code?.state === "current") {
    return code.links.flatMap((link) =>
      link.resolution === "resolved" && link.target !== undefined ? [{ source: link.source, seq: 0, target: link.target, kind: link.kind, text: link.text, line: link.line }] : [],
    );
  }
  return (model.flow?.nodes ?? []).flatMap((node) =>
    (node.chips ?? []).flatMap((chip) =>
      chip.resolution === "resolved" && chip.target !== undefined ? [{ source: chip.source, seq: chip.seq, target: chip.target, kind: chip.kind, text: chip.name, line: chip.line }] : [],
    ),
  );
}

/** The call and handler arrows of one card. */
function cardArrows(code: CardCode, cardOf: (key: string) => string | undefined): CallArrow[] {
  const { model } = code;
  if (!model) {
    return [];
  }
  if (model.type === "entrance") {
    return model.targets.flatMap((target) => {
      const to = target.resolution === "resolved" && target.target !== null ? cardOf(target.target) : undefined;
      return to === undefined || to === code.card.id ? [] : [{ from: code.card.id, to, kind: "handler" as const, text: target.name ?? target.text, line: target.line }];
    });
  }
  const seen = new Set<string>();
  return occurrences(model).flatMap((occurrence) => {
    const to = cardOf(occurrence.target);
    const id = `${occurrence.source}\u0000${occurrence.seq}\u0000${to ?? ""}`;
    if (to === undefined || to === code.card.id || seen.has(id)) {
      return [];
    }
    seen.add(id);
    return [{ from: code.card.id, to, kind: occurrence.kind, text: occurrence.text, line: occurrence.line, source: occurrence.source, seq: occurrence.seq }];
  });
}

/** One field row of a type card: its name and written type (`=` for an alias's aliased type). */
function fieldRows(definition: TypeDefinition, shownBase: (base: TypeText) => boolean): { field: string; type: TypeText }[] {
  const member = (prefix: string) => (item: TypeMember) => (item.type ? [{ field: `${prefix}${item.name ?? `(${item.kind})`}`, type: item.type }] : []);
  return [
    ...(definition.aliased ? [{ field: "=", type: definition.aliased }] : []),
    ...definition.members.flatMap(member("")),
    // A base with its own card on the page draws its own fields' arrows.
    ...definition.inherited.flatMap((group) => (shownBase(group.from) ? [] : group.members.flatMap(member(`from ${group.from.text}: `)))),
  ];
}

/** The type arrows of one type card showing its fields: one per field and card its written type names. */
function cardTypeArrows(code: CardCode, cardOf: (key: string) => string | undefined): TypeArrow[] {
  const { definition } = code;
  if (!definition) {
    return [];
  }
  const declared = (type: TypeText) => type.spans.flatMap((span) => (span.style === "declared" && span.target !== undefined ? [span.target] : []));
  // A base is the type its text starts with (`Base<Item>` is Base).
  const shownBase = (base: TypeText) => {
    const [first] = base.spans;
    return first?.style === "declared" && first.target !== undefined && cardOf(first.target) !== undefined;
  };
  return fieldRows(definition, shownBase).flatMap(({ field, type }) => {
    const targets = new Set(declared(type).flatMap((key) => cardOf(key) ?? []));
    targets.delete(code.card.id);
    return [...targets].map((to) => ({ from: code.card.id, to, field, type }));
  });
}

/** Reads the cards' code of a page of `project` and works out what the board draws between them. */
export async function pageCode(context: ToolContext, project: ProjectRow, page: Page): Promise<PageCode> {
  const loaded = await Promise.all(page.cards.map((card) => loadCard(context, project, page, card)));
  const cards = new Map(loaded.map((code) => [code.card.id, code]));
  // A call or a type reaches the first card of its key, as the canvas draws it.
  const byKey = new Map<string, string>();
  for (const code of loaded) {
    if (!byKey.has(code.key)) {
      byKey.set(code.key, code.card.id);
    }
  }
  const cardOf = (key: string) => byKey.get(key);
  return {
    cards,
    arrows: loaded.flatMap((code) => cardArrows(code, cardOf)),
    typeArrows: loaded.flatMap((code) => cardTypeArrows(code, cardOf)),
  };
}
