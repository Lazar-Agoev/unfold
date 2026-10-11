/**
 * `frame_cards` (design §6.12): gathers items of a page into frames, or updates frames by id: title,
 * description, members, look, icon, parent, direction, and whether it is collapsed. An item is in one
 * frame, so members come from no frame, from the frame itself, or from the parent it is nested in;
 * frames nest at most three deep and never in a cycle. A frame's title and description are a person's
 * words when a person wrote them, also when moving a frame out leaves theirs empty (it would go). The
 * result names each frame and what it holds, and notes shared code that is in no frame though code in
 * several frames calls it.
 */

import type { AgentFrameMember, AgentFrameResult, AgentFrameSpec, AgentItemResult, AgentItemStatus, FrameCardsOp } from "@unfold/desktop/agent-api";
import type { Frame, Page } from "../../../ipc/generated";
import { findCard, isFreeId } from "../document";
import { MAX_FRAME_DEPTH, addFrame, findFrame, frameChain, frameItems, frameOf, nestFrame, nestingRemoval, setFrameMembers, updateFrame } from "../frames";
import { cardName, guardPeopleText, guardRemoval, invalid, noteName, pageArrows, type OpContext } from "./context";

/** What an item of the page is, for frames' rules and results. */
function itemKind(page: Page, id: string): AgentFrameMember["kind"] | "group" | "link" | null {
  if (findCard(page, id)) {
    return "card";
  }
  if (findFrame(page, id)) {
    return "frame";
  }
  if ((page.resources ?? []).some((item) => item.id === id)) {
    return "resource";
  }
  if ((page.notes ?? []).some((item) => item.id === id)) {
    return "note";
  }
  if (page.groups.some((item) => item.id === id)) {
    return "group";
  }
  return page.links.some((item) => item.id === id) ? "link" : null;
}

/** Unfold's name for an item a frame holds: a card's, a frame's title, a resource card's name, a note's start. */
function memberName(ctx: OpContext, page: Page, id: string): string {
  const card = findCard(page, id);
  if (card) {
    return cardName(ctx, card);
  }
  const named = findFrame(page, id)?.title ?? (page.resources ?? []).find((item) => item.id === id)?.name;
  if (named !== undefined) {
    return named;
  }
  const note = (page.notes ?? []).find((item) => item.id === id);
  return note ? noteName(note) : id;
}

/** A frame's result entry: what it is, where it is nested, and what it holds. */
export function frameResult(ctx: OpContext, page: Page, frame: Frame, status: AgentItemStatus): AgentFrameResult {
  const parent = frameOf(page, frame.id);
  return {
    kind: "frame",
    id: frame.id,
    status,
    name: frame.title,
    look: frame.look,
    ...(frame.icon !== undefined ? { icon: frame.icon } : {}),
    ...(frame.direction !== undefined ? { direction: frame.direction } : {}),
    ...(frame.collapsed === true ? { collapsed: true } : {}),
    ...(parent ? { parent: parent.id } : {}),
    members: frame.members.flatMap((id) => {
      const kind = itemKind(page, id);
      return kind === "card" || kind === "frame" || kind === "resource" || kind === "note" ? [{ id, kind, name: memberName(ctx, page, id) }] : [];
    }),
  };
}

/** `“Request checks”`. */
function quoted(frame: Pick<Frame, "title">): string {
  return `“${frame.title}”`;
}

/** Refuses members a frame can't hold: missing items, step groups and links, itself, its own holders, and items in another frame. */
function checkMembers(ctx: OpContext, page: Page, spec: AgentFrameSpec, title: string, self: Frame | undefined): void {
  if (spec.members.length === 0) {
    throw invalid(`The frame “${title}” needs members: the ids of the cards, resource cards, notes, or frames on this page it holds.`);
  }
  const holders = self ? frameChain(page, self.id) : [];
  for (const id of spec.members) {
    const kind = itemKind(page, id);
    if (kind === null) {
      throw invalid(`There is no item with id “${id}” on this page. read_page lists the page's ids.`);
    }
    if (kind === "group" || kind === "link") {
      throw invalid(`“${id}” is a ${kind === "group" ? "step group" : "link"}: a frame holds cards, resource cards, notes, and frames.`);
    }
    if (id === self?.id || id === spec.id) {
      throw invalid(`The frame “${title}” can't hold itself.`);
    }
    const holder = holders.find((frame) => frame.id === id);
    if (holder) {
      throw invalid(`The frame “${title}” can't hold ${quoted(holder)}: ${quoted(holder)} holds it, and frames can't hold each other in a cycle.`);
    }
    const now = frameOf(page, id);
    if (now && now.id !== self?.id && now.id !== spec.parent) {
      throw invalid(
        `${memberName(ctx, page, id)} (id ${id}) is already in the frame ${quoted(now)} (id ${now.id}), and an item is in one frame. Leave it there, take it out of that frame first (frame_cards with its members without it), or nest “${title}” in ${quoted(now)} with parent.`,
      );
    }
  }
}

/** Refuses a parent that isn't a frame of the page, or that the frame holds. */
function checkParent(page: Page, spec: AgentFrameSpec, title: string, self: Frame | undefined): void {
  if (spec.parent === undefined || spec.parent === null) {
    return;
  }
  const parent = findFrame(page, spec.parent);
  if (!parent) {
    throw invalid(`There is no frame with id “${spec.parent}” on this page to nest “${title}” in. read_page lists the page's frames.`);
  }
  if (parent.id === self?.id || parent.id === spec.id || spec.members.includes(parent.id) || (self !== undefined && frameItems(page, self.id).includes(parent.id))) {
    throw invalid(`“${title}” can't be nested in ${quoted(parent)}: ${quoted(parent)} would be inside it, and frames can't hold each other in a cycle.`);
  }
}

/** Refuses a page whose frames would nest more than three deep. */
function checkDepth(page: Page, title: string): void {
  for (const frame of page.frames ?? []) {
    const depth = frameChain(page, frame.id).length + 1;
    if (depth > MAX_FRAME_DEPTH) {
      throw invalid(
        `Can't make the frame “${title}” as asked: ${quoted(frame)} would be ${depth} frames deep, and frames nest at most ${MAX_FRAME_DEPTH} deep (a zone, a box in it, and a box in that). Nest it in a frame higher up, or take a level out.`,
      );
    }
  }
}

/** Creates or updates one frame; returns its id and what happened to it. */
function applyFrameSpec(ctx: OpContext, page: Page, spec: AgentFrameSpec, replacePeopleText: boolean | undefined): { id: string; status: AgentItemStatus } {
  const title = spec.title.trim();
  if (title.length === 0) {
    throw invalid("A frame needs a title: a noun of 1 to 4 words, such as “Request checks” or “Order API”.");
  }
  const self = spec.id !== undefined ? findFrame(page, spec.id) : undefined;
  if (!self && spec.id !== undefined && !isFreeId(page, spec.id)) {
    const kind = itemKind(page, spec.id);
    throw invalid(
      kind !== null
        ? `“${spec.id}” is the id of a ${kind === "group" ? "step group" : kind} on this page. Give the frame another id, or leave id out.`
        : `“${spec.id}” can't name a frame: an id is letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`,
    );
  }
  const members = [...new Set(spec.members)];
  checkMembers(ctx, page, { ...spec, members }, title, self);
  checkParent(page, spec, title, self);
  const author = ctx.agent.author;
  const text = spec.description?.trim();
  const description = text !== undefined && text.length > 0 ? { description: { text, author } } : {};
  const shown = {
    ...(spec.look !== undefined ? { look: spec.look } : {}),
    ...(spec.icon !== undefined ? { icon: spec.icon } : {}),
    ...(spec.direction !== undefined ? { direction: spec.direction } : {}),
    ...(spec.collapsed !== undefined ? { collapsed: spec.collapsed } : {}),
  };
  let result: { id: string; status: AgentItemStatus };
  if (self) {
    if (self.title !== title) {
      guardPeopleText(`the title of the frame ${quoted(self)}`, self.author, replacePeopleText);
    }
    if (self.description && text !== undefined && self.description.text !== text) {
      guardPeopleText(`the description of the frame ${quoted(self)}`, self.description.author, replacePeopleText);
    }
    const before = JSON.stringify([self, frameOf(page, self.id)?.id]);
    updateFrame(page, self.id, { title, author, ...description, ...shown });
    if (spec.parent !== undefined) {
      // A frame it leaves empty goes, and with it what would dangle: a person's words among them stay theirs.
      guardRemoval(ctx, page, nestingRemoval(page, self.id, spec.parent), replacePeopleText);
      nestFrame(page, self.id, spec.parent);
    }
    setFrameMembers(page, self.id, members);
    const after = findFrame(page, self.id);
    result = { id: self.id, status: after && JSON.stringify([after, frameOf(page, self.id)?.id]) === before ? "kept" : "updated" };
  } else {
    const id = addFrame(page, {
      title,
      members,
      look: spec.look ?? "box",
      author,
      ...(spec.id !== undefined ? { id: spec.id } : {}),
      ...(spec.icon !== undefined ? { icon: spec.icon } : {}),
      ...(spec.direction !== undefined ? { direction: spec.direction } : {}),
      ...(spec.collapsed !== undefined ? { collapsed: spec.collapsed } : {}),
      ...(spec.parent !== undefined && spec.parent !== null ? { parent: spec.parent } : {}),
    });
    updateFrame(page, id, description);
    result = { id, status: "created" };
  }
  checkDepth(page, title);
  return result;
}

/** "server and worker", "server, worker, and cli". */
function listed(names: readonly string[]): string {
  return names.length <= 2 ? names.join(" and ") : `${names.slice(0, -1).join(", ")}, and ${names.at(-1) ?? ""}`;
}

/**
 * Shared code: cards in no frame that cards in two or more units call (a unit is a card's outermost
 * box, else its innermost frame). A frame of its own makes that clear.
 */
function sharedCode(ctx: OpContext, page: Page): string[] {
  if ((page.frames ?? []).length === 0) {
    return [];
  }
  const unit = (id: string) => {
    const chain = frameChain(page, id);
    return chain.findLast((frame) => frame.look === "box") ?? chain[0];
  };
  const arrows = pageArrows(ctx, page).filter((arrow) => arrow.kind !== "type");
  return page.cards.flatMap((card) => {
    if (frameOf(page, card.id)) {
      return [];
    }
    const units = [...new Set(arrows.filter((arrow) => arrow.target === card.id).flatMap((arrow) => unit(arrow.source) ?? []))];
    if (units.length < 2) {
      return [];
    }
    const callers = listed(units.map((frame) => frame.title));
    return [`${cardName(ctx, card)} is in no frame, and ${callers} ${units.length === 2 ? "both" : "all"} call it. Consider a frame of its own (icon library).`];
  });
}

export function applyFrameCards(page: Page, op: FrameCardsOp, ctx: OpContext): AgentItemResult[] {
  const before = new Map((page.frames ?? []).map((frame) => [frame.id, frameResult(ctx, page, frame, "removed")]));
  const applied = op.frames.map((spec) => applyFrameSpec(ctx, page, spec, op.replacePeopleText));
  ctx.warnings.push(...sharedCode(ctx, page));
  // Each frame as the whole call left it, then the frames it emptied (a frame without members goes).
  const results: AgentItemResult[] = applied.flatMap(({ id, status }) => {
    const frame = findFrame(page, id);
    return frame ? [frameResult(ctx, page, frame, status)] : [];
  });
  for (const [id, removed] of before) {
    if (!findFrame(page, id)) {
      results.push({ ...removed, members: [] });
    }
  }
  return results;
}
