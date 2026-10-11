/**
 * What an agent's operations share as they apply to a page: the board, the agent, the data loaded
 * for them, refusals, the rule that people's words are safe (design §6.1), and how cards are named,
 * sized, and found on the page.
 */

import type { AgentCardResult, AgentIdentity, AgentItemResult, AgentItemStatus } from "@unfold/desktop/agent-api";
import { computeArrows, type Arrow } from "../../../canvas/arrows";
import { lastVerifiedCode } from "../../../canvas/cardData";
import { cardBody } from "../../../code/typeKinds";
import type { Card, Page, ProjectId, ProjectName, Resource } from "../../../ipc/generated";
import { estimatedResourceSize, fallbackItemSize, pageGeometry } from "../../../canvas/frames/geometry";
import { tableColumns, type TableColumn } from "../../../canvas/resources/columns";
import type { Rect, Size } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { estimatedCardSize } from "../../../canvas/sizing";
import { authorName } from "../../../code/text";
import { refProjectFor, resolveRefProject, type ProjectIndex } from "../../workspace";
import { entranceTitle } from "../api";
import { cardKey, findCard, keyDisplayName } from "../document";
import { findResource } from "../resources";
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

/** The name a board file gives `project` (a `ref.project`, a resource's or an anchor's `project`): undefined for the board's own project, or a refusal. */
export function refProject(ctx: Pick<OpContext, "index" | "board">, project: ProjectId): ProjectName | undefined {
  const row = ctx.index.byId.get(project);
  if (!row) {
    throw invalid(`No project with the id “${project}” is in the workspace. list_projects shows the projects and their ids.`);
  }
  const result = refProjectFor(ctx.index, ctx.board.project, project);
  if (!result.ok) {
    throw invalid(`${row.name} has the same name as this board's project, so nothing of it can be added. Ask the person to rename one of them in its settings.`);
  }
  return result.project;
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
 * A card's size: as measured, else estimated from its model in the view it shows on `page` (a card not
 * drawn yet, or a page no one has shown), else a default.
 */
export function cardSize(ctx: OpContext, page: Pick<Page, "defaultView">, card: Pick<Card, "id" | "ref" | "view" | "description">): Size {
  const measured = ctx.measured[card.id];
  if (measured) {
    return measured;
  }
  const project = cardProject(ctx, card);
  const key = cardKey(card);
  const definition = ctx.data.definition(project, key);
  return estimatedCardSize(
    {
      key,
      model: ctx.data.model(project, key),
      view: card.view ?? page.defaultView,
      monogram: card.ref.project !== undefined,
      ...(card.description ? { description: card.description } : {}),
      ...(definition ? { definition } : {}),
    },
    measureText,
  );
}

/** The page's arrows between cards (calls, handlers, type arrows), from the loaded models, each card in the view it shows. */
export function pageArrows(ctx: OpContext, page: Page): Arrow[] {
  return computeArrows(
    page.cards.map((card) => ({
      id: card.id,
      key: cardKey(card),
      project: cardProject(ctx, card),
      entrance: "entrance" in card.ref,
      body: cardBody(cardKey(card), card.view ?? page.defaultView),
    })),
    ctx.data.loadedModels(),
    lastVerifiedCode,
    ctx.data.loadedDefinitions(),
  );
}

/** The project a resource card's type is in (its `project`, else the board's own), or null when it isn't connected. */
export function resourceProject(ctx: Pick<OpContext, "index" | "board">, resource: Pick<Resource, "project">): ProjectId | null {
  return resolveRefProject(ctx.index, ctx.board.project, resource.project);
}

/** A table's columns as its card shows them: its type's fields (loaded for the edit) with its marks, or its own. */
export function resourceColumns(ctx: OpContext, resource: Resource): TableColumn[] {
  const definition = resource.type !== undefined ? ctx.data.definition(resourceProject(ctx, resource), resource.type) : undefined;
  return tableColumns(resource, definition);
}

/**
 * An item's size: a card's (`cardSize`), else as measured, else estimated (a collapsed frame's node, a
 * resource card with its columns, a note).
 */
export function itemSize(ctx: OpContext, page: Page, id: string): Size {
  const card = findCard(page, id);
  if (card) {
    return cardSize(ctx, page, card);
  }
  const measured = ctx.measured[id];
  const resource = measured ? undefined : findResource(page, id);
  return measured ?? (resource ? estimatedResourceSize(resource, resourceColumns(ctx, resource)) : fallbackItemSize(page, id));
}

/** Every drawn item's rectangle on the page as it would be drawn: cards, frames' boxes, collapsed frames' nodes. */
export function pageRects(ctx: OpContext, page: Page): ReadonlyMap<string, Rect> {
  return pageGeometry(page, (id) => itemSize(ctx, page, id)).rects;
}
