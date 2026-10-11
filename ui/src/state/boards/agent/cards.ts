/**
 * `add_cards`: puts functions, classes, types, and entrances on a page the way following a call places
 * them, and optionally their call trees (resolved calls only) or the types their fields name, saying
 * where following stopped. A card goes beside the item it is `near`, else beside a card that calls it,
 * whose field names it, or that it calls, else right of everything (of the frame it goes into); it
 * never covers another card, or a frame it isn't in. Cards already on the page are kept, not
 * duplicated. With `frame`, the cards it adds go into that frame.
 */

import type { AddCardsOp, AgentCardResult, AgentCardSpec, AgentFollowStop, AgentItemResult } from "@unfold/desktop/agent-api";
import { fieldRows, occurrencesOf } from "../../../canvas/arrows";
import { lastVerifiedCode } from "../../../canvas/cardData";
import { FOLLOW_GAP, HEADER_PORT, clearSpot, followPlacement, newCardPlacement, type Rect, type Size } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { descriptionHeight, estimatedFieldOffset, estimatedLineOffset } from "../../../canvas/sizing";
import { cardBody, isTypeKind, keyBlockKind, memberName } from "../../../code/typeKinds";
import type { Card, CardRef, Key, Page, ProjectId, ProjectName, TypeDefinition, View } from "../../../ipc/generated";
import { framePadding } from "../../../canvas/frames/geometry";
import { addCard, cardKey, findCard, findCardByRef, keyDisplayName, setCardView } from "../document";
import { addToFrame, findFrame, frameChain } from "../frames";
import { cardProject, cardResult, cardSize, invalid, pageRects, refProject, type ApplyOp, type OpContext } from "./context";
import type { AgentData } from "./data";

/** How deep following goes at most: what a card calls, what calls it, and the types its fields name. */
const MAX_CALLEES = 3;
const MAX_CALLERS = 2;
const MAX_TYPES = 2;
/** The most cards following adds for one card. */
const MAX_FOLLOWED = 40;

/**
 * One card following reaches: from which key, which way, how deep, and where it starts: the calling
 * line (callees) or the field whose written type names it (types, its `fieldRows` id).
 */
interface FollowStep {
  readonly from: Key;
  readonly to: Key;
  readonly direction: "callees" | "callers" | "types";
  readonly depth: number;
  readonly line?: number;
  readonly row?: string;
}

/** What following adds for one card, in order, where it stopped, and what the agent should know. */
interface FollowPlan {
  readonly steps: FollowStep[];
  readonly stops: Map<Key, AgentFollowStop[]>;
  readonly warnings: string[];
}

/** A level count clamped to 0–`max`. */
function levels(value: number | undefined, max: number): number {
  return Math.max(0, Math.min(max, Math.trunc(value ?? 0)));
}

/** Where following a type's fields stops at a written type name that isn't a declaration in the project. */
function typeStop(style: "external" | "builtin" | "unresolved", type: string, field: string | undefined, note: string | undefined): AgentFollowStop {
  return { reason: style, type, ...(field !== undefined ? { field } : {}), ...(note !== undefined ? { note } : {}) };
}

/**
 * The types the fields of type `key` name, in field order, each once with the first field that names it;
 * and where following stops there: each external, builtin, or unresolved name once (as written, with
 * the first field that writes it and its module or reason).
 */
function fieldTypes(definition: TypeDefinition): { reached: { target: Key; row: string }[]; blocked: AgentFollowStop[] } {
  const reached: { target: Key; row: string }[] = [];
  const blocked = new Map<string, AgentFollowStop>();
  for (const { row, member, type } of fieldRows(definition)) {
    for (const span of type?.spans ?? []) {
      const text = type?.text.slice(span.start, span.end) ?? "";
      if (span.style === "declared") {
        if (span.target !== undefined && !reached.some((item) => item.target === span.target)) {
          reached.push({ target: span.target, row });
        }
        continue;
      }
      const id = `${span.style}\u0000${text}`;
      if (!blocked.has(id)) {
        const field = member ? memberName(member) : undefined;
        blocked.set(id, typeStop(span.style, text, field, span.style === "unresolved" ? span.note : (span.module ?? span.note)));
      }
    }
  }
  return { reached, blocked: [...blocked.values()] };
}

/**
 * Follows resolved calls from `root` to the depths asked for, breadth first, and the types its fields
 * name (from a type card), loading every card it reaches.
 */
async function planFollow(data: AgentData, project: ProjectId, root: Key, follow: NonNullable<AgentCardSpec["follow"]>): Promise<FollowPlan> {
  const plan: FollowPlan = { steps: [], stops: new Map(), warnings: [] };
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
  const walkTypes = async (depthAsked: number) => {
    if (depthAsked > 0 && !(await data.loadDefinition(project, root))) {
      plan.warnings.push(`follow.types follows a type's fields, and ${keyDisplayName(root)} isn't a type declaration or a class, so no types were added for it.`);
      return;
    }
    let level = depthAsked > 0 ? [root] : [];
    for (let depth = 1; level.length > 0; depth += 1) {
      const next: Key[] = [];
      for (const key of level) {
        const definition = await data.loadDefinition(project, key);
        if (!definition) {
          continue;
        }
        const { reached, blocked } = fieldTypes(definition);
        if (depth <= depthAsked) {
          blocked.forEach((entry) => {
            stop(key, entry);
          });
        }
        const fresh = reached.filter((item) => !seen.has(item.target));
        if (depth > depthAsked) {
          if (fresh.length > 0) {
            stop(key, { reason: "depth", count: fresh.length });
          }
          continue;
        }
        const room = Math.max(0, MAX_FOLLOWED - plan.steps.length);
        for (const { target, row } of fresh.slice(0, room)) {
          seen.add(target);
          plan.steps.push({ from: key, to: target, direction: "types", depth, row });
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
  await walkTypes(levels(follow.types, MAX_TYPES));
  await Promise.all(plan.steps.map((step) => data.loadModel(project, step.to)));
  return plan;
}

/** Loads each card's model (and a type's definition) and follows its calls or types; returns how to apply the operation. */
export async function prepareAddCards(op: AddCardsOp, ctx: OpContext): Promise<ApplyOp> {
  const plans = new Map<AgentCardSpec, FollowPlan>();
  for (const spec of op.cards) {
    const project = spec.project ?? ctx.board.project;
    refProject(ctx, project);
    if (!(await ctx.data.loadModel(project, spec.key))) {
      const name = ctx.index.byId.get(project)?.name ?? project;
      throw invalid(`${name} has no function, type, class, or entrance with the key “${spec.key}”. Keys come from search, list_entrances, read_code, or get_relations.`);
    }
    if (isTypeKind(keyBlockKind(spec.key) ?? "")) {
      await ctx.data.loadDefinition(project, spec.key);
    }
    plans.set(spec, await planFollow(ctx.data, project, spec.key, spec.follow ?? {}));
  }
  return (page, applyCtx) => applyAddCards(page, op, plans, applyCtx);
}

/** The offset below a type card's top of its field's row `row`, when it shows its fields on `page`. */
function fieldOffset(ctx: OpContext, page: Page, card: Card, row: string): number | undefined {
  const definition = ctx.data.definition(cardProject(ctx, card), cardKey(card));
  if (!definition || cardBody(cardKey(card), card.view ?? page.defaultView) !== "fields") {
    return undefined;
  }
  return estimatedFieldOffset(definition, row, descriptionHeight(card.description, null, measureText));
}

/**
 * The offset below a card's top of the port where it calls `key`, or of its first field whose written
 * type names `key`; undefined when it does neither.
 */
function portToward(ctx: OpContext, page: Page, card: Card, key: Key): number | undefined {
  const project = cardProject(ctx, card);
  const model = ctx.data.model(project, cardKey(card));
  if (model?.type === "entrance") {
    return model.targets.some((target) => target.target === key) ? HEADER_PORT : undefined;
  }
  if (model?.type !== "block" || project === null) {
    return undefined;
  }
  const definition = ctx.data.definition(project, cardKey(card));
  const field = definition ? fieldRows(definition).find(({ type }) => type?.spans.some((span) => span.style === "declared" && span.target === key)) : undefined;
  if (field) {
    return fieldOffset(ctx, page, card, field.row) ?? HEADER_PORT;
  }
  const occurrence = occurrencesOf(model, lastVerifiedCode(project, cardKey(card))).find((item) => item.target === key);
  if (!occurrence) {
    return undefined;
  }
  const code = cardBody(cardKey(card), card.view ?? page.defaultView) === "code";
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

/** What a new card going into frame `frame` (if any) keeps clear of: every card and node, and the boxes of the frames it isn't in. */
function occupiedFor(page: Page, rects: ReadonlyMap<string, Rect>, frame: string | undefined): Rect[] {
  const within = new Set(frame !== undefined ? [frame, ...frameChain(page, frame).map((item) => item.id)] : []);
  return [...rects].flatMap(([id, rect]) => (within.has(id) ? [] : [rect]));
}

/**
 * Where a new card for `key` goes: beside `near`, else beside a related card, else right of everything
 * (inside `frame`, right of what it holds); never covering a card, or a frame it doesn't go into.
 */
function newCardSpot(ctx: OpContext, page: Page, ref: CardRef, near: string | undefined, size: Size, frame: string | undefined): [number, number] {
  const rects = pageRects(ctx, page);
  const occupied = occupiedFor(page, rects, frame);
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
  const box = frame !== undefined ? rects.get(frame) : undefined;
  const holder = frame !== undefined ? findFrame(page, frame) : undefined;
  if (!at && box && holder) {
    const padding = framePadding(holder);
    at = { x: box.x + box.width - padding.right + FOLLOW_GAP, y: box.y + padding.top };
  }
  if (!at) {
    const [x, y] = newCardPlacement(occupied);
    at = { x, y };
  }
  return clearSpot(at, size, occupied);
}

/**
 * Where a card following reached goes: right of its caller at the calling line, or of the type whose
 * field names it at that field's row; or left of its callee.
 */
function followedSpot(ctx: OpContext, page: Page, from: Card, step: FollowStep, size: Size, frame: string | undefined): [number, number] {
  const rects = pageRects(ctx, page);
  const rect = rects.get(from.id) ?? { x: from.at[0], y: from.at[1], ...cardSize(ctx, page, from) };
  const code = cardBody(cardKey(from), from.view ?? page.defaultView) === "code";
  const line = step.line !== undefined && code ? estimatedLineOffset(ctx.data.model(cardProject(ctx, from), cardKey(from)), step.line) : undefined;
  const field = step.row !== undefined ? fieldOffset(ctx, page, from, step.row) : undefined;
  const at = step.direction === "callers" ? leftOf(rect, size) : rightOf(rect, line ?? field ?? HEADER_PORT);
  return clearSpot(at, size, occupiedFor(page, rects, frame));
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
  const place = (ref: CardRef, at: [number, number], view: View | undefined) => {
    const id = addCard(page, { ref, at, ...(view !== undefined ? { view } : {}) });
    return { id, ref, at: [Math.round(at[0]), Math.round(at[1])] as const };
  };
  const sized = (ref: CardRef, view: View | undefined) => cardSize(ctx, page, { id: "", ref, ...(view !== undefined ? { view } : {}) });
  for (const spec of op.cards) {
    const named = refProject(ctx, spec.project ?? ctx.board.project);
    const ref = refOf(spec.key, named);
    const { view, frame } = spec;
    if (frame !== undefined && !findFrame(page, frame)) {
      throw invalid(`There is no frame with id “${frame}” on this page to put the cards in. read_page lists the page's frames; frame_cards makes one.`);
    }
    const into = frame !== undefined ? { frame } : {};
    const created: string[] = [];
    const existing = findCardByRef(page, spec.key, named);
    if (existing) {
      const changed = view !== undefined && existing.view !== view;
      if (changed) {
        setCardView(page, existing.id, view);
      }
      note(existing, cardResult(ctx, existing, changed ? "updated" : "kept"));
    } else {
      const card = place(ref, newCardSpot(ctx, page, ref, spec.near, sized(ref, view), frame), view);
      note(card, cardResult(ctx, card, "created", { at: card.at, ...into }));
      created.push(card.id);
    }
    const plan = plans.get(spec);
    ctx.warnings.push(...(plan?.warnings ?? []));
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
      // A type reached from a type's fields shows as the type it was followed from was asked to.
      const toRef = refOf(step.to, named);
      const toView = step.direction === "types" ? view : undefined;
      const card = place(toRef, followedSpot(ctx, page, from, step, sized(toRef, toView), frame), toView);
      note(card, cardResult(ctx, card, "created", { at: card.at, via, ...into }));
      created.push(card.id);
    }
    if (frame !== undefined) {
      addToFrame(page, frame, created);
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
