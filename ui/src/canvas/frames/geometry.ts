/**
 * Where a page's frames are (design §4.4). A frame has no position of its own: its box is the bounds
 * of what it holds plus room for its title, computed from the inside out, so it follows its members as
 * they move and reflow. A collapsed frame is one node, drawn where the top-left of its box would be;
 * what it holds isn't drawn, and the arrows and links of those items attach to it instead (the
 * outermost collapsed frame, when they nest). Pure, so it is tested without a DOM.
 */

import type { CardBody } from "../../code/typeKinds";
import type { Card, Frame, Resource } from "../../ipc/generated";
import { boundsOf, fallbackSize, reflow, type Point, type Rect, type Size } from "../layout";
import { measureText } from "../measure";
import { frameNodeSize } from "../sizing";

/** A resource card before it is drawn. */
const RESOURCE_SIZE: Size = { width: 240, height: 80 };

/**
 * A size for an item that hasn't been measured: a card's (an entrance or a block shown as `body`), a
 * collapsed frame's node (estimated from its words), or a resource card's.
 */
export function fallbackItemSize(page: FramedPage, id: string, body?: CardBody): Size {
  const card = page.cards.find((item) => item.id === id);
  if (card) {
    return fallbackSize("entrance" in card.ref, body);
  }
  const frame = (page.frames ?? []).find((item) => item.id === id);
  return frame ? frameNodeSize(frame, measureText) : RESOURCE_SIZE;
}

/** The room a frame leaves around what it holds: its title band on top, then each side. */
interface FramePadding {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

/** A box's title bar (frames.css) and the gap under it. */
const BOX_TITLE = 32;
const BOX_GAP = 12;
const BOX_SIDE = 16;
/** A zone's title band: its padding, the large title's line, the description's line, and the gap under it. */
const ZONE_PAD = 16;
const ZONE_TITLE = 24;
const ZONE_DESCRIPTION = 20;
const ZONE_SIDE = 24;

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

/** The items of a page geometry reads: cards and resource cards have positions, frames hold them. */
export interface FramedPage {
  readonly cards: readonly Card[];
  readonly frames?: readonly Frame[] | undefined;
  readonly resources?: readonly Resource[] | undefined;
}

/** A frame as drawn: its box (or, collapsed, its node), and how many frames hold it. */
interface DrawnFrame {
  readonly frame: Frame;
  readonly rect: Rect;
  readonly depth: number;
  readonly collapsed: boolean;
}

interface PageGeometry {
  /** Every drawn item's rectangle: cards and resource cards, frames' boxes, collapsed frames' nodes. */
  readonly rects: ReadonlyMap<string, Rect>;
  /** The frames drawn (none inside a collapsed frame), outermost first. */
  readonly frames: readonly DrawnFrame[];
  /** Each item inside a collapsed frame, and the collapsed frame drawn in its place. */
  readonly shownAs: ReadonlyMap<string, string>;
  /** The nodes drawn inside each expanded frame (cards, resource cards, collapsed frames), nested ones included. */
  readonly inside: ReadonlyMap<string, readonly string[]>;
}

/** An item being dragged, at the pointer: a card, a resource card, or a frame (which moves what it holds). */
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
  for (const id of [...page.cards.map((card) => card.id), ...(page.resources ?? []).map((resource) => resource.id), ...tree.byId.keys()]) {
    const around = tree.collapsedAround(id);
    if (around) {
      places.set(id, around.id);
    }
  }
  return places;
}

/**
 * The page's geometry: cards and resource cards at their saved positions reflowed by `sizeOf` (as
 * `reflow` does without frames), each collapsed frame as a node in that reflow, and each expanded
 * frame's box around what it shows. `sizeOf` sizes cards, resource cards, and collapsed frames' nodes.
 * While `drag` moves an item, it follows the pointer; a dragged frame takes what it holds along.
 */
export function pageGeometry(page: FramedPage, sizeOf: (id: string) => Size, drag?: Drag | null): PageGeometry {
  const tree = new FrameTree(page.frames ?? []);
  const leaves: readonly (Card | Resource)[] = [...page.cards, ...(page.resources ?? [])];
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

  const inside = new Map<string, string[]>();
  const boxes = (shift: (id: string) => Point) => {
    const rects = new Map<string, Rect>(nodes.map(({ id }) => [id, { ...shift(id), ...sizeOf(id) }]));
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
          return rect ? [rect] : [];
        }
        const rect = rects.get(member);
        if (rect) {
          drawn.push(member);
        }
        return rect ? [rect] : [];
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

  const frames = [...tree.byId.values()]
    .flatMap((frame): DrawnFrame[] => {
      const rect = rects.get(frame.id);
      return rect && !shownAs.has(frame.id) ? [{ frame, rect, depth: tree.chain(frame.id).length, collapsed: frame.collapsed === true }] : [];
    })
    .sort((a, b) => a.depth - b.depth);
  return { rects, frames, shownAs, inside };
}
