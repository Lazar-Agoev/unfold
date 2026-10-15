/**
 * Where an agent's new resource cards and notes go: beside the item they are `near` (or attached to),
 * else right of what a frame holds, else right of everything; never covering a card, a resource card, a
 * note, or a frame they aren't in.
 */

import { FOLLOW_GAP, STACK_GAP, clearSpot, newCardPlacement, type Point, type Rect, type Size } from "../../../canvas/layout";
import { NOTE_GAP, framePadding } from "../../../canvas/frames/geometry";
import { lineMiddle } from "../../../canvas/lines/paths";
import { lineNoteSpot } from "../../../canvas/notes/spots";
import type { Page } from "../../../ipc/generated";
import { findFrame, frameChain } from "../frames";
import { invalid, pageRects, pageRoutes, type OpContext } from "./context";

function meets(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width + STACK_GAP && b.x < a.x + a.width + STACK_GAP && a.y < b.y + b.height + STACK_GAP && b.y < a.y + a.height + STACK_GAP;
}

/** What an item going into `frame` (if any) keeps clear of: every drawn item but the frames it goes into. */
export function occupiedFor(page: Page, rects: ReadonlyMap<string, Rect>, frame: string | undefined): Rect[] {
  const within = new Set(frame !== undefined ? [frame, ...frameChain(page, frame).map((item) => item.id)] : []);
  return [...rects].flatMap(([id, rect]) => (within.has(id) ? [] : [rect]));
}

/** The rectangle of item `id` as drawn: a card, resource card, note, frame, or (a link) its line's middle as routed (`lineMiddle`). */
export function nearRect(ctx: OpContext, page: Page, rects: ReadonlyMap<string, Rect>, id: string): Rect {
  const link = page.links.find((item) => item.id === id);
  const route = link && rects.has(link.from) && rects.has(link.to) ? pageRoutes(ctx, page).get(link.id) : undefined;
  if (route) {
    return { ...lineMiddle(route.points), width: 0, height: 0 };
  }
  const rect = rects.get(id);
  if (!rect) {
    throw invalid(`There is no item with id “${id}” drawn on this page to place it beside (a collapsed frame may hide it). read_page lists the page's ids.`);
  }
  return rect;
}

/**
 * Where a new resource card of `size` goes: right of the item it is `near` (level with its top), else
 * right of what `frame` holds, else right of everything; then down until it covers nothing.
 */
export function resourceSpot(ctx: OpContext, page: Page, size: Size, near: string | undefined, frame: string | undefined): [number, number] {
  const rects = pageRects(ctx, page);
  const occupied = occupiedFor(page, rects, frame);
  let at: Point | undefined;
  if (near !== undefined) {
    const rect = nearRect(ctx, page, rects, near);
    at = { x: rect.x + rect.width + FOLLOW_GAP, y: rect.y };
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
 * Where a note of `size` goes beside item `id`: under it, else right of it, above it, or left of it,
 * whichever first covers nothing; else under it, moved down until it covers nothing. A link's note
 * sits beside the middle of its line, as Arrange puts it (`lineNoteSpot`), in the frames around its ends.
 */
export function noteSpot(ctx: OpContext, page: Page, size: Size, id: string): [number, number] {
  const rects = pageRects(ctx, page);
  const rect = nearRect(ctx, page, rects, id);
  const link = page.links.find((item) => item.id === id);
  if (link) {
    const around = new Set([...frameChain(page, link.from), ...frameChain(page, link.to)].map((frame) => frame.id));
    return lineNoteSpot(rect, size, [...rects].flatMap(([other, box]) => (around.has(other) ? [] : [box])));
  }
  // The frames around the item take the note in; the item itself is what it sits beside.
  const occupied = occupiedFor(page, rects, frameChain(page, id)[0]?.id).filter((other) => other !== rect);
  const below = { x: rect.x, y: rect.y + rect.height + NOTE_GAP };
  const spots: Point[] = [
    below,
    { x: rect.x + rect.width + NOTE_GAP, y: rect.y },
    { x: below.x, y: rect.y - NOTE_GAP - size.height },
    { x: rect.x - NOTE_GAP - size.width, y: rect.y },
  ];
  const free = spots.find((spot) => !occupied.some((other) => meets({ ...spot, ...size }, other)));
  return free ? [Math.round(free.x), Math.round(free.y)] : clearSpot(below, size, occupied);
}

/** Where a free note goes when it is near nothing: right of everything. */
export function freeNoteSpot(ctx: OpContext, page: Page, size: Size): [number, number] {
  const occupied = [...pageRects(ctx, page).values()];
  const [x, y] = newCardPlacement(occupied);
  return clearSpot({ x, y }, size, occupied);
}
