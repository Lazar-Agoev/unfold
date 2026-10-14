/**
 * How a page shows and where its items are: `set_view` (each card compact or expanded; whether a
 * combined step or a frame is collapsed) and `arrange` (Arrange from the inside out, `arrangePage`:
 * each frame by call depth and connections in its direction, type cards after the types that link to
 * them; then the agent's moves of cards and frames). Arrange reads the arrows from the cards' models
 * and type definitions, and sizes and ports from what the canvas measured, else estimates, so it works
 * on a page no one has shown.
 */

import type { AgentCardResult, AgentItemResult, AgentMove, AgentViewSpec, ArrangeOp, SetViewOp } from "@unfold/desktop/agent-api";
import { arrangePage, lineNoteSpots } from "../../../canvas/arrangePage";
import type { Arrow } from "../../../canvas/arrows";
import { FOLLOW_GAP, STACK_GAP, clearSpot } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { descriptionHeight, estimatedFieldOffset } from "../../../canvas/sizing";
import type { Card, Page } from "../../../ipc/generated";
import { cardKey, cardView, findCard, findCombined, moveItems, openPaths, setCardView, updateCombined } from "../document";
import { findFrame, frameChain, frameItems, moveFrame, updateFrame } from "../frames";
import { findNote } from "../notes";
import { findResource } from "../resources";
import { cardName, cardResult, combinedResult, invalid, itemSize, pageArrows, pageRects, type OpContext } from "./context";
import { frameResult } from "./frames";
import { resourceResult } from "./resources";

/** Applies a card's part of `set_view`: compact or expanded. */
function setCardShow(ctx: OpContext, page: Page, card: Card, item: AgentViewSpec): AgentCardResult {
  const name = cardName(ctx, card);
  if (item.collapsed !== undefined) {
    throw invalid(`“${item.id}” is a card: it takes view (compact or expanded); collapsed is for combined steps and frames.`);
  }
  // A type's card keeps its fields in every view; a class's expands to its class-level code like a function's.
  const model = ctx.data.model(ctx.board.project, cardKey(card));
  const typeCard = model?.type === "definition" && model.flow.header.kind === "type";
  if (typeCard && item.view === "expanded") {
    throw invalid(`${name} is a type card: it keeps its fields in every view, so it doesn't expand. Expand a definition's, a class's, or an entrance's card.`);
  }
  const before = card.view;
  if (item.view !== undefined) {
    setCardView(page, card.id, item.view);
  }
  const status = card.view === before ? "kept" : "updated";
  return cardResult(ctx, card, status, typeCard ? {} : { view: cardView(card, page.defaultView), open: openPaths(card).map((path) => [...path]) });
}

export function applySetView(page: Page, op: SetViewOp, ctx: OpContext): AgentItemResult[] {
  return op.items.map((item): AgentItemResult => {
    const combined = findCombined(page, item.id);
    if (combined) {
      if (item.view !== undefined) {
        throw invalid(`“${item.id}” is a combined step: it takes collapsed, not view.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== combined.collapsed;
      if (item.collapsed !== undefined) {
        updateCombined(page, combined.id, { collapsed: item.collapsed });
      }
      return combinedResult(combined, changed ? "updated" : "kept");
    }
    const frame = findFrame(page, item.id);
    if (frame) {
      if (item.view !== undefined) {
        throw invalid(`“${item.id}” is a frame: it takes collapsed, not view.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== (frame.collapsed === true);
      if (item.collapsed !== undefined) {
        updateFrame(page, frame.id, { collapsed: item.collapsed });
      }
      return frameResult(ctx, page, frame, changed ? "updated" : "kept");
    }
    const card = findCard(page, item.id);
    if (!card && findResource(page, item.id)) {
      throw invalid(`“${item.id}” is a resource card: it keeps its columns in every view. set_view takes cards, combined steps, and frames.`);
    }
    if (!card) {
      throw invalid(`There is no card, combined step, or frame with id “${item.id}” on this page. read_page lists the page's ids.`);
    }
    return setCardShow(ctx, page, card, item);
  });
}

/** A card, resource card, note, or frame of the page, by id, or a refusal. */
function movable(page: Page, id: string): "item" | "frame" {
  if (findFrame(page, id)) {
    return "frame";
  }
  if (findCard(page, id) || findResource(page, id) || findNote(page, id)) {
    return "item";
  }
  throw invalid(`There is no card, resource card, note, or frame with id “${id}” on this page. read_page lists the page's ids.`);
}

/**
 * Applies one move: a card, resource card, or note goes to the position (an attached note stays
 * attached, at a new offset), a frame's box to it (everything in it moves along); beside another
 * item, it never covers what it isn't in.
 */
function applyMove(ctx: OpContext, page: Page, move: AgentMove): void {
  const kind = movable(page, move.id);
  const rects = pageRects(ctx, page);
  const self = rects.get(move.id);
  if (!self) {
    throw invalid(`“${move.id}” isn't drawn: a collapsed frame holds it. Expand that frame first (set_view), or move the frame.`);
  }
  let at: readonly [number, number];
  if ("at" in move) {
    at = move.at;
  } else {
    const otherId = "rightOf" in move ? move.rightOf : move.below;
    movable(page, otherId);
    const other = rects.get(otherId);
    if (!other || otherId === move.id) {
      throw invalid(`“${move.id}” can't move beside “${otherId}”${other ? "" : ": a collapsed frame holds it"}.`);
    }
    const spot = "rightOf" in move ? { x: other.x + other.width + FOLLOW_GAP, y: other.y } : { x: other.x, y: other.y + other.height + STACK_GAP };
    // What it is in (and holds) grows around it; everything else stays clear.
    const own = new Set([move.id, ...frameChain(page, move.id).map((frame) => frame.id), ...(kind === "frame" ? frameItems(page, move.id) : [])]);
    at = clearSpot(spot, self, [...rects].filter(([id]) => !own.has(id)).map(([, rect]) => rect));
  }
  if (kind === "frame") {
    moveFrame(page, move.id, at[0] - self.x, at[1] - self.y);
  } else {
    moveItems(page, { [move.id]: at });
  }
}

export function applyArrange(page: Page, op: ArrangeOp, ctx: OpContext): AgentItemResult[] {
  const before = new Map([...page.cards, ...(page.resources ?? [])].map((item) => [item.id, `${item.at[0]},${item.at[1]}`]));
  const frame = op.frame !== undefined ? findFrame(page, op.frame) : undefined;
  if (op.frame !== undefined && !frame) {
    throw invalid(`There is no frame with id “${op.frame}” on this page to arrange. read_page lists the page's frames.`);
  }
  const turned = frame !== undefined && op.direction !== undefined && op.direction !== frame.direction;
  if (frame && op.direction !== undefined) {
    updateFrame(page, frame.id, { direction: op.direction });
  }
  if (page.cards.length > 0 || (page.resources ?? []).length > 0) {
    // A field's row in a type card; else the header.
    const port = (arrow: Arrow) => {
      const card = findCard(page, arrow.source);
      if (!card || arrow.kind !== "type") {
        return undefined;
      }
      const definition = ctx.data.definition(arrow.project, cardKey(card));
      const row = arrow.sourceHandle.slice("field:".length);
      return definition ? estimatedFieldOffset(definition, row, descriptionHeight(card.description, null, measureText)) : undefined;
    };
    const positions = arrangePage({
      page,
      rects: pageRects(ctx, page),
      sizeOf: (id) => itemSize(ctx, page, id),
      arrows: pageArrows(ctx, page),
      portOffset: port,
      ...(frame ? { frame: frame.id } : op.direction !== undefined ? { direction: op.direction } : {}),
    });
    moveItems(page, Object.fromEntries([...positions].map(([id, at]) => [id, [at.x, at.y] as const])));
  }
  for (const move of op.moves ?? []) {
    applyMove(ctx, page, move);
  }
  // Then the notes on links (a frame's: those on its items' links), beside their lines as they now run.
  const held = frame ? new Set([frame.id, ...frameItems(page, frame.id)]) : undefined;
  const lines = held ? new Set(page.links.filter((link) => held.has(link.from) || held.has(link.to)).map((link) => link.id)) : undefined;
  moveItems(page, lineNoteSpots(page, (id) => itemSize(ctx, page, id), lines));
  const moved = [
    ...page.cards.flatMap((card) => (before.get(card.id) === `${card.at[0]},${card.at[1]}` ? [] : [cardResult(ctx, card, "updated", { at: [card.at[0], card.at[1]] })])),
    ...(page.resources ?? []).flatMap((resource) =>
      before.get(resource.id) === `${resource.at[0]},${resource.at[1]}` ? [] : [resourceResult(ctx, page, resource, "updated", { at: [resource.at[0], resource.at[1]] })],
    ),
  ];
  const arranged = frame ? findFrame(page, frame.id) : undefined;
  return turned && arranged ? [frameResult(ctx, page, arranged, "updated"), ...moved] : moved;
}
