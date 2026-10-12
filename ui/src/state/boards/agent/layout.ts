/**
 * How a page shows and where its items are: `set_view` (each card's expansion in Diagram, the calls it
 * expands in place, and its fold; whether a step group or a frame is collapsed; the page's default
 * view) and `arrange` (Arrange from the inside out, `arrangePage`: each frame by call depth and
 * connections in its direction, type cards after the types that link to them; then the agent's moves
 * of cards and frames). Arrange reads the arrows from the cards' models and type definitions, and
 * sizes and ports from what the canvas measured, else estimates, so it works on a page no one has
 * shown.
 */

import type { AgentCallPath, AgentCardResult, AgentItemResult, AgentMove, AgentViewSpec, ArrangeOp, SetViewOp } from "@unfold/desktop/agent-api";
import { arrangePage, lineNoteSpots } from "../../../canvas/arrangePage";
import type { Arrow } from "../../../canvas/arrows";
import { FOLLOW_GAP, STACK_GAP, clearSpot } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { descriptionHeight, estimatedFieldOffset, estimatedLineOffset } from "../../../canvas/sizing";
import { cardBody, fieldsOnly } from "../../../code/typeKinds";
import type { Card, Key, Page } from "../../../ipc/generated";
import {
  callExpansions,
  cardKey,
  findCard,
  isCallExpanded,
  MAX_EXPANDED_CALLS,
  MAX_EXPANSION_DEPTH,
  moveItems,
  setCardExpanded,
  setCardView,
  setCardsFolded,
  setDefaultView,
  toggleCallExpansion,
  updateGroup,
} from "../document";
import { cardExpanded, shownView } from "../store";
import { findFrame, frameChain, frameItems, moveFrame, updateFrame } from "../frames";
import { findNote } from "../notes";
import { findResource } from "../resources";
import { AgentRefusal, cardName, cardProject, cardResult, invalid, itemSize, pageArrows, pageRects, type ApplyOp, type OpContext } from "./context";
import { checkCallPath, type ModelLookup } from "./expansions";
import { frameResult } from "./frames";
import { resourceResult } from "./resources";

/** The most models one call path's check loads: a model per level, and a callback's per level. */
const MAX_PATH_LOADS = 2 * MAX_EXPANSION_DEPTH + 1;

/** The models of a card's project, as the edit loaded them. */
function modelsOf(ctx: OpContext, card: Pick<Card, "ref">): ModelLookup {
  const project = cardProject(ctx, card);
  return (key) => (project === null ? null : ctx.data.loaded(project, key));
}

/**
 * `set_view`, ready to apply: the models each call path to expand goes through are loaded first (the
 * callees and callbacks of each level), so the paths are checked against them as the edit applies.
 */
export async function prepareSetView(op: SetViewOp, page: Page, ctx: OpContext): Promise<ApplyOp> {
  for (const item of op.items) {
    const card = findCard(page, item.id);
    const project = card ? cardProject(ctx, card) : null;
    if (!card || project === null || !("block" in card.ref)) {
      continue;
    }
    const models = modelsOf(ctx, card);
    for (const path of item.expandCalls ?? []) {
      for (let loads = 0; loads < MAX_PATH_LOADS; loads += 1) {
        const check = checkCallPath(card.ref.block, path, models);
        if (check.ok !== "load") {
          break;
        }
        await ctx.data.loadModel(project, check.key);
      }
    }
  }
  return (target, run) => applySetView(target, op, run);
}

/** A call path as a refusal names it: its keys joined with › . */
function pathText(path: readonly Key[]): string {
  return `[${path.join(" › ")}]`;
}

function samePath(a: readonly Key[], b: readonly Key[]): boolean {
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

/** Expands the call at `path` in place, with the levels above it; refused past the card's limit. */
function expandCall(card: Card, path: AgentCallPath, name: string): void {
  const paths = callExpansions(card).map((item) => [...item]);
  for (let depth = 1; depth <= path.length; depth += 1) {
    const level = path.slice(0, depth);
    if (!paths.some((item) => samePath(item, level))) {
      paths.push(level);
    }
  }
  if (paths.length > MAX_EXPANDED_CALLS) {
    throw invalid(`${name} would expand ${paths.length} calls in place, and a card expands at most ${MAX_EXPANDED_CALLS}. Collapse some (collapseCalls), or follow the calls to their own cards (add_cards).`);
  }
  card.expanded = paths;
}

/** Applies a card's part of `set_view`: its expansion (or legacy view), its fold, and the calls it expands or collapses in place. */
function setCardShow(ctx: OpContext, page: Page, card: Card, item: AgentViewSpec): AgentCardResult {
  const name = cardName(ctx, card);
  if (item.collapsed !== undefined) {
    throw invalid(`“${item.id}” is a card: it takes expanded, expandCalls, collapseCalls, and folded; collapsed is for step groups and frames.`);
  }
  // A type declaration's card keeps its fields in every view; a class's expands to its detail like a function's.
  const typeCard = fieldsOnly(cardKey(card));
  const calls = item.expandCalls !== undefined || item.collapseCalls !== undefined;
  if (typeCard && (item.expanded !== undefined || (item.view !== undefined && item.view !== "diagram") || calls)) {
    throw invalid(`${name} is a type card: it keeps its fields in every view, so it takes folded, not expanded or calls to expand. Expand a function's, a class's, or an entrance's card.`);
  }
  if (!("block" in card.ref) && calls) {
    throw invalid(`${name} is an entrance card: its targets aren't calls to expand in place. expanded: true shows its evidence and targets; follow a handler to its card (add_cards with follow: {callees: 1}) and expand calls there.`);
  }
  const before = JSON.stringify([card.view, card.folded, card.expanded]);
  const expanded = item.expanded ?? (item.view === undefined ? undefined : item.view !== "diagram");
  if (typeCard && item.view === "diagram") {
    setCardView(page, card.id, "diagram");
  } else if (expanded !== undefined) {
    setCardExpanded(page, card.id, expanded);
  }
  if (item.folded !== undefined) {
    setCardsFolded(page, [card.id], item.folded);
  }
  for (const path of item.collapseCalls ?? []) {
    if (path.length === 0 || path.length > MAX_EXPANSION_DEPTH) {
      throw invalid(`collapseCalls takes paths of 1 to ${MAX_EXPANSION_DEPTH} keys, as read_page lists ${name}'s expanded calls.`);
    }
    if (isCallExpanded(card, path)) {
      toggleCallExpansion(page, card.id, path);
    }
  }
  if ("block" in card.ref && (item.expandCalls ?? []).length > 0) {
    const root = card.ref.block;
    const models = modelsOf(ctx, card);
    for (const path of item.expandCalls ?? []) {
      const check = checkCallPath(root, path, models);
      if (check.ok === "load") {
        throw new AgentRefusal("page_changed", `${name} changed while the edit applied, and its calls weren't loaded. Read the page again, then try again.`);
      }
      if (!check.ok) {
        throw invalid(`Can't expand ${pathText(path)} in ${name}: ${check.reason}`);
      }
      expandCall(card, path, name);
    }
    if (!cardExpanded(card, page.defaultView)) {
      ctx.warnings.push(`${name} is compact: the calls it expands in place show once it's expanded (expanded: true).`);
    }
  }
  const status = JSON.stringify([card.view, card.folded, card.expanded]) === before ? "kept" : "updated";
  return cardResult(ctx, card, status, typeCard ? {} : { expanded: cardExpanded(card, page.defaultView), expandedCalls: callExpansions(card).map((path) => [...path]) });
}

export function applySetView(page: Page, op: SetViewOp, ctx: OpContext): AgentItemResult[] {
  // The page's default first, so each card's result says how it shows with it.
  if (op.defaultView !== undefined) {
    setDefaultView(page, op.defaultView);
  }
  return op.items.map((item): AgentItemResult => {
    const takesCards = item.view !== undefined || item.folded !== undefined || item.expanded !== undefined || item.expandCalls !== undefined || item.collapseCalls !== undefined;
    const group = page.groups.find((entry) => entry.id === item.id);
    if (group) {
      if (takesCards) {
        throw invalid(`“${item.id}” is a step group: it takes collapsed, not expanded, calls, view, or folded.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== group.collapsed;
      if (item.collapsed !== undefined) {
        updateGroup(page, group.id, { collapsed: item.collapsed });
      }
      return { kind: "group", id: group.id, status: changed ? "updated" : "kept", card: group.card, name: group.title };
    }
    const frame = findFrame(page, item.id);
    if (frame) {
      if (takesCards) {
        throw invalid(`“${item.id}” is a frame: it takes collapsed, not expanded, calls, view, or folded.`);
      }
      const changed = item.collapsed !== undefined && item.collapsed !== (frame.collapsed === true);
      if (item.collapsed !== undefined) {
        updateFrame(page, frame.id, { collapsed: item.collapsed });
      }
      return frameResult(ctx, page, frame, changed ? "updated" : "kept");
    }
    const card = findCard(page, item.id);
    if (!card && findResource(page, item.id)) {
      throw invalid(`“${item.id}” is a resource card: it keeps its columns in every view and takes no expansion. set_view takes cards, step groups, and frames.`);
    }
    if (!card) {
      throw invalid(`There is no card, step group, or frame with id “${item.id}” on this page. read_page lists the page's ids.`);
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
    const bodyOf = (id: string) => {
      const card = findCard(page, id);
      return card ? cardBody(cardKey(card), shownView(null, card, page.defaultView)) : undefined;
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
