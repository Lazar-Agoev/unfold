/**
 * Pure edits of a page's frames: named, described boxes around cards, resource cards,
 * notes, and other frames. An item is in at most one frame, frames never hold each other in a cycle,
 * and they nest at most three deep. A frame has no position of its own: its box is its members'
 * bounds, so moving a frame moves everything in it. Each function is an Immer recipe over a draft
 * page; optional fields are deleted rather than set to `undefined`, as the file omits them.
 *
 * A frame always holds at least one item: one left empty is removed with what would dangle
 * (`removeItems`), as the engine does.
 */

import type { Frame, FrameDirection, FrameIcon, FrameLook, Page } from "../../ipc/generated";
import { description, removalCascade, removeItems, uniqueId } from "./document";

/** A page's frames, as the queries read them. */
interface Framed {
  readonly frames?: readonly Frame[] | undefined;
}

export function findFrame(page: Framed, frameId: string): Frame | undefined {
  return (page.frames ?? []).find((frame) => frame.id === frameId);
}

/** The frame that holds item `id`, if any. */
export function frameOf(page: Framed, id: string): Frame | undefined {
  return (page.frames ?? []).find((frame) => frame.members.includes(id));
}

/** The frames that hold item `id`, innermost first (a cycle, which the file never has, ends the walk). */
export function frameChain(page: Framed, id: string): Frame[] {
  const chain: Frame[] = [];
  for (let holder = frameOf(page, id); holder && !chain.includes(holder); holder = frameOf(page, holder.id)) {
    chain.push(holder);
  }
  return chain;
}

/** Every item inside frame `frameId`, nested frames and what they hold included, in member order. */
export function frameItems(page: Framed, frameId: string): string[] {
  const items: string[] = [];
  const visit = (id: string) => {
    for (const member of findFrame(page, id)?.members ?? []) {
      if (!items.includes(member) && member !== frameId) {
        items.push(member);
        visit(member);
      }
    }
  };
  visit(frameId);
  return items;
}

/** Takes `id` out of the frame that holds it; a frame left empty goes, with what would dangle. */
function release(page: Page, id: string): void {
  const holder = frameOf(page, id);
  if (!holder) {
    return;
  }
  holder.members = holder.members.filter((member) => member !== id);
  if (holder.members.length === 0) {
    removeItems(page, [holder.id]);
  }
}

/** Puts `ids` in frame `frame` (at the end), each out of the frame that held it. */
function gather(page: Page, frame: Frame, ids: readonly string[]): void {
  for (const id of ids) {
    if (!frame.members.includes(id)) {
      release(page, id);
      frame.members.push(id);
    }
  }
}

/** Puts items in frame `frameId` (after what it holds), each out of the frame that held it. */
export function addToFrame(page: Page, frameId: string, ids: readonly string[]): void {
  const frame = findFrame(page, frameId);
  if (frame) {
    gather(page, frame, ids);
  }
}

/** What a new frame is. Its members are items of the page; each leaves the frame that held it. */
interface NewFrame {
  /** An id of its own choosing (the save refuses one the page has); absent: one from the title. */
  readonly id?: string;
  readonly title: string;
  readonly members: readonly string[];
  readonly look: FrameLook;
  readonly author: string;
  readonly icon?: FrameIcon;
  readonly direction?: FrameDirection;
  readonly collapsed?: boolean;
  /** The frame to nest it in: it takes the place of the first member it takes from there. */
  readonly parent?: string;
}

/** Adds a frame and returns its id. */
export function addFrame(page: Page, spec: NewFrame): string {
  const id = spec.id ?? uniqueId(page, spec.title.length > 0 ? spec.title : "frame");
  page.frames ??= [];
  page.frames.push({
    id,
    title: spec.title,
    members: [],
    look: spec.look,
    author: spec.author,
    ...(spec.icon !== undefined ? { icon: spec.icon } : {}),
    ...(spec.direction !== undefined ? { direction: spec.direction } : {}),
    ...(spec.collapsed === true ? { collapsed: true } : {}),
  });
  const parent = spec.parent !== undefined ? findFrame(page, spec.parent) : undefined;
  if (parent) {
    // Nested first, so a parent that gives it all its members isn't left empty.
    const first = parent.members.findIndex((member) => spec.members.includes(member));
    parent.members.splice(first < 0 ? parent.members.length : first, 0, id);
  }
  const frame = findFrame(page, id);
  if (frame) {
    gather(page, frame, spec.members);
  }
  return id;
}

/**
 * Makes `members` what frame `frameId` holds, in that order: new ones leave the frame that held
 * them, and the ones it no longer lists go to the frame that holds it (or out of every frame).
 */
export function setFrameMembers(page: Page, frameId: string, members: readonly string[]): void {
  const frame = findFrame(page, frameId);
  if (!frame || members.length === 0 || members.join("\u0000") === frame.members.join("\u0000")) {
    return;
  }
  const dropped = frame.members.filter((member) => !members.includes(member));
  const holder = frameOf(page, frameId);
  for (const id of members) {
    if (!frame.members.includes(id)) {
      release(page, id);
    }
  }
  frame.members = [...new Set(members)];
  if (holder && dropped.length > 0) {
    holder.members.splice(holder.members.indexOf(frameId) + 1, 0, ...dropped);
  }
}

/** Nests frame `frameId` in `parentId` (at the end), or takes it out of every frame (null). */
export function nestFrame(page: Page, frameId: string, parentId: string | null): void {
  const holder = frameOf(page, frameId);
  if (holder?.id === (parentId ?? undefined)) {
    return;
  }
  // In its new place first, so a holder it leaves empty can't take the new parent with it.
  if (parentId !== null) {
    findFrame(page, parentId)?.members.push(frameId);
  }
  if (holder) {
    holder.members = holder.members.filter((member) => member !== frameId);
    if (holder.members.length === 0) {
      removeItems(page, [holder.id]);
    }
  }
}

/**
 * What `nestFrame` takes with it (`removalCascade`): the frame that held `frameId` when it leaves that
 * frame empty, and what would dangle then; nothing otherwise.
 */
export function nestingRemoval(page: Page, frameId: string, parentId: string | null): Set<string> {
  const holder = frameOf(page, frameId);
  if (!holder || holder.id === parentId || holder.members.some((member) => member !== frameId)) {
    return new Set();
  }
  // Its new parent holds it by then, so a frame around both keeps something.
  const frames = (page.frames ?? []).map((frame) => (frame.id === parentId ? { ...frame, members: [...frame.members, frameId] } : frame));
  return removalCascade({ ...page, frames }, [holder.id]);
}

/** What changes about a frame. */
export interface FrameChange {
  readonly title?: string;
  /** Who wrote a new title (and so drew the frame as it is now). */
  readonly author?: string;
  /** Blank text clears the description. */
  readonly description?: { readonly text: string; readonly author: string };
  readonly look?: FrameLook;
  readonly icon?: FrameIcon;
  readonly direction?: FrameDirection;
  /** `collapsed` is written only when true. */
  readonly collapsed?: boolean;
}

export function updateFrame(page: Page, frameId: string, change: FrameChange): void {
  const frame = findFrame(page, frameId);
  if (!frame) {
    return;
  }
  if (change.title !== undefined && change.title.trim().length > 0 && change.title !== frame.title) {
    frame.title = change.title;
    if (change.author !== undefined) {
      frame.author = change.author;
    }
  }
  if (change.description) {
    if (change.description.text.trim().length === 0) {
      delete frame.description;
    } else if (frame.description?.text !== change.description.text) {
      frame.description = description(change.description.text, change.description.author);
    }
  }
  if (change.look !== undefined) {
    frame.look = change.look;
  }
  if (change.icon !== undefined) {
    frame.icon = change.icon;
  }
  if (change.direction !== undefined) {
    frame.direction = change.direction;
  }
  if (change.collapsed === true) {
    frame.collapsed = true;
  } else if (change.collapsed === false) {
    delete frame.collapsed;
  }
}

/**
 * Removes a frame. Its members stay, in the frame that held it (in its place), unless `keepMembers`
 * is false, which removes everything in it. Links to removed items go; notes attached to them stay
 * where they were. Returns the ids removed.
 */
export function removeFrame(page: Page, frameId: string, keepMembers = true): string[] {
  const frame = findFrame(page, frameId);
  if (!frame) {
    return [];
  }
  if (!keepMembers) {
    return removeItems(page, [frameId, ...frameItems(page, frameId)]);
  }
  const holder = frameOf(page, frameId);
  if (holder) {
    holder.members.splice(holder.members.indexOf(frameId), 1, ...frame.members);
  }
  return removeItems(page, [frameId]);
}

/**
 * What `removeFrame` takes (`removalCascade`): the frame, what would dangle, and everything in it unless
 * `keepMembers`. Kept members take its place in the frame that held it, which so keeps something.
 */
export function frameRemoval(page: Page, frameId: string, keepMembers: boolean): Set<string> {
  const frame = findFrame(page, frameId);
  if (!frame) {
    return new Set();
  }
  if (!keepMembers) {
    return removalCascade(page, [frameId, ...frameItems(page, frameId)]);
  }
  const frames = (page.frames ?? []).map((item) => (item.members.includes(frameId) ? { ...item, members: [...item.members, ...frame.members] } : item));
  return removalCascade({ ...page, frames }, [frameId]);
}

/** Moves a frame by `dx`, `dy`: every card, resource card, and unattached note in it moves with it. */
export function moveFrame(page: Page, frameId: string, dx: number, dy: number): void {
  const inside = new Set(frameItems(page, frameId));
  const placed = [...page.cards, ...(page.resources ?? []), ...(page.notes ?? []).filter((note) => note.attach === undefined)];
  for (const item of placed) {
    if (inside.has(item.id)) {
      item.at = [Math.round(item.at[0] + dx), Math.round(item.at[1] + dy)];
    }
  }
}
