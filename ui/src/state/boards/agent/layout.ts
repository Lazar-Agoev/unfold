/**
 * How a page shows and where its cards are: `set_view` (each card's view and fold, whether a step
 * group is collapsed, the page's default view) and `arrange` (today's Arrange by call depth, type cards
 * after the types that link to them, then the agent's moves). Arrange reads the arrows from the cards'
 * models and type definitions, and sizes and ports from what the canvas measured, else estimates, so it
 * works on a page no one has shown.
 */

import type { AgentItemResult, AgentMove, ArrangeOp, SetViewOp } from "@unfold/desktop/agent-api";
import { computeArrows, type Arrow } from "../../../canvas/arrows";
import { lastVerifiedCode } from "../../../canvas/cardData";
import { FOLLOW_GAP, STACK_GAP, arrangedPositions, clearSpot } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { descriptionHeight, estimatedFieldOffset, estimatedLineOffset } from "../../../canvas/sizing";
import { cardBody } from "../../../code/typeKinds";
import type { Card, Page } from "../../../ipc/generated";
import { cardKey, findCard, moveCards, setCardView, setCardsFolded, setDefaultView, updateGroup } from "../document";
import { cardOf, cardProject, cardResult, invalid, pageRects, type OpContext } from "./context";

export function applySetView(page: Page, op: SetViewOp, ctx: OpContext): AgentItemResult[] {
  const results = op.items.map((item): AgentItemResult => {
    const group = page.groups.find((entry) => entry.id === item.id);
    if (group) {
      if (item.view !== undefined || item.folded !== undefined) {
        throw invalid(`“${item.id}” is a step group: it takes collapsed, not view or folded.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== group.collapsed;
      if (item.collapsed !== undefined) {
        updateGroup(page, group.id, { collapsed: item.collapsed });
      }
      return { kind: "group", id: group.id, status: changed ? "updated" : "kept", card: group.card, name: group.title };
    }
    const card = findCard(page, item.id);
    if (!card) {
      throw invalid(`There is no card or step group with id “${item.id}” on this page. read_page lists the page's ids.`);
    }
    if (item.collapsed !== undefined) {
      throw invalid(`“${item.id}” is a card: it takes view and folded; collapsed is for step groups.`);
    }
    const before = JSON.stringify([card.view, card.folded]);
    if (item.view !== undefined) {
      setCardView(page, card.id, item.view);
    }
    if (item.folded !== undefined) {
      setCardsFolded(page, [card.id], item.folded);
    }
    return cardResult(ctx, card, JSON.stringify([card.view, card.folded]) === before ? "kept" : "updated");
  });
  if (op.defaultView !== undefined) {
    setDefaultView(page, op.defaultView);
  }
  return results;
}

/** Where a move puts a card: at a canvas position, or just right of or below another card (never covering one). */
function moveTarget(ctx: OpContext, page: Page, move: AgentMove): [number, number] {
  if ("at" in move) {
    return [move.at[0], move.at[1]];
  }
  const rects = pageRects(ctx, page);
  const otherId = "rightOf" in move ? move.rightOf : move.below;
  const other = rects.get(cardOf(page, otherId).id);
  const self = rects.get(move.id);
  if (!other || !self) {
    throw invalid(`“${move.id}” can't move beside “${otherId}”.`);
  }
  const at = "rightOf" in move ? { x: other.x + other.width + FOLLOW_GAP, y: other.y } : { x: other.x, y: other.y + other.height + STACK_GAP };
  const occupied = [...rects].filter(([id]) => id !== move.id).map(([, rect]) => rect);
  return clearSpot(at, self, occupied);
}

export function applyArrange(page: Page, op: ArrangeOp, ctx: OpContext): AgentItemResult[] {
  const before = new Map(page.cards.map((card) => [card.id, `${card.at[0]},${card.at[1]}`]));
  if (page.cards.length > 0) {
    const bodyOf = (card: Card) => cardBody(cardKey(card), card.view ?? page.defaultView);
    const arrows = computeArrows(
      page.cards.map((card) => ({ id: card.id, key: cardKey(card), project: cardProject(ctx, card), entrance: "entrance" in card.ref, body: bodyOf(card) })),
      ctx.data.loadedModels(),
      lastVerifiedCode,
      ctx.data.loadedDefinitions(),
    );
    // A calling line's row in a Code card, a field's row in a type card; else the header.
    const port = (arrow: Arrow) => {
      const card = findCard(page, arrow.source);
      if (!card) {
        return undefined;
      }
      if (arrow.kind === "type") {
        const definition = ctx.data.definition(arrow.project, cardKey(card));
        const row = arrow.sourceHandle.slice("field:".length);
        return definition ? estimatedFieldOffset(definition, row, descriptionHeight(card.description, null, measureText)) : undefined;
      }
      return bodyOf(card) === "code" && arrow.line !== null ? estimatedLineOffset(ctx.data.model(arrow.project, cardKey(card)), arrow.line) : undefined;
    };
    const positions = arrangedPositions(page.cards, pageRects(ctx, page), arrows, port);
    moveCards(page, Object.fromEntries([...positions].map(([id, at]) => [id, [at.x, at.y] as const])));
  }
  for (const move of op.moves ?? []) {
    cardOf(page, move.id);
    moveCards(page, { [move.id]: moveTarget(ctx, page, move) });
  }
  return page.cards.flatMap((card) =>
    before.get(card.id) === `${card.at[0]},${card.at[1]}` ? [] : [cardResult(ctx, card, "updated", { at: [card.at[0], card.at[1]] })],
  );
}
