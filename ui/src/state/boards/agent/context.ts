/**
 * What an agent's operations share as they apply to a page: the board, the agent, the data loaded
 * for them, refusals, the rule that people's words are safe and what those words are,
 * and how items are named, sized, and found on the page.
 */

import type { AgentBoardInfo, AgentCardResult, AgentCombinedResult, AgentFailure, AgentIdentity, AgentItemResult, AgentItemStatus } from "@unfold/desktop/agent-api";
import { cardShape } from "@unfold/desktop/drawing";
import { callRows, computeArrows, type Arrow } from "../../../canvas/arrows";
import type { BoardState, Card, Combined, Connection, Description, Note, Page, Resource } from "../../../ipc/generated";
import { estimatedResourceSize, fallbackItemSize, pageGeometry } from "../../../canvas/frames/geometry";
import { tableColumns, type TableColumn } from "../../../canvas/resources/columns";
import { RESOURCE_WORDS } from "../../../canvas/resources/kinds";
import type { Rect, Size } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { estimatedCardSize, portEstimator } from "../../../canvas/sizing";
import { routePage } from "../../../canvas/lines/pageLines";
import type { RoutedLine } from "../../../canvas/lines/route";
import { authorName } from "../../../code/text";
import { cardName as drawnName } from "../../cards";
import { cardKey, cardView, findCard, findCombined } from "../document";
import { findFrame } from "../frames";
import { findNote } from "../notes";
import { findProposal } from "../proposals";
import { findResource } from "../resources";
import type { BoardRef, CardSize } from "../types";
import type { AgentData } from "./data";

/** A note's words as a name: its start, cut at 40 characters. */
const NOTE_NAME = 40;

/** A refusal, as the window answers the main process. */
export function failure(code: string, message: string): AgentFailure {
  return { ok: false, code, message };
}

/** What the agent learns about a board (as held, or as saved): its title and file revision. */
export function boardInfo(board: Pick<BoardState, "document" | "revision">): AgentBoardInfo {
  return { title: board.document.title, revision: board.revision };
}

/** Why an operation can't apply, and what to do instead. Nothing of the edit applies. */
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

/** A text of a page that may be a person's words: what it is, as a person would find it, and who wrote it. */
export type Words = readonly [what: string, author: string | undefined];

/** `the table “orders”`. */
export function resourceName(resource: Pick<Resource, "kind" | "name">): string {
  return `the ${RESOURCE_WORDS[resource.kind]} “${resource.name}”`;
}

/** A note's name: its first 40 characters. */
export function noteName(note: Pick<Note, "text">): string {
  return note.text.length > NOTE_NAME ? `${note.text.slice(0, NOTE_NAME - 1)}…` : note.text;
}

/** A connection itself as someone's words: its author's. */
export function lineWords(link: Pick<Connection, "kind" | "author">): Words {
  return [`the ${link.kind} connection`, link.author];
}

function describedWords(what: string, description: Pick<Description, "author"> | undefined): Words[] {
  return description ? [[`the description of ${what}`, description.author]] : [];
}

/**
 * The words item `id` of a page carries, each with who wrote it: descriptions, labels,
 * combined step and frame titles, and a resource card, connection, note, or proposed step itself. An agent
 * replaces or removes none a person wrote unless the person asked (`guardPeopleText`).
 */
export function itemWords(page: Page, id: string, nameCard: (card: Card) => string): Words[] {
  const card = findCard(page, id);
  if (card) {
    return describedWords(nameCard(card), card.description);
  }
  const combined = findCombined(page, id);
  if (combined) {
    const what = `the combined step “${combined.title}”`;
    return [[what, combined.author], ...describedWords(what, combined.description)];
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    const line = lineWords(link);
    return [...(link.label ? [[`the label “${link.label.text}”`, link.label.author] as const] : []), line, ...describedWords(line[0], link.description)];
  }
  const resource = findResource(page, id);
  if (resource) {
    return [[resourceName(resource), resource.author], ...describedWords(resourceName(resource), resource.description)];
  }
  const note = findNote(page, id);
  if (note) {
    return [[`the note “${noteName(note)}”`, note.author]];
  }
  const proposal = findProposal(page, id);
  if (proposal) {
    return [[`the proposed step “${proposal.title}”`, proposal.author]];
  }
  const frame = findFrame(page, id);
  if (!frame) {
    return [];
  }
  const what = `the frame “${frame.title}”`;
  return [[what, frame.author], ...describedWords(what, frame.description)];
}

/** Refuses (`people_text`) to remove items `ids` of a page while any carries a person's words, unless the person asked. */
export function guardRemoval(ctx: OpContext, page: Page, ids: Iterable<string>, replacePeopleText: boolean | undefined): void {
  for (const id of ids) {
    for (const [what, author] of itemWords(page, id, (card) => cardName(ctx, card))) {
      guardPeopleText(what, author, replacePeopleText);
    }
  }
}

/** Unfold's name for a card, from its loaded card (`place_order`, `Place an order`); its key as written while there is none. */
export function cardName(ctx: OpContext, card: Pick<Card, "ref">): string {
  const model = ctx.data.model(ctx.board.project, cardKey(card));
  return model ? drawnName(model) : cardKey(card);
}

/** A combined step's result entry. */
export function combinedResult(combined: Combined, status: AgentItemStatus): AgentCombinedResult {
  return { kind: "combined", id: combined.id, status, name: combined.title, members: [...combined.members], collapsed: combined.collapsed };
}

/** A card's result entry. */
export function cardResult(ctx: OpContext, card: Pick<Card, "id" | "ref">, status: AgentItemStatus, extra: Partial<AgentCardResult> = {}): AgentCardResult {
  return {
    kind: "card",
    id: card.id,
    status,
    key: cardKey(card),
    name: cardName(ctx, card),
    ...extra,
  };
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
  const { project } = ctx.board;
  const key = cardKey(card);
  const definition = ctx.data.definition(project, key);
  return estimatedCardSize(
    {
      model: ctx.data.model(project, key),
      view: cardView(card, page.defaultView),
      ...(card.description ? { description: card.description } : {}),
      ...(definition ? { definition } : {}),
    },
    measureText,
  );
}

/** The page's arrows between cards (calls, starts, type arrows), from the loaded models, each card in the view it shows. */
/** Each card's ports below its top, estimated from what it drew (an agent's page may never have been shown): `portEstimator`. */
export function cardPorts(ctx: OpContext, page: Page): (card: string, port: string) => number | undefined {
  return portEstimator((id) => {
    const card = findCard(page, id);
    if (!card) {
      return undefined;
    }
    const key = cardKey(card);
    const model = ctx.data.model(ctx.board.project, key);
    return { model, shape: cardShape(model, cardView(card, page.defaultView)), description: card.description, definition: ctx.data.definition(ctx.board.project, key) };
  }, measureText);
}

/** The page's lines as the canvas routes them, from its document, the sizes the canvas measured (else estimated), and estimated ports (`routePage`). */
export function pageRoutes(ctx: OpContext, page: Page): Map<string, RoutedLine> {
  return routePage(page, (id) => itemSize(ctx, page, id), pageArrows(ctx, page), cardPorts(ctx, page));
}

export function pageArrows(ctx: OpContext, page: Page): Arrow[] {
  return computeArrows(
    page.cards.map((card) => ({
      id: card.id,
      key: cardKey(card),
      project: ctx.board.project,
      view: cardView(card, page.defaultView),
    })),
    ctx.data.loadedModels(),
    ctx.data.loadedDefinitions(),
  );
}

/** A table's columns as its card shows them: its type's fields (loaded for the edit) with its marks, or its own. */
export function resourceColumns(ctx: OpContext, resource: Resource): TableColumn[] {
  const definition = resource.type !== undefined ? ctx.data.definition(ctx.board.project, resource.type) : undefined;
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
  return pageGeometry(page, (id) => itemSize(ctx, page, id), null, callRows(pageArrows(ctx, page), cardPorts(ctx, page), page.links)).rects;
}
