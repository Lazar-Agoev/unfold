/**
 * Arrange (design §4.12), from the inside out. Each frame, deepest first, lays out what it holds by
 * call depth and connections (`arrange`), to the right or downward as its direction says, and takes
 * the size that fits them plus room for its title. The level above treats each frame as one node: the
 * arrows that cross its border join it at the card they reach inside. A collapsed frame is one node of
 * its own size, and what it holds is laid out inside it too, ready for when it opens. Type cards rank
 * by their type arrows, as calls. A connection orders its ends as data flows: a `reads` connection's
 * resource card goes before the code that reads it, any other after the side that acts; drawn links
 * don't count. A note attached to a card, resource card, or frame goes under it (several in id order),
 * and the item takes room for them; once the items are placed, a note on a link goes beside its line's
 * middle, clear of them (`lineNoteSpots`); free notes aren't placed.
 *
 * It is deterministic: the order of the items depends only on what they are and where they are now,
 * so arranging again changes nothing. A page without frames arranges exactly as it did before frames.
 */

import type { Frame, FrameDirection, Link, Note } from "../ipc/generated";
import { frameChain } from "../state/boards/frames";
import type { Arrow } from "./arrows";
import { NOTE_GAP, framePadding, pageGeometry, type FramedPage } from "./frames/geometry";
import { HEADER_PORT, arrange, boundsOf, type ArrangeCall, type Point, type Rect, type Size } from "./layout";
import { lineMiddle, lineNoteSpot } from "./notes/spots";

export interface ArrangeInput {
  readonly page: FramedPage & { readonly links: readonly Link[] };
  /** Each drawn item's rectangle now (`pageGeometry`): it orders the items, and the page keeps its top-left. */
  readonly rects: ReadonlyMap<string, Rect>;
  /** The sizes of cards, resource cards, notes, and collapsed frames' nodes. */
  readonly sizeOf: (id: string) => Size;
  /** The page's arrows between cards (`computeArrows`): calls, handlers, and type arrows. */
  readonly arrows: readonly Arrow[];
  /** An arrow's port offset below its card's top, when known (else the header's). */
  readonly portOffset: (arrow: Arrow) => number | undefined;
  /** How the page's top level is laid out (default: to the right). */
  readonly direction?: FrameDirection;
  /** Arrange only what this frame holds; its box keeps its top-left. */
  readonly frame?: string;
}

/** Something that orders two items: an arrow between cards, or a connection. */
interface Edge {
  readonly source: string;
  readonly target: string;
  /** The arrow's own port below its card's top, when it leaves a card drawn as itself. */
  readonly port: number | undefined;
  readonly order: number;
}

/** Where something inside a laid-out frame is drawn, from the frame's top: by itself, or by a collapsed frame's node. */
interface Drawn {
  readonly top: number;
  readonly own: boolean;
}

/** A node at one level: a card, a resource card, or a frame as one block, with the notes under it. */
interface Block {
  readonly id: string;
  readonly size: Size;
  readonly entrance: boolean;
  /** Where each card, resource card, and note inside goes, from the block's top-left. */
  readonly offsets: ReadonlyMap<string, Point>;
  /** Where each item inside is drawn, from the block's top (a frame's own nodes only). */
  readonly drawn: ReadonlyMap<string, Drawn>;
}

/** The order of a level's items: entrances first, then left to right and top to bottom as they are now. */
function byPriority(corner: (id: string) => Point): (a: Block, b: Block) => number {
  return (a, b) => {
    const ca = corner(a.id);
    const cb = corner(b.id);
    return Number(b.entrance) - Number(a.entrance) || ca.x - cb.x || ca.y - cb.y || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  };
}

/** Lays out one level's blocks from `origin`, to the right or (transposed) downward. */
function layoutLevel(blocks: readonly Block[], calls: readonly ArrangeCall[], direction: FrameDirection, origin: Point): Map<string, Point> {
  const cards = blocks.map(({ id, size }) => ({ id, ...size }));
  if (direction === "right") {
    return arrange(cards, calls, origin);
  }
  // Downward: depth runs down the page, a callee under its caller's left edge.
  const flipped = arrange(
    cards.map(({ id, width, height }) => ({ id, width: height, height: width })),
    calls.map((call) => ({ ...call, port: HEADER_PORT, entry: HEADER_PORT })),
    { x: origin.y, y: origin.x },
  );
  return new Map([...flipped].map(([id, point]) => [id, { x: point.y, y: point.x }]));
}

/** Where Arrange puts the page's cards, resource cards, and the notes attached to them (the ones it moves). */
export function arrangePage(input: ArrangeInput): Map<string, Point> {
  const { page, rects, sizeOf } = input;
  const frames = new Map((page.frames ?? []).map((frame) => [frame.id, frame]));
  const leaves = new Map([...page.cards, ...(page.resources ?? [])].map((leaf) => [leaf.id, leaf]));
  const holder = new Map<string, string>();
  for (const frame of frames.values()) {
    for (const member of frame.members) {
      if (!holder.has(member)) {
        holder.set(member, frame.id);
      }
    }
  }
  const entrances = new Set(page.cards.flatMap((card) => ("entrance" in card.ref ? [card.id] : [])));
  // The notes that go under each card, resource card, and frame.
  const notesOf = new Map<string, Note[]>();
  for (const note of [...(page.notes ?? [])].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))) {
    if (note.attach !== undefined && (leaves.has(note.attach) || frames.has(note.attach))) {
      notesOf.set(note.attach, [...(notesOf.get(note.attach) ?? []), note]);
    }
  }
  /** An item of `size` with its notes under it: the room both take, and where each note goes. */
  const withNotes = (id: string, size: Size): { size: Size; notes: [string, Point][] } => {
    let height = size.height;
    let width = size.width;
    const notes: [string, Point][] = [];
    for (const note of notesOf.get(id) ?? []) {
      const own = sizeOf(note.id);
      notes.push([note.id, { x: 0, y: height + NOTE_GAP }]);
      height += NOTE_GAP + own.height;
      width = Math.max(width, own.width);
    }
    return { size: { width, height }, notes };
  };

  // Where an item is now: as drawn, else its saved position, else (a hidden frame) where its box's
  // top-left would be: the top-left of what it holds, less its padding.
  const corners = new Map<string, Point>();
  const corner = (id: string, seen: ReadonlySet<string> = new Set()): Point => {
    const known = corners.get(id);
    if (known) {
      return known;
    }
    const rect = rects.get(id);
    const leaf = leaves.get(id);
    const frame = frames.get(id);
    const inner = new Set([...seen, id]);
    const members = seen.has(id) ? [] : (frame?.members ?? []).filter((member) => leaves.has(member) || frames.has(member)).map((member) => corner(member, inner));
    const held = boundsOf(members.map((at) => ({ ...at, width: 0, height: 0 })));
    const padding = frame ? framePadding(frame) : undefined;
    const point = rect ?? (leaf ? { x: leaf.at[0], y: leaf.at[1] } : held && padding ? { x: held.x - padding.left, y: held.y - padding.top } : { x: 0, y: 0 });
    corners.set(id, { x: point.x, y: point.y });
    return { x: point.x, y: point.y };
  };

  const edges: Edge[] = [
    ...input.arrows.map((arrow) => ({ source: arrow.source, target: arrow.target, port: input.portOffset(arrow), order: arrow.order })),
    ...page.links.flatMap((link): Edge[] => {
      if (link.kind === undefined) {
        return [];
      }
      // A resource card goes before the code that reads it, after the code that writes it.
      const [source, target] = link.kind === "reads" ? [link.to, link.from] : [link.from, link.to];
      return [{ source, target, port: undefined, order: 0 }];
    }),
  ];

  /** The item at the level of `container` (null: the page) that is or holds `id`. */
  const childOf = (id: string, container: string | null): string | undefined => {
    const seen = new Set<string>();
    for (let at: string | undefined = id; at !== undefined && !seen.has(at); at = holder.get(at)) {
      seen.add(at);
      if ((holder.get(at) ?? null) === container) {
        return at;
      }
    }
    return undefined;
  };

  /** What draws `item` in `block`, from the block's top: a collapsed frame draws everything in it with its node. */
  const anchor = (block: Block, item: string): Drawn =>
    frames.get(block.id)?.collapsed === true ? { top: 0, own: false } : (block.drawn.get(item) ?? { top: 0, own: block.id === item && leaves.has(item) });

  /** The calls among a level's blocks, each at the card it leaves and the card it reaches. */
  const callsAmong = (blocks: ReadonlyMap<string, Block>, container: string | null): ArrangeCall[] =>
    edges.flatMap((edge) => {
      const from = childOf(edge.source, container);
      const to = childOf(edge.target, container);
      const source = from !== undefined ? blocks.get(from) : undefined;
      const target = to !== undefined ? blocks.get(to) : undefined;
      if (!source || !target || source === target) {
        return [];
      }
      const leaving = anchor(source, edge.source);
      const entering = anchor(target, edge.target);
      const port = leaving.top + ((leaving.own ? edge.port : undefined) ?? HEADER_PORT);
      return [{ source: source.id, target: target.id, port, order: edge.order, ...(entering.top !== 0 ? { entry: entering.top + HEADER_PORT } : {}) }];
    });

  const blocks = new Map<string, Block>();
  /** A frame laid out inside: its members by call depth in its direction, then its size and what goes where. */
  const frameBlock = (frame: Frame, seen: ReadonlySet<string>): Block => {
    const inner = new Set([...seen, frame.id]);
    const children = new Map(
      frame.members.flatMap((member) => {
        const block = seen.has(member) || member === frame.id ? undefined : blockOf(member, inner);
        return block ? [[member, block] as const] : [];
      }),
    );
    const ordered = [...children.values()].sort(byPriority((id) => corner(id)));
    const positions = layoutLevel(ordered, callsAmong(children, frame.id), frame.direction ?? "right", { x: 0, y: 0 });
    const content = boundsOf(ordered.map((block) => ({ ...(positions.get(block.id) ?? { x: 0, y: 0 }), ...block.size })));
    const padding = framePadding(frame);
    const shift = { x: padding.left - (content?.x ?? 0), y: padding.top - (content?.y ?? 0) };
    const offsets = new Map<string, Point>();
    const drawn = new Map<string, Drawn>();
    for (const block of ordered) {
      const at = positions.get(block.id) ?? { x: 0, y: 0 };
      const x = at.x + shift.x;
      const y = at.y + shift.y;
      if (leaves.has(block.id)) {
        offsets.set(block.id, { x, y });
      }
      drawn.set(block.id, { top: y, own: leaves.has(block.id) });
      for (const [id, point] of block.offsets) {
        offsets.set(id, { x: x + point.x, y: y + point.y });
      }
      const collapsed = frames.get(block.id)?.collapsed === true;
      for (const [id, place] of block.drawn) {
        drawn.set(id, collapsed ? { top: y, own: false } : { top: y + place.top, own: place.own });
      }
    }
    const own =
      frame.collapsed === true
        ? sizeOf(frame.id)
        : { width: (content?.width ?? 0) + padding.left + padding.right, height: (content?.height ?? 0) + padding.top + padding.bottom };
    const { size, notes } = withNotes(frame.id, own);
    for (const [id, point] of notes) {
      offsets.set(id, point);
    }
    return { id: frame.id, size, entrance: [...offsets.keys()].some((id) => entrances.has(id)), offsets, drawn };
  };
  const blockOf = (id: string, seen: ReadonlySet<string>): Block | undefined => {
    const known = blocks.get(id);
    if (known) {
      return known;
    }
    const frame = frames.get(id);
    const leaf = leaves.has(id) ? withNotes(id, sizeOf(id)) : undefined;
    const block: Block | undefined = frame
      ? frameBlock(frame, seen)
      : leaf
        ? { id, size: leaf.size, entrance: entrances.has(id), offsets: new Map(leaf.notes), drawn: new Map() }
        : undefined;
    if (block) {
      blocks.set(id, block);
    }
    return block;
  };

  const result = new Map<string, Point>();
  const place = (block: Block, at: Point) => {
    if (leaves.has(block.id)) {
      result.set(block.id, at);
    }
    for (const [id, offset] of block.offsets) {
      result.set(id, { x: Math.round(at.x + offset.x), y: Math.round(at.y + offset.y) });
    }
  };
  const scope = input.frame !== undefined ? frames.get(input.frame) : undefined;
  if (scope) {
    place(frameBlock(scope, new Set()), corner(scope.id));
    return result;
  }
  const top = new Map(
    [...leaves.keys(), ...frames.keys()].flatMap((id) => {
      const block = holder.has(id) ? undefined : blockOf(id, new Set());
      return block ? [[id, block] as const] : [];
    }),
  );
  const ordered = [...top.values()].sort(byPriority((id) => corner(id)));
  const all = ordered.map((block) => corner(block.id));
  const origin = { x: Math.min(...all.map((point) => point.x)), y: Math.min(...all.map((point) => point.y)) };
  const positions = layoutLevel(ordered, callsAmong(top, null), input.direction ?? "right", origin);
  for (const block of ordered) {
    place(block, positions.get(block.id) ?? origin);
  }
  return result;
}

/**
 * Where Arrange puts the notes on links, once the items they join are placed (`page` as arranged):
 * each beside its line's middle (`lineNoteSpot`), in id order, clear of the cards, resource cards,
 * collapsed frames, and other notes drawn, the notes it placed before, and the frames the line doesn't
 * start or end in. A note whose line isn't drawn between its own ends (one is hidden in a collapsed
 * frame) stays as it is. `links`, when given, limits it to the notes on those links.
 */
export function lineNoteSpots(page: FramedPage & { readonly links: readonly Link[] }, sizeOf: (id: string) => Size, links?: ReadonlySet<string>): Record<string, [number, number]> {
  const lines = new Map(page.links.filter((link) => links?.has(link.id) ?? true).map((link) => [link.id, link]));
  const notes = (page.notes ?? []).filter((note) => note.attach !== undefined && lines.has(note.attach)).sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const placing = new Set(notes.map((note) => note.id));
  const { rects } = pageGeometry(page, sizeOf);
  const occupied = [...rects].filter(([id]) => !placing.has(id));
  const spots: Record<string, [number, number]> = {};
  for (const note of notes) {
    const link = note.attach !== undefined ? lines.get(note.attach) : undefined;
    const from = link ? rects.get(link.from) : undefined;
    const to = link ? rects.get(link.to) : undefined;
    if (!link || !from || !to || !rects.has(note.id)) {
      continue;
    }
    // The frames around its ends hold the line, so the note may sit in them.
    const around = new Set([...frameChain(page, link.from), ...frameChain(page, link.to)].map((frame) => frame.id));
    const size = sizeOf(note.id);
    const [x, y] = lineNoteSpot(lineMiddle(from, to), size, occupied.flatMap(([id, rect]) => (around.has(id) ? [] : [rect])));
    spots[note.id] = [x, y];
    occupied.push([note.id, { x, y, ...size }]);
  }
  return spots;
}
