/**
 * What the compiler says about a page's cards (design §6.5, `read_page`): each card's drawn flow
 * (`card`, shaped by the page's names, combined steps, and proposed steps and by the card's open
 * paths) or its app or entrance, each type card's definition, and from them what the board draws
 * between cards: one arrow per project call box whose callee has a card on the page, an app's or
 * entrance's arrows to the cards of its code, and type arrows from a type card's field to the card of
 * each type its written type names. A type's card shows its fields in every view, a class's while it
 * is compact, and arrows reach the first card of a key.
 */

import type { ToolContext } from "./context";
import type { CallBox, Card, CardView, DrawnFlow, OverlayArgs, Page, ProjectRow, Sequence, TypeDefinition, TypeMember, TypeText, View } from "./engine-types";
import { ToolError } from "./results";

/** One card with what the engine says of it. */
export interface CardCode {
  readonly card: Card;
  /** The key it shows. */
  readonly key: string;
  readonly model: CardView | null;
  /** A type card's definition, when the card shows its fields. */
  readonly definition: TypeDefinition | null;
  /** An expanded card's drawn flow as agent text (`flow_text`), when it draws a flow rather than fields. */
  readonly flow: string | null;
  /** Why its code can't be read now, when it can't. */
  readonly problem?: string;
}

/** An arrow the board draws for a call, or from an app or entrance to its code. */
interface CallArrow {
  readonly from: string;
  readonly to: string;
  /** The call as written, or the code's label. */
  readonly text: string;
  /** The call's block key, which `read_call` takes (none for an app's or entrance's code). */
  readonly call?: string;
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

/** How a card shows: its own view, else the page's default. */
export function cardView(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): View {
  return card.view ?? page.defaultView;
}

/** The page's overlay as `card` and `flow_text` take it: its names, combined steps, and proposed steps. */
function overlayArgs(page: Page): OverlayArgs {
  return {
    names: page.names.map(({ key, text }) => ({ key, text })),
    combined: page.combined.map(({ id, title, members, collapsed }) => ({ id, title, members, collapsed })),
    proposed: (page.proposals ?? []).map((proposal) => ({ id: proposal.id, title: proposal.title, ...("after" in proposal ? { after: proposal.after } : { start: proposal.start }) })),
  };
}

/** Whether a card shows a type card's fields: a type always, a class while compact. */
function showsFields(page: Pick<Page, "defaultView">, card: Pick<Card, "view">, model: CardView): boolean {
  if (model.type !== "definition") {
    return false;
  }
  const { kind } = model.flow.header;
  return kind === "type" || (kind === "class" && cardView(page, card) === "compact");
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
  const args = { project: project.id, key, overlay: overlayArgs(page), open: card.open ?? [] };
  try {
    const model = await context.engine("card", args);
    const fields = showsFields(page, card, model);
    const [definition, flow] = await Promise.all([
      fields ? context.engine("type_definition", { project: project.id, key }).catch(() => null) : null,
      model.type === "definition" && !fields && cardView(page, card) === "expanded" ? context.engine("flow_text", args).then((text) => text.text) : null,
    ]);
    return { card, key, model, definition, flow };
  } catch (error) {
    return { card, key, model: null, definition: null, flow: null, problem: why(error) };
  }
}

/** The call boxes a sequence draws, its frames' headers, tests, and lanes, and the open combined steps included, in order. */
function callBoxes(sequence: Sequence): CallBox[] {
  return sequence.steps.flatMap((step): CallBox[] => {
    switch (step.kind) {
      case "call":
        return [step];
      case "decision":
      case "loop":
      case "try":
      case "with":
        return [...step.headerCalls, ...step.lanes.flatMap((lane) => [...(lane.test?.calls ?? []), ...callBoxes(lane.steps)])];
      case "exit":
        return step.value?.kind === "call" ? [step.value] : [];
      case "combined":
        return step.steps ? callBoxes(step.steps) : [];
      case "thin":
      case "proposed":
        return [];
    }
  });
}

/** The project calls a drawn flow makes, by their call keys and callees. */
function projectCalls(flow: DrawnFlow): { call: string; text: string; target: string }[] {
  return callBoxes(flow.steps).flatMap((box) => (box.to.outcome === "project" && box.to.key !== undefined ? [{ call: box.key, text: box.callee, target: box.to.key }] : []));
}

/** The call and code arrows of one card. */
function cardArrows(code: CardCode, cardOf: (key: string) => string | undefined): CallArrow[] {
  const { model } = code;
  if (!model) {
    return [];
  }
  if (model.type === "start") {
    return model.code.flatMap((target) => {
      const to = target.state === "found" ? cardOf(target.key) : undefined;
      return to === undefined || to === code.card.id ? [] : [{ from: code.card.id, to, text: target.label ?? target.key }];
    });
  }
  return projectCalls(model.flow).flatMap((call) => {
    const to = cardOf(call.target);
    return to === undefined || to === code.card.id ? [] : [{ from: code.card.id, to, text: call.text, call: call.call }];
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

/** The project types a written type names, by key. */
function declared(type: TypeText): string[] {
  return type.spans.flatMap((span) => (span.to.outcome === "project" && span.to.key !== undefined ? [span.to.key] : []));
}

/** The type arrows of one type card showing its fields: one per field and card its written type names. */
function cardTypeArrows(code: CardCode, cardOf: (key: string) => string | undefined): TypeArrow[] {
  const { definition } = code;
  if (!definition) {
    return [];
  }
  // A base is the type its text starts with (`Base<Item>` is Base).
  const shownBase = (base: TypeText) => {
    const [first] = declared(base);
    return first !== undefined && cardOf(first) !== undefined;
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
