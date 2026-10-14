/**
 * `shape_card`: a card's whole shaping as one operation — its
 * title (the page's name for its definition), its description, names on blocks, combined steps, and
 * the calls it opens in place; an app's or entrance's card, or a type card, which draw no flow, take only
 * a description. Names and combined steps apply page-wide, by key. It uses the edits
 * people's block menu uses, and the save checks every key and the overlay rules (combined steps are
 * consecutive steps of one body or one whole control block, never overlapping or nesting; open paths
 * are project calls or callbacks, at most 3 deep and 64 per card), so nothing here repeats a rule. New
 * words are the agent's; a person's are changed or removed only when the person asked
 * (`replacePeopleText`).
 */

import type { AgentCombineSpec, AgentItemResult, ShapeCardOp } from "@unfold/desktop/agent-api";
import type { Card, Combined, Page } from "../../../ipc/generated";
import { addCombined, cardKey, cardView, findCard, findCombined, openPaths, setCardDescription, setName, updateCombined } from "../document";
import { cardName, cardResult, combinedResult, guardPeopleText, invalid, type OpContext } from "./context";

/** Text an agent writes, trimmed; blank is refused. */
function words(text: string, what: string): string {
  const trimmed = text.trim();
  if (trimmed.length === 0) {
    throw invalid(`The ${what} is blank. Write it in a few words, or leave it out.`);
  }
  return trimmed;
}

/** Sets or takes off (null) the page's name for block `key`; a person's name changes only when asked. */
function nameBlock(ctx: OpContext, page: Page, key: string, text: string | null, replacePeopleText: boolean | undefined): void {
  const existing = page.names.find((name) => name.key === key);
  const next = text === null ? null : words(text, `name for “${key}”`);
  if (existing && existing.text !== next) {
    guardPeopleText(`the name “${existing.text}”`, existing.author, replacePeopleText);
  }
  setName(page, key, next ?? "", ctx.agent.author);
}

function sameMembers(combined: Combined, members: readonly string[]): boolean {
  return combined.members.length === members.length && combined.members.every((key, index) => key === members[index]);
}

/** Makes a combined step, or updates the page's one with exactly these blocks. */
function combine(ctx: OpContext, page: Page, spec: AgentCombineSpec, replacePeopleText: boolean | undefined): AgentItemResult {
  const title = words(spec.title, "combined step's title");
  const existing = page.combined.find((combined) => sameMembers(combined, spec.members));
  if (existing) {
    if (existing.title !== title) {
      guardPeopleText(`the combined step “${existing.title}”`, existing.author, replacePeopleText);
    }
    const before = JSON.stringify(existing);
    updateCombined(page, existing.id, { title, author: ctx.agent.author, ...(spec.collapsed !== undefined ? { collapsed: spec.collapsed } : {}) });
    return combinedResult(existing, JSON.stringify(existing) === before ? "kept" : "updated");
  }
  const id = addCombined(page, { members: spec.members, title, author: ctx.agent.author });
  if (spec.collapsed === false) {
    updateCombined(page, id, { collapsed: false });
  }
  const added = findCombined(page, id);
  if (!added) {
    throw invalid(`The combined step “${title}” couldn't be added.`);
  }
  return combinedResult(added, "created");
}

/** The card and the page's names, to tell whether shaping changed them. */
function snapshot(page: Page, card: Card): string {
  return JSON.stringify([card, page.names]);
}

/** Why a card takes only a description: it draws no flow (an app's or entrance's card, a type card); undefined for a card that draws one. */
function flowless(ctx: OpContext, card: Card, name: string): string | undefined {
  if (!("block" in card.ref)) {
    return `${name} is an app's or entrance's card: it draws no flow of its own, so shape_card writes only its description. Shape the cards of its code for the rest.`;
  }
  const model = ctx.data.model(ctx.board.project, cardKey(card));
  return model?.type === "definition" && model.flow.header.kind === "type"
    ? `${name} is a type card: it shows fields, not a flow, so shape_card writes only its description. Shape a function's, a class's, or a module's card for the rest.`
    : undefined;
}

export function applyShapeCard(page: Page, op: ShapeCardOp, ctx: OpContext): AgentItemResult[] {
  const card = findCard(page, op.card);
  if (!card) {
    throw invalid(`There is no card with id “${op.card}” on this page. read_page lists the page's cards with their ids.`);
  }
  const name = cardName(ctx, card);
  const onlyDescribed = flowless(ctx, card, name);
  if (onlyDescribed !== undefined && (op.title !== undefined || op.names !== undefined || op.combine !== undefined || op.open !== undefined)) {
    throw invalid(onlyDescribed);
  }
  const before = snapshot(page, card);
  if (op.description !== undefined) {
    const text = words(op.description, "description");
    if (card.description && card.description.text !== text) {
      guardPeopleText(`the description of ${name}`, card.description.author, op.replacePeopleText);
    }
    setCardDescription(page, card.id, text, ctx.agent.author);
  }
  if (onlyDescribed !== undefined) {
    return [cardResult(ctx, card, snapshot(page, card) === before ? "kept" : "updated")];
  }
  if (op.title !== undefined) {
    nameBlock(ctx, page, cardKey(card), op.title, op.replacePeopleText);
  }
  for (const entry of op.names ?? []) {
    nameBlock(ctx, page, entry.key, entry.text, op.replacePeopleText);
  }
  const combined = (op.combine ?? []).map((spec) => combine(ctx, page, spec, op.replacePeopleText));
  if (op.open !== undefined) {
    if (op.open.length > 0) {
      card.open = op.open.map((path) => [...path]);
    } else {
      delete card.open;
    }
    if (op.open.length > 0 && cardView(card, page.defaultView) === "compact") {
      ctx.warnings.push(`${name} is compact, so the calls it opens in place show once set_view expands it.`);
    }
  }
  const status = snapshot(page, card) === before ? "kept" : "updated";
  return [cardResult(ctx, card, status, { view: cardView(card, page.defaultView), open: openPaths(card).map((path) => [...path]) }), ...combined];
}
