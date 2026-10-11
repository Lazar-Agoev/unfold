/**
 * `add_cards`: puts functions, classes, and entrances on a page the way following a call places
 * them, and optionally their call trees (resolved calls only), saying where following stopped. A card
 * goes beside the item it is `near`, else beside a card that calls it or that it calls, else right of
 * everything; it never covers another card. Cards already on the page are kept, not duplicated.
 */

import type { AddCardsOp, AgentCardResult, AgentCardSpec, AgentFollowStop, AgentItemResult } from "@unfold/desktop/agent-api";
import { occurrencesOf } from "../../../canvas/arrows";
import { lastVerifiedCode } from "../../../canvas/cardData";
import { FOLLOW_GAP, HEADER_PORT, clearSpot, followPlacement, newCardPlacement, type Rect, type Size } from "../../../canvas/layout";
import { estimatedLineOffset } from "../../../canvas/sizing";
import type { Card, CardRef, Key, Page, ProjectId, ProjectName } from "../../../ipc/generated";
import { refProjectFor } from "../../workspace";
import { addCard, cardKey, findCard, findCardByRef, setCardView } from "../document";
import { cardProject, cardResult, cardSize, invalid, pageRects, type ApplyOp, type OpContext } from "./context";
import type { AgentData } from "./data";

/** How deep following goes at most: what a card calls, and what calls it. */
const MAX_CALLEES = 3;
const MAX_CALLERS = 2;
/** The most cards following adds for one card. */
const MAX_FOLLOWED = 40;

/** One card following reaches: from which key, which way, how deep, and the calling line (callees). */
interface FollowStep {
  readonly from: Key;
  readonly to: Key;
  readonly direction: "callees" | "callers";
  readonly depth: number;
  readonly line?: number;
}

/** What following adds for one card, in order, and where it stopped. */
interface FollowPlan {
  readonly steps: FollowStep[];
  readonly stops: Map<Key, AgentFollowStop[]>;
}

/** A level count clamped to 0–`max`. */
function levels(value: number | undefined, max: number): number {
  return Math.max(0, Math.min(max, Math.trunc(value ?? 0)));
}

/** Follows resolved calls from `root` to the depths asked for, breadth first, loading every card it reaches. */
async function planFollow(data: AgentData, project: ProjectId, root: Key, follow: NonNullable<AgentCardSpec["follow"]>): Promise<FollowPlan> {
  const plan: FollowPlan = { steps: [], stops: new Map() };
  const stop = (key: Key, entry: AgentFollowStop) => {
    plan.stops.set(key, [...(plan.stops.get(key) ?? []), entry]);
  };
  const seen = new Set([root]);
  const walk = async (direction: FollowStep["direction"], depthAsked: number) => {
    let level = depthAsked > 0 ? [root] : [];
    for (let depth = 1; level.length > 0; depth += 1) {
      const next: Key[] = [];
      for (const key of level) {
        let targets: { readonly target: Key; readonly line?: number }[];
        if (direction === "callees") {
          const calls = await data.followableCalls(project, key);
          if (depth <= depthAsked) {
            calls.blocked.forEach((entry) => {
              stop(key, entry);
            });
          }
          targets = [...calls.resolved];
        } else {
          const callers = (await data.loadModel(project, key))?.type === "entrance" ? null : await data.callers(project, key);
          const unlisted = callers ? callers.total - callers.callers.length : 0;
          if (unlisted > 0 && depth <= depthAsked) {
            stop(key, { reason: "limit", count: unlisted });
          }
          targets = (callers?.callers ?? []).map((caller) => ({ target: caller.key }));
        }
        const fresh = targets.filter((item) => !seen.has(item.target));
        if (depth > depthAsked) {
          if (fresh.length > 0) {
            stop(key, { reason: "depth", count: fresh.length });
          }
          continue;
        }
        const room = Math.max(0, MAX_FOLLOWED - plan.steps.length);
        for (const { target, line } of fresh.slice(0, room)) {
          seen.add(target);
          plan.steps.push({ from: key, to: target, direction, depth, ...(line !== undefined ? { line } : {}) });
          next.push(target);
        }
        if (fresh.length > room) {
          stop(key, { reason: "limit", count: fresh.length - room });
        }
      }
      level = next;
    }
  };
  await walk("callees", levels(follow.callees, MAX_CALLEES));
  await walk("callers", levels(follow.callers, MAX_CALLERS));
  await Promise.all(plan.steps.map((step) => data.loadModel(project, step.to)));
  return plan;
}

/** The `ref.project` a card from `project` gets on this board, or a refusal. */
function refProject(ctx: OpContext, project: ProjectId): ProjectName | undefined {
  const row = ctx.index.byId.get(project);
  if (!row) {
    throw invalid(`No project with the id “${project}” is in the workspace. list_projects shows the projects and their ids.`);
  }
  const result = refProjectFor(ctx.index, ctx.board.project, project);
  if (!result.ok) {
    throw invalid(`${row.name} has the same name as this board's project, so its cards can't be added. Ask the person to rename one of them in its settings.`);
  }
  return result.project;
}

/** Loads each card's model and follows its calls; returns how to apply the operation. */
export async function prepareAddCards(op: AddCardsOp, ctx: OpContext): Promise<ApplyOp> {
  const plans = new Map<AgentCardSpec, FollowPlan>();
  for (const spec of op.cards) {
    const project = spec.project ?? ctx.board.project;
    refProject(ctx, project);
    if (!(await ctx.data.loadModel(project, spec.key))) {
      const name = ctx.index.byId.get(project)?.name ?? project;
      throw invalid(`${name} has no function, class, or entrance with the key “${spec.key}”. Keys come from search, list_entrances, read_code, or get_relations.`);
    }
    plans.set(spec, await planFollow(ctx.data, project, spec.key, spec.follow ?? {}));
  }
  return (page, applyCtx) => applyAddCards(page, op, plans, applyCtx);
}

/** The offset below a card's top of the port where it calls `key`, or undefined when it doesn't call it. */
function portToward(ctx: OpContext, page: Page, card: Card, key: Key): number | undefined {
  const project = cardProject(ctx, card);
  const model = ctx.data.model(project, cardKey(card));
  if (model?.type === "entrance") {
    return model.targets.some((target) => target.target === key) ? HEADER_PORT : undefined;
  }
  if (model?.type !== "block" || project === null) {
    return undefined;
  }
  const occurrence = occurrencesOf(model, lastVerifiedCode(project, cardKey(card))).find((item) => item.target === key);
  if (!occurrence) {
    return undefined;
  }
  const code = (card.view ?? page.defaultView) === "code";
  return (code ? estimatedLineOffset(model, occurrence.line) : undefined) ?? HEADER_PORT;
}

/** The point right of `rect`, with a card's header level with the port at `offset`. */
function rightOf(rect: Rect, offset: number) {
  const [x, y] = followPlacement(rect, offset);
  return { x, y };
}

/** The point left of `rect` for a card of `size`, level with its top. */
function leftOf(rect: Rect, size: Size) {
  return { x: rect.x - FOLLOW_GAP - size.width, y: rect.y };
}

/** The card an item of the page sits on: a card, a step group's card, or a link's start. */
function nearCard(page: Page, id: string): Card {
  const groupCard = page.groups.find((group) => group.id === id)?.card;
  const linkCard = page.links.find((link) => link.id === id)?.from;
  const card = findCard(page, groupCard ?? linkCard ?? id);
  if (!card) {
    throw invalid(`There is no item with id “${id}” on this page to place the card near. read_page lists the page's ids.`);
  }
  return card;
}

/** Where a new card for `key` goes: beside `near`, else beside a related card, else right of everything; never covering a card. */
function newCardSpot(ctx: OpContext, page: Page, ref: CardRef, near: string | undefined, size: Size): [number, number] {
  const rects = pageRects(ctx, page);
  const occupied = [...rects.values()];
  const key = cardKey({ ref });
  const sameProject = (card: Card) => card.ref.project === ref.project;
  const model = ctx.data.model(cardProject(ctx, { ref }), key);
  const calls = (card: Card) => model?.type === "block" && occurrencesOf(model, undefined).some((item) => item.target === cardKey(card));
  const beside = (card: Card) => {
    const rect = rects.get(card.id);
    if (!rect) {
      return undefined;
    }
    const port = sameProject(card) ? portToward(ctx, page, card, key) : undefined;
    if (port !== undefined) {
      return rightOf(rect, port);
    }
    return sameProject(card) && calls(card) ? leftOf(rect, size) : undefined;
  };
  let at: { x: number; y: number } | undefined;
  if (near !== undefined) {
    const card = nearCard(page, near);
    const rect = rects.get(card.id);
    at = beside(card) ?? (rect ? rightOf(rect, HEADER_PORT) : undefined);
  } else {
    for (const card of page.cards) {
      at ??= beside(card);
    }
  }
  if (!at) {
    const [x, y] = newCardPlacement(occupied);
    at = { x, y };
  }
  return clearSpot(at, size, occupied);
}

/** Where a card following reached goes: right of its caller at the calling line, or left of its callee. */
function followedSpot(ctx: OpContext, page: Page, from: Card, step: FollowStep, size: Size): [number, number] {
  const rects = pageRects(ctx, page);
  const rect = rects.get(from.id) ?? { x: from.at[0], y: from.at[1], ...cardSize(ctx, from) };
  const code = (from.view ?? page.defaultView) === "code";
  const offset =
    step.line !== undefined && code ? (estimatedLineOffset(ctx.data.model(cardProject(ctx, from), cardKey(from)), step.line) ?? HEADER_PORT) : HEADER_PORT;
  const at = step.direction === "callees" ? rightOf(rect, offset) : leftOf(rect, size);
  return clearSpot(at, size, [...rects.values()]);
}

function refOf(key: Key, project: ProjectName | undefined): CardRef {
  const named = project !== undefined ? { project } : {};
  return key.startsWith("entrance:") ? { entrance: key, ...named } : { block: key, ...named };
}

function applyAddCards(page: Page, op: AddCardsOp, plans: ReadonlyMap<AgentCardSpec, FollowPlan>, ctx: OpContext): AgentItemResult[] {
  const results = new Map<string, AgentCardResult>();
  const note = (card: Pick<Card, "id">, entry: AgentCardResult) => {
    if (!results.has(card.id)) {
      results.set(card.id, entry);
    }
  };
  // A plain copy of what was added: the page is a draft that doesn't outlive the edit.
  const place = (ref: CardRef, at: [number, number]) => ({ id: addCard(page, { ref, at }), ref, at: [Math.round(at[0]), Math.round(at[1])] as const });
  for (const spec of op.cards) {
    const named = refProject(ctx, spec.project ?? ctx.board.project);
    const ref = refOf(spec.key, named);
    const { view } = spec;
    const existing = findCardByRef(page, spec.key, named);
    if (existing) {
      const changed = view !== undefined && existing.view !== view;
      if (changed) {
        setCardView(page, existing.id, view);
      }
      note(existing, cardResult(ctx, existing, changed ? "updated" : "kept"));
    } else {
      const card = place(ref, newCardSpot(ctx, page, ref, spec.near, cardSize(ctx, { id: "", ref })));
      if (view !== undefined) {
        setCardView(page, card.id, view);
      }
      note(card, cardResult(ctx, card, "created", { at: card.at }));
    }
    const plan = plans.get(spec);
    for (const step of plan?.steps ?? []) {
      const from = findCardByRef(page, step.from, named);
      if (!from) {
        continue;
      }
      const via = { from: from.id, direction: step.direction, depth: step.depth };
      const reached = findCardByRef(page, step.to, named);
      if (reached) {
        note(reached, cardResult(ctx, reached, "kept", { via }));
        continue;
      }
      const toRef = refOf(step.to, named);
      const card = place(toRef, followedSpot(ctx, page, from, step, cardSize(ctx, { id: "", ref: toRef })));
      note(card, cardResult(ctx, card, "created", { at: card.at, via }));
    }
    for (const [key, stops] of plan?.stops ?? []) {
      const card = findCardByRef(page, key, named);
      const entry = card ? results.get(card.id) : undefined;
      if (card && entry) {
        results.set(card.id, { ...entry, stops: [...(entry.stops ?? []), ...stops] });
      }
    }
  }
  return [...results.values()];
}
