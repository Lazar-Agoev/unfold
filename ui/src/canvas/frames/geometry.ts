/**
 * Where a page's frames and notes are. A frame has no position of its own: its box
 * is the bounds of what it holds plus room for its title, computed from the inside out, so it follows
 * its members as they move and reflow. A collapsed frame is one node, drawn where the top-left of its
 * box would be; what it holds isn't drawn, and the arrows and links of those items attach to it instead
 * (the outermost collapsed frame, when they nest). A free note is placed like a card; an attached note
 * sits at its offset from what it is attached to, so it moves with it. Pure, so it is tested without a
 * DOM.
 */

import type { Card, Connection, Frame, Note, Resource, View } from "../../ipc/generated";
import { boundsOf, fallbackSize, reflow, type Point, type Rect, type Size } from "../layout";
import { measureText } from "../measure";
import { tableColumns } from "../resources/columns";
import { frameNodeSize, noteSize, resourceCardSize } from "../sizing";

/** A resource card's size before it is drawn, from what the page says (a table backed by a type lists its columns once drawn). */
export function estimatedResourceSize(resource: Resource, columns = tableColumns(resource, null)): Size {
  return resourceCardSize(
    {
      kind: resource.kind,
      name: resource.name,
      columns,
      ...(resource.description ? { description: resource.description } : {}),
    },
    measureText,
  );
}

/**
 * A size for an item that hasn't been measured: a card's (in its view, else the page's), a collapsed
 * frame's node (estimated from its words), a resource card's, or a note's.
 */
export function fallbackItemSize(page: FramedPage, id: string): Size {
  const card = page.cards.find((item) => item.id === id);
  if (card) {
    return fallbackSize(card.view ?? page.defaultView);
  }
  const frame = (page.frames ?? []).find((item) => item.id === id);
  if (frame) {
    return frameNodeSize(frame, measureText);
  }
  const note = (page.notes ?? []).find((item) => item.id === id);
  if (note) {
    return noteSize(note.text, measureText);
  }
  const resource = (page.resources ?? []).find((item) => item.id === id);
  return resource ? estimatedResourceSize(resource) : fallbackSize();
}

/** The room a frame leaves around what it holds: its title band on top, then each side. */
interface FramePadding {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

/** A box's title in its top-left corner (frames.css: 12px down, an 18px line) and the room under it. */
const BOX_TITLE = 12 + 18;
const BOX_GAP = 14;
const BOX_SIDE = 16;
/** A zone's title band: 18px down, the large title's 26px line, the description's line 2px under it, and the gap under them. */
const ZONE_PAD = 18;
const ZONE_TITLE = 26;
const ZONE_DESCRIPTION = 2 + 18;
const ZONE_SIDE = 20;

/** The room a frame leaves around its members. */
export function framePadding(frame: Pick<Frame, "look" | "description">): FramePadding {
  if (frame.look === "box") {
    return { top: BOX_TITLE + BOX_GAP, right: BOX_SIDE, bottom: BOX_SIDE, left: BOX_SIDE };
  }
  const top = ZONE_PAD + ZONE_TITLE + (frame.description ? ZONE_DESCRIPTION : 0) + ZONE_PAD;
  return { top, right: ZONE_SIDE, bottom: ZONE_SIDE, left: ZONE_SIDE };
}

/** `rect` with a frame's padding around it. */
function padded(rect: Rect, padding: FramePadding): Rect {
  return { x: rect.x - padding.left, y: rect.y - padding.top, width: rect.width + padding.left + padding.right, height: rect.height + padding.top + padding.bottom };
}

/** The items of a page geometry reads: cards, resource cards, and notes have positions, frames hold them, notes attach to them. */
export interface FramedPage {
  readonly cards: readonly Card[];
  /** The view of cards without their own. */
  readonly defaultView?: View;
  readonly frames?: readonly Frame[] | undefined;
  readonly resources?: readonly Resource[] | undefined;
  readonly notes?: readonly Note[] | undefined;
  /** The connections a note can be attached to. */
  readonly links?: readonly Connection[] | undefined;
}

/** A frame as drawn: its box (or, collapsed, its node), and how many frames hold it. */
interface DrawnFrame {
  readonly frame: Frame;
  readonly rect: Rect;
  readonly depth: number;
  readonly collapsed: boolean;
}

interface PageGeometry {
  /** Every drawn item's rectangle: cards, resource cards, and notes, frames' boxes, collapsed frames' nodes. */
  readonly rects: ReadonlyMap<string, Rect>;
  /** The frames drawn (none inside a collapsed frame), outermost first. */
  readonly frames: readonly DrawnFrame[];
  /** Each item inside a collapsed frame, and the collapsed frame drawn in its place. */
  readonly shownAs: ReadonlyMap<string, string>;
  /** The nodes drawn inside each expanded frame (cards, resource cards, free notes, collapsed frames), nested ones included. */
  readonly inside: ReadonlyMap<string, readonly string[]>;
}

/** An item being dragged, at the pointer: a card, a resource card, a note, or a frame (which moves what it holds). */
export interface Drag {
  readonly id: string;
  readonly x: number;
  readonly y: number;
}

/** What holds what among a page's frames (a cycle, which the file never has, ends a walk). */
class FrameTree {
  readonly byId: ReadonlyMap<string, Frame>;
  private readonly holder = new Map<string, Frame>();

  constructor(frames: readonly Frame[]) {
    this.byId = new Map(frames.map((frame) => [frame.id, frame]));
    for (const frame of frames) {
      for (const member of frame.members) {
        if (!this.holder.has(member)) {
          this.holder.set(member, frame);
        }
      }
    }
  }

  /** The frames that hold `id`, innermost first. */
  chain(id: string): Frame[] {
    const chain: Frame[] = [];
    for (let frame = this.holder.get(id); frame && !chain.includes(frame); frame = this.holder.get(frame.id)) {
      chain.push(frame);
    }
    return chain;
  }

  /** The outermost collapsed frame holding `id`, if any. */
  collapsedAround(id: string): Frame | undefined {
    return this.chain(id).findLast((frame) => frame.collapsed === true);
  }
}

/** Each item inside a collapsed frame, and the collapsed frame drawn in its place (the outermost). */
export function collapsedPlaces(page: FramedPage): Map<string, string> {
  const tree = new FrameTree(page.frames ?? []);
  const places = new Map<string, string>();
  const placed = [...page.cards, ...(page.resources ?? []), ...(page.notes ?? [])];
  for (const id of [...placed.map((item) => item.id), ...tree.byId.keys()]) {
    const around = tree.collapsedAround(id);
    if (around) {
      places.set(id, around.id);
    }
  }
  return places;
}

/**
 * Where an attached note's offset starts from, as the file's rule (the engine's) reads on what is
 * drawn: a card's or resource card's top-left; the top-left of a frame's cards, resource cards, and
 * nested frames (their points, not their boxes; a collapsed frame's node corner inside its padding);
 * or the midpoint of a link's ends' points. Undefined for an item that isn't drawn.
 */
export function attachPoint(page: FramedPage, rects: ReadonlyMap<string, Rect>, id: string, depth = 0): Point | undefined {
  const link = (page.links ?? []).find((item) => item.id === id);
  if (link) {
    const from = depth < POINT_DEPTH ? attachPoint(page, rects, link.from, depth + 1) : undefined;
    const to = depth < POINT_DEPTH ? attachPoint(page, rects, link.to, depth + 1) : undefined;
    return from && to ? { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 } : undefined;
  }
  const rect = rects.get(id);
  const frame = (page.frames ?? []).find((item) => item.id === id);
  if (!rect || !frame) {
    return rect ? { x: rect.x, y: rect.y } : undefined;
  }
  if (frame.collapsed === true) {
    const padding = framePadding(frame);
    return { x: rect.x + padding.left, y: rect.y + padding.top };
  }
  const notes = new Set((page.notes ?? []).map((note) => note.id));
  const corners = depth < POINT_DEPTH ? frame.members.flatMap((member) => (notes.has(member) ? [] : (attachPoint(page, rects, member, depth + 1) ?? []))) : [];
  return corners.length > 0 ? { x: Math.min(...corners.map((at) => at.x)), y: Math.min(...corners.map((at) => at.y)) } : undefined;
}

/** How many links and frames `attachPoint` walks through at most. */
const POINT_DEPTH = 5;

/** The gap between an item and a note under it (Arrange's, and where a note covering its item goes). */
export const NOTE_GAP = 12;

/** Whether two rectangles overlap (touching edges don't). */
function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * The page's geometry: cards, resource cards, and free notes at their saved positions reflowed by
 * `sizeOf` (as `reflow` does without frames), each collapsed frame as a node in that reflow, each
 * expanded frame's box around what it shows and the notes attached to those items, and each attached
 * note at its offset from its item (under the item, where the offset would put it over it). A note on
 * an item that isn't drawn isn't drawn either. `sizeOf`
 * sizes cards, resource cards, notes, and collapsed frames' nodes. While `drag` moves an item, it
 * follows the pointer; a dragged frame takes what it holds along.
 */
export function pageGeometry(page: FramedPage, sizeOf: (id: string) => Size, drag?: Drag | null): PageGeometry {
  const tree = new FrameTree(page.frames ?? []);
  const attached = (page.notes ?? []).filter((note) => note.attach !== undefined);
  const leaves: readonly (Card | Resource | Note)[] = [...page.cards, ...(page.resources ?? []), ...(page.notes ?? []).filter((note) => note.attach === undefined)];
  const shownAs = collapsedPlaces(page);

  // Where a collapsed frame's node goes: where its box's top-left would be, from the saved positions.
  const at = new Map(leaves.map((leaf) => [leaf.id, leaf.at] as const));
  const natural = new Map<string, Rect | undefined>();
  const naturalRect = (id: string, seen: ReadonlySet<string>): Rect | undefined => {
    const position = at.get(id);
    if (position) {
      return { x: position[0], y: position[1], ...sizeOf(id) };
    }
    const frame = tree.byId.get(id);
    if (!frame || seen.has(id)) {
      return undefined;
    }
    if (!natural.has(id)) {
      const inner = new Set([...seen, id]);
      const members = boundsOf(frame.members.flatMap((member) => naturalRect(member, inner) ?? []));
      natural.set(id, members ? padded(members, framePadding(frame)) : undefined);
    }
    const box = natural.get(id);
    return box && frame.collapsed === true ? { x: box.x, y: box.y, ...sizeOf(id) } : box;
  };

  // Reflow what is drawn on its own: cards and resource cards, and collapsed frames' nodes.
  const nodes = [
    ...leaves.filter((leaf) => !shownAs.has(leaf.id)).map((leaf) => ({ id: leaf.id, at: leaf.at })),
    ...[...tree.byId.values()].flatMap((frame) => {
      const rect = frame.collapsed === true && !shownAs.has(frame.id) ? naturalRect(frame.id, new Set()) : undefined;
      return rect ? [{ id: frame.id, at: [rect.x, rect.y] as const }] : [];
    }),
  ];
  const positions = reflow(nodes, sizeOf, drag?.id);

  // An attached note at its offset from what it is attached to (drawn), or at the pointer while dragged.
  // Where the offset would put it over its own item (which has grown since), it sits under the item.
  const noteRect = (note: Note, rects: ReadonlyMap<string, Rect>): Rect | undefined => {
    const from = note.attach !== undefined && !shownAs.has(note.id) ? attachPoint(page, rects, note.attach) : undefined;
    if (!from) {
      return undefined;
    }
    const size = sizeOf(note.id);
    if (drag?.id === note.id) {
      return { x: drag.x, y: drag.y, ...size };
    }
    const rect = { x: from.x + note.at[0], y: from.y + note.at[1], ...size };
    const item = note.attach !== undefined ? rects.get(note.attach) : undefined;
    return item && overlaps(rect, item) ? { ...rect, y: item.y + item.height + NOTE_GAP } : rect;
  };

  const inside = new Map<string, string[]>();
  const boxes = (shift: (id: string) => Point) => {
    const rects = new Map<string, Rect>(nodes.map(({ id }) => [id, { ...shift(id), ...sizeOf(id) }]));
    // A frame takes in the notes attached to what it holds, as Arrange places them (not those on itself).
    const notesOn = (member: string): Rect[] => attached.flatMap((note) => (note.attach === member ? (noteRect(note, rects) ?? []) : []));
    const box = (frame: Frame, seen: ReadonlySet<string>): Rect | undefined => {
      const known = rects.get(frame.id);
      if (known || seen.has(frame.id)) {
        return known;
      }
      const inner = new Set([...seen, frame.id]);
      const drawn: string[] = [];
      const members = frame.members.flatMap((member) => {
        const nested = tree.byId.get(member);
        if (nested && nested.collapsed !== true) {
          const rect = box(nested, inner);
          drawn.push(...(inside.get(nested.id) ?? []));
          return rect ? [rect, ...notesOn(member)] : [];
        }
        const rect = rects.get(member);
        if (rect) {
          drawn.push(member);
        }
        return rect ? [rect, ...notesOn(member)] : [];
      });
      inside.set(frame.id, drawn);
      const bounds = boundsOf(members);
      const rect = bounds ? padded(bounds, framePadding(frame)) : undefined;
      if (rect) {
        rects.set(frame.id, rect);
      }
      return rect;
    };
    for (const frame of tree.byId.values()) {
      if (frame.collapsed !== true && !shownAs.has(frame.id)) {
        box(frame, new Set());
      }
    }
    return rects;
  };
  const placed = (id: string): Point => positions.get(id) ?? { x: 0, y: 0 };
  let rects = boxes((id) => (drag?.id === id ? drag : placed(id)));
  // A dragged frame's box follows the pointer, with everything it holds.
  const dragged = drag && tree.byId.has(drag.id) && !nodes.some((node) => node.id === drag.id) ? rects.get(drag.id) : undefined;
  if (drag && dragged) {
    const held = new Set(inside.get(drag.id));
    const dx = drag.x - dragged.x;
    const dy = drag.y - dragged.y;
    rects = boxes((id) => {
      const point = placed(id);
      return held.has(id) ? { x: point.x + dx, y: point.y + dy } : point;
    });
  }

  // Attached notes last, so a note on a frame or a link finds it drawn.
  for (const note of attached) {
    const rect = noteRect(note, rects);
    if (rect) {
      rects.set(note.id, rect);
    }
  }

  const frames = [...tree.byId.values()]
    .flatMap((frame): DrawnFrame[] => {
      const rect = rects.get(frame.id);
      return rect && !shownAs.has(frame.id) ? [{ frame, rect, depth: tree.chain(frame.id).length, collapsed: frame.collapsed === true }] : [];
    })
    .sort((a, b) => a.depth - b.depth);
  return { rects, frames, shownAs, inside };
}
