/**
 * What an agent's operations share as they apply to a page: the board, the agent, the data loaded
 * for them, refusals, the rule that people's words are safe (design §6.1), and how cards are named,
 * sized, and found on the page.
 */

import type { AgentCardResult, AgentIdentity, AgentItemResult, AgentItemStatus } from "@unfold/desktop/agent-api";
import type { Card, Page, ProjectId } from "../../../ipc/generated";
import { fallbackSize, type Rect, type Size } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { entranceCardWidth, estimatedBlockSize } from "../../../canvas/sizing";
import { authorName } from "../../../code/text";
import { resolveRefProject, type ProjectIndex } from "../../workspace";
import { cardRects, entranceCardText, entranceTitle } from "../api";
import { cardKey, findCard, keyDisplayName } from "../document";
import type { BoardRef, CardSize } from "../types";
import type { AgentData } from "./data";

/** Why an operation can't apply, and what to do instead (design §7.8). Nothing of the edit applies. */
export class AgentRefusal extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "AgentRefusal";
  }
}

/** Refuses an operation whose arguments don't fit the page (`invalid_op`). */
export function invalid(message: string): AgentRefusal {
  return new AgentRefusal("invalid_op", message);
}

/** An operation ready to apply to a page (its data loaded): returns what it did, or throws an {@link AgentRefusal}. */
export type ApplyOp = (page: Page, ctx: OpContext) => AgentItemResult[];

/** What applying an agent's operations needs besides the page. */
export interface OpContext {
  readonly board: BoardRef;
  readonly agent: AgentIdentity;
  readonly data: AgentData;
  readonly index: ProjectIndex;
  /** The page's measured card sizes. */
  readonly measured: Readonly<Record<string, CardSize>>;
  /** Warnings for the result, added as operations apply. */
  readonly warnings: string[];
}

/** Whether text was written by a person: anything not signed by an agent (a title without an author too). */
function byPerson(author: string | undefined): boolean {
  return author?.startsWith("agent:") !== true;
}

/**
 * Refuses (`people_text`) to replace or remove `what`, which a person wrote, unless the call says the
 * person asked for it.
 */
export function guardPeopleText(what: string, author: string | undefined, replacePeopleText: boolean | undefined): void {
  if (replacePeopleText !== true && byPerson(author)) {
    const who = author === undefined ? "a person" : authorName(author);
    throw new AgentRefusal(
      "people_text",
      `Can't change ${what}: ${who} wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.`,
    );
  }
}

/** The project a card reads from, or null for a project that isn't connected. */
export function cardProject(ctx: Pick<OpContext, "index" | "board">, card: Pick<Card, "ref">): ProjectId | null {
  return resolveRefProject(ctx.index, ctx.board.project, card.ref.project);
}

/** Unfold's name for a card: `OrderService.place`, `POST /orders`. */
export function cardName(ctx: OpContext, card: Pick<Card, "ref">): string {
  const model = ctx.data.model(cardProject(ctx, card), cardKey(card));
  if (model?.type === "block") {
    return model.header.qualifiedName;
  }
  return model?.type === "entrance" ? entranceTitle(model) : keyDisplayName(cardKey(card));
}

/** A card's result entry. */
export function cardResult(ctx: OpContext, card: Pick<Card, "id" | "ref">, status: AgentItemStatus, extra: Partial<AgentCardResult> = {}): AgentCardResult {
  return {
    kind: "card",
    id: card.id,
    status,
    key: cardKey(card),
    project: cardProject(ctx, card) ?? ctx.board.project,
    name: cardName(ctx, card),
    ...extra,
  };
}

/** A card on the page, or a refusal naming what isn't there. */
export function cardOf(page: Page, id: string): Card {
  const card = findCard(page, id);
  if (!card) {
    throw invalid(`There is no card with id “${id}” on this page. read_page lists the page's ids.`);
  }
  return card;
}

/**
 * A card's size: as measured, else estimated from its model (a card not drawn yet, or a page no one
 * has shown), else a default.
 */
export function cardSize(ctx: OpContext, card: Pick<Card, "id" | "ref">): Size {
  const measured = ctx.measured[card.id];
  if (measured) {
    return measured;
  }
  const model = ctx.data.model(cardProject(ctx, card), cardKey(card));
  if (model?.type === "entrance") {
    return { width: entranceCardWidth(entranceCardText(model), measureText), height: fallbackSize(true).height };
  }
  return model ? estimatedBlockSize(model, card.ref.project !== undefined, measureText) : fallbackSize("entrance" in card.ref);
}

/** Every card's rectangle on the page as it would be drawn. */
export function pageRects(ctx: OpContext, page: Page): Map<string, Rect> {
  return cardRects(page.cards, (id) => {
    const card = findCard(page, id);
    return card ? cardSize(ctx, card) : fallbackSize(false);
  });
}
