/**
 * What the compiler says about a page's cards (`read_page`): each card's drawn flow (`card`, shaped by
 * the page's names, combined steps, and proposed steps and by the card's open paths; an expanded
 * card's agent text comes in the same answer) or its app or entrance, each fields card's type
 * definition, and from them the arrows the board draws between cards: `pageArrows`, the rule the
 * canvas draws by.
 */

import { cardShape, pageArrows, type PageArrow } from "../../shared/drawing";
import type { ToolContext } from "./context";
import type { Card, CardView, OverlayArgs, Page, ProjectRow, TypeDefinition, View } from "./engine-types";
import { ToolError } from "./results";

/** One card with what the engine says of it. */
export interface CardCode {
  readonly card: Card;
  /** The key it shows. */
  readonly key: string;
  readonly model: CardView | undefined;
  /** A type card's definition, when the card shows its fields. */
  readonly definition: TypeDefinition | undefined;
  /** An expanded card's drawn flow as agent text, when it draws a flow rather than fields. */
  readonly flow: string | null;
  /** Why its code can't be read now, when it can't. */
  readonly problem?: string;
}

/** A page's cards with their code, and the arrows the board draws between them. */
export interface PageCode {
  readonly cards: ReadonlyMap<string, CardCode>;
  readonly arrows: readonly PageArrow[];
}

/** The key a card shows. */
export function cardKey(card: Pick<Card, "ref">): string {
  return "block" in card.ref ? card.ref.block : card.ref.entrance;
}

/** How a card shows: its own view, else the page's default. */
export function cardView(page: Pick<Page, "defaultView">, card: Pick<Card, "view">): View {
  return card.view ?? page.defaultView;
}

/** The page's overlay as `card` takes it: its names, combined steps, and proposed steps. */
export function overlayArgs(page: Page): OverlayArgs {
  return {
    names: page.names.map(({ key, text }) => ({ key, text })),
    combined: page.combined.map(({ id, title, members, collapsed }) => ({ id, title, members, collapsed })),
    proposed: (page.proposals ?? []).map((proposal) => ({ id: proposal.id, title: proposal.title, ...("after" in proposal ? { after: proposal.after } : { start: proposal.start }) })),
  };
}

/** A failed read's reason, for the card's line. */
function why(error: unknown): string {
  if (error instanceof ToolError) {
    return error.message;
  }
  throw error;
}

/** A card's code, drawn once: an expanded card's agent text comes with its drawn flow. */
async function loadCard(context: ToolContext, project: ProjectRow, page: Page, card: Card): Promise<CardCode> {
  const key = cardKey(card);
  const view = cardView(page, card);
  try {
    const model = await context.engine("card", { project: project.id, key, overlay: overlayArgs(page), open: card.open ?? [], ...(view === "expanded" ? { agentText: true } : {}) });
    const fields = cardShape(model, view) === "fields";
    const definition = fields ? await context.engine("type_definition", { project: project.id, key }).catch(() => undefined) : undefined;
    const flow = model.type === "definition" && !fields ? (model.agentText?.text ?? null) : null;
    return { card, key, model, definition, flow };
  } catch (error) {
    return { card, key, model: undefined, definition: undefined, flow: null, problem: why(error) };
  }
}

/** Reads the cards' code of a page of `project` and works out the arrows the board draws between them. */
export async function pageCode(context: ToolContext, project: ProjectRow, page: Page): Promise<PageCode> {
  const loaded = await Promise.all(page.cards.map((card) => loadCard(context, project, page, card)));
  return {
    cards: new Map(loaded.map((code) => [code.card.id, code])),
    arrows: pageArrows(loaded.map(({ card, key, model, definition }) => ({ id: card.id, key, view: cardView(page, card), model, definition }))),
  };
}
