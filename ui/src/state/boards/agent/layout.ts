/**
 * How a page shows and where its items are: `set_view` (each card's view and fold, whether a step
 * group or a frame is collapsed, the page's default view) and `arrange` (Arrange from the inside out,
 * `arrangePage`: each frame by call depth and connections in its direction, type cards after the
 * types that link to them; then the agent's moves of cards and frames). Arrange reads the arrows from
 * the cards' models and type definitions, and sizes and ports from what the canvas measured, else
 * estimates, so it works on a page no one has shown.
 */

import type { AgentItemResult, AgentMove, ArrangeOp, SetViewOp } from "@unfold/desktop/agent-api";
import { arrangePage } from "../../../canvas/arrangePage";
import type { Arrow } from "../../../canvas/arrows";
import { FOLLOW_GAP, STACK_GAP, clearSpot } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { descriptionHeight, estimatedFieldOffset, estimatedLineOffset } from "../../../canvas/sizing";
import { cardBody } from "../../../code/typeKinds";
import type { Page } from "../../../ipc/generated";
import { cardKey, findCard, moveItems, setCardView, setCardsFolded, setDefaultView, updateGroup } from "../document";
import { findFrame, frameChain, frameItems, moveFrame, updateFrame } from "../frames";
import { cardResult, invalid, itemSize, pageArrows, pageRects, type OpContext } from "./context";
import { frameResult } from "./frames";

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
    const frame = findFrame(page, item.id);
    if (frame) {
      if (item.view !== undefined || item.folded !== undefined) {
        throw invalid(`“${item.id}” is a frame: it takes collapsed, not view or folded.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== (frame.collapsed === true);
      if (item.collapsed !== undefined) {
        updateFrame(page, frame.id, { collapsed: item.collapsed });
      }
      return frameResult(ctx, page, frame, changed ? "updated" : "kept");
    }
    const card = findCard(page, item.id);
    if (!card) {
      throw invalid(`There is no card, step group, or frame with id “${item.id}” on this page. read_page lists the page's ids.`);
    }
    if (item.collapsed !== undefined) {
      throw invalid(`“${item.id}” is a card: it takes view and folded; collapsed is for step groups and frames.`);
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

/** A card or a frame of the page, by id, or a refusal. */
function movable(page: Page, id: string): "card" | "frame" {
  if (findCard(page, id)) {
    return "card";
  }
  if (findFrame(page, id)) {
    return "frame";
  }
  throw invalid(`There is no card or frame with id “${id}” on this page. read_page lists the page's ids.`);
}

/**
 * Applies one move: a card goes to the position, a frame's box to it (everything in it moves along);
 * beside another card or frame, it never covers what it isn't in.
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
  const before = new Map(page.cards.map((card) => [card.id, `${card.at[0]},${card.at[1]}`]));
  const frame = op.frame !== undefined ? findFrame(page, op.frame) : undefined;
  if (op.frame !== undefined && !frame) {
    throw invalid(`There is no frame with id “${op.frame}” on this page to arrange. read_page lists the page's frames.`);
  }
  const turned = frame !== undefined && op.direction !== undefined && op.direction !== (frame.direction ?? "right");
  if (frame && op.direction !== undefined) {
    updateFrame(page, frame.id, { direction: op.direction });
  }
  if (page.cards.length > 0) {
    const bodyOf = (id: string) => {
      const card = findCard(page, id);
      return card ? cardBody(cardKey(card), card.view ?? page.defaultView) : undefined;
    };
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
      return bodyOf(card.id) === "code" && arrow.line !== null ? estimatedLineOffset(ctx.data.model(arrow.project, cardKey(card)), arrow.line) : undefined;
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
  const moved = page.cards.flatMap((card) =>
    before.get(card.id) === `${card.at[0]},${card.at[1]}` ? [] : [cardResult(ctx, card, "updated", { at: [card.at[0], card.at[1]] })],
  );
  const arranged = frame ? findFrame(page, frame.id) : undefined;
  return turned && arranged ? [frameResult(ctx, page, arranged, "updated"), ...moved] : moved;
}
