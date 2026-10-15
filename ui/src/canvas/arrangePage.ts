/**
 * Arrange, from the inside out. Each frame, deepest first, lays out what it holds by
 * call depth and connections (`arrange`), to the right or downward as its direction says, and takes
 * the size that fits them plus room for its title. The level above treats each frame as one node: the
 * arrows that cross its border join it at the card they reach inside. It runs twice: the first time
 * says where everything goes, so the second knows which lines leave each frame by its east side; those
 * get a lane through the frame to that side, and what they reach lines up with where they leave. A
 * collapsed frame is one node of its own size, and what it holds is laid out inside it too, ready for
 * when it opens. Type cards rank
 * by their type arrows, as calls. A connection runs from the code to what it touches, so a resource
 * card is a leaf in the column after the step that touches it, level with that step's row (the
 * middle of its steps' rows when several touch it); a queue goes before the code it delivers to, and an
 * actor after the table it acts on. Arrange never changes what a frame holds: a resource card no
 * frame holds goes at the page's level, past the deepest step that uses it. Tables joined by
 * relationships are one block, stacked. A note attached to a card, resource card, or frame goes under
 * it (several in id order), and the item takes room for them; a free note goes to the top right of the
 * frame it is in, else of the page; once the items are placed, a note on a connection goes beside its
 * line's middle, clear of them (`lineNoteSpots`). Items with no lines go in a grid at the end, a
 * file's cards together. App and entrance cards come first.
 *
 * It is deterministic: the order of the items depends only on what they are and where they are now,
 * so arranging again changes nothing.
 */

import type { Connection, Frame, FrameDirection, Note, Resource } from "../ipc/generated";
import { frameChain } from "../state/boards/frames";
import { stepPort, type Arrow } from "./arrows";
import { NOTE_GAP, framePadding, pageGeometry, type FramedPage } from "./frames/geometry";
import { HEADER_PORT, STACK_GAP, arrange, boundsOf, type ArrangeCall, type Point, type Rect, type Size } from "./layout";
import { lineMiddle } from "./lines/paths";
import type { RoutedLine } from "./lines/route";
import { lineNoteSpot } from "./notes/spots";

export interface ArrangeInput {
  readonly page: FramedPage & { readonly links: readonly Connection[] };
  /** Each drawn item's rectangle now (`pageGeometry`): it orders the items, and the page keeps its top-left. */
  readonly rects: ReadonlyMap<string, Rect>;
  /** The sizes of cards, resource cards, notes, and collapsed frames' nodes. */
  readonly sizeOf: (id: string) => Size;
  /** The page's arrows between cards (`computeArrows`): calls, starts, and type arrows. */
  readonly arrows: readonly Arrow[];
  /** A card's port (an arrow's, or a connection's `stepPort`) below its top, when known (else the header's). */
  readonly portOffset: (card: string, port: string) => number | undefined;
  /** How the page's top level is laid out (default: to the right). */
  readonly direction?: FrameDirection;
  /** Arrange only what this frame holds; its box keeps its top-left. */
  readonly frame?: string;
}

/** Something that orders two items: an arrow between cards, or a connection. */
interface Edge {
  readonly source: string;
  readonly target: string;
  /** Its own port below its card's top (an arrow's, or a connection's step), when it leaves a card drawn as itself. */
  readonly port: number | undefined;
  readonly order: number;
  /** A connection's (else an arrow's). */
  readonly link?: true;
}

/** Where a block's members go inside it: its padding, and the direction they run in. */
interface Inside {
  readonly padding: { readonly top: number; readonly right: number; readonly bottom: number; readonly left: number };
  readonly direction: FrameDirection;
}

/** A group of related tables: stacked, nothing around them. */
const TABLES: Inside = { padding: { top: 0, right: 0, bottom: 0, left: 0 }, direction: "down" };
/** The prefixes of a group of related tables' id and of where a line leaves a frame's east side, which no page item has. */
const TABLES_ID = "\u0000tables:";
const EXIT_ID = "\u0000exit:";

/**
 * Which way a connection orders its ends: from the code to what it touches, so a resource card is
 * never a root; from a queue to the code it delivers to; from a table to the actor that acts on it.
 * Otherwise from `from` to `to`.
 */
function linkOrder(link: Connection, resource: (id: string) => Resource | undefined): [string, string] {
  const from = resource(link.from)?.kind;
  const to = resource(link.to)?.kind;
  const flipped = (from !== undefined && to === undefined && from !== "queue") || (from === "actor" && to !== undefined && to !== "actor");
  return flipped ? [link.to, link.from] : [link.from, link.to];
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
  /** Its file, for a card: items with no lines are grouped by it. */
  readonly group?: string | undefined;
  /** An expanded frame: it keeps a wider gap from what is stacked by it. */
  readonly spaced?: boolean;
  /** Where each card, resource card, and note inside goes, from the block's top-left. */
  readonly offsets: ReadonlyMap<string, Point>;
  /** Where each item inside is drawn, from the block's top (a frame's own nodes only). */
  readonly drawn: ReadonlyMap<string, Drawn>;
  /** Where lines leave its east side, below its top, by their edge's index (a frame's, to what is past it). */
  readonly exits?: ReadonlyMap<number, number>;
}

/** The order of a level's items: entrances first, then left to right and top to bottom as they are now. */
function byPriority(corner: (id: string) => Point): (a: Block, b: Block) => number {
  return (a, b) => {
    const ca = corner(a.id);
    const cb = corner(b.id);
    return Number(b.entrance) - Number(a.entrance) || ca.x - cb.x || ca.y - cb.y || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  };
}

/** Lays out one level's blocks from `origin`, to the right (with `exits` on its east side) or (transposed) downward. */
function layoutLevel(blocks: readonly Block[], calls: readonly ArrangeCall[], direction: FrameDirection, origin: Point, exits: readonly string[] = []): Map<string, Point> {
  const cards = blocks.map(({ id, size, group, spaced }) => ({ id, ...size, ...(group !== undefined ? { group } : {}), ...(spaced === true ? { spaced } : {}) }));
  if (direction === "right") {
    return arrange(cards, calls, origin, exits);
  }
  // Downward: depth runs down the page, a callee under its caller's left edge.
  const flipped = arrange(
    cards.map(({ id, width, height }) => ({ id, width: height, height: width })),
    calls.map((call) => ({ ...call, port: HEADER_PORT, entry: HEADER_PORT })),
    { x: origin.y, y: origin.x },
  );
  return new Map([...flipped].map(([id, point]) => [id, { x: point.y, y: point.x }]));
}

/** Where Arrange puts the page's cards, resource cards, and notes (all but those on connections). */
export function arrangePage(input: ArrangeInput): Map<string, Point> {
  const { page, rects, sizeOf } = input;
  const frames = new Map((page.frames ?? []).map((frame) => [frame.id, frame]));
  const resources = new Map((page.resources ?? []).map((resource) => [resource.id, resource]));
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
  // What a card is grouped with when it has no lines: its file.
  const files = new Map(page.cards.flatMap((card) => ("block" in card.ref ? [[card.id, card.ref.block.split("#")[0] ?? ""] as const] : [])));
  const byId = (a: Note, b: Note) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  // The notes that go under each card, resource card, and frame; the free notes of each frame (and of the page, at "").
  const notesOf = new Map<string, Note[]>();
  const freeNotes = new Map<string, Note[]>();
  for (const note of [...(page.notes ?? [])].sort(byId)) {
    if (note.attach !== undefined && (leaves.has(note.attach) || frames.has(note.attach))) {
      notesOf.set(note.attach, [...(notesOf.get(note.attach) ?? []), note]);
    } else if (note.attach === undefined) {
      const frame = holder.get(note.id) ?? "";
      freeNotes.set(frame, [...(freeNotes.get(frame) ?? []), note]);
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

  // Tables joined by relationships, where they sit (the same frame, or the page): each group is one block.
  const tablesOf = new Map<string, string>();
  for (const link of page.links) {
    const [from, to] = [resources.get(link.from), resources.get(link.to)];
    if (link.kind !== "relates" || !from || !to || from === to || holder.get(from.id) !== holder.get(to.id)) {
      continue;
    }
    const a = tablesOf.get(from.id) ?? from.id;
    const b = tablesOf.get(to.id) ?? to.id;
    const [kept, joined] = a < b ? [a, b] : [b, a];
    for (const [table, group] of [...tablesOf, [from.id, a], [to.id, b]] as const) {
      tablesOf.set(table, group === joined ? kept : group);
    }
  }
  const tables = new Map<string, string[]>();
  for (const [table, group] of [...tablesOf].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    tables.set(`${TABLES_ID}${group}`, [...(tables.get(`${TABLES_ID}${group}`) ?? []), table]);
  }
  /** The item a level lays out for `id`: its group of related tables, else itself. */
  const levelItem = (id: string): string => {
    const group = tablesOf.get(id);
    return group !== undefined ? `${TABLES_ID}${group}` : id;
  };
  /** What holds `id`: a table's group, a group's tables' frame, or a frame (undefined: the page). */
  const parentOf = (id: string): string | undefined => {
    const item = levelItem(id);
    return item !== id ? item : holder.get(tables.get(id)?.[0] ?? id);
  };

  // Where an item is now: as drawn, else its saved position, else (a hidden frame) where its box's
  // top-left would be: the top-left of what it holds, less its padding.
  const corners = new Map<string, Point>();
  const corner = (id: string, seen: ReadonlySet<string> = new Set()): Point => {
    const known = corners.get(id);
    if (known) {
      return known;
    }
    const group = tables.get(id);
    if (group) {
      const point = boundsOf(group.map((table) => ({ ...corner(table, seen), width: 0, height: 0 }))) ?? { x: 0, y: 0 };
      corners.set(id, { x: point.x, y: point.y });
      return { x: point.x, y: point.y };
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

  // Ports to the half pixel (rows sit on half pixels): a measured port varies in its last digits, and Arrange must not.
  const portOffset = (card: string, port: string): number | undefined => {
    const offset = input.portOffset(card, port);
    return offset === undefined ? undefined : Math.round(offset * 2) / 2;
  };
  const edges: Edge[] = [
    ...input.arrows.map((arrow) => ({ source: arrow.source, target: arrow.target, port: portOffset(arrow.source, arrow.sourceHandle), order: arrow.order })),
    ...page.links.map((link): Edge => {
      const [source, target] = linkOrder(link, (id) => resources.get(id));
      // A connection leaves from its step's row where its card draws the step.
      const port = link.step !== undefined && source === link.from ? portOffset(link.from, stepPort(link.step)) : undefined;
      return { source, target, port, order: 0, link: true };
    }),
  ];

  /** The item at the level of `container` (null: the page) that is or holds `id`. */
  const childOf = (id: string, container: string | null): string | undefined => {
    const seen = new Set<string>();
    for (let at: string | undefined = id; at !== undefined && !seen.has(at); at = parentOf(at)) {
      seen.add(at);
      if ((parentOf(at) ?? null) === container) {
        return at;
      }
    }
    return undefined;
  };

  /** What draws `item` in `block`, from the block's top: a collapsed frame draws everything in it with its node. */
  const anchor = (block: Block, item: string): Drawn =>
    frames.get(block.id)?.collapsed === true ? { top: 0, own: false } : (block.drawn.get(item) ?? { top: 0, own: block.id === item && leaves.has(item) });

  /** Where edge `index` leaves `block`, below its top: where it crosses a frame's east side, else at the card it leaves. */
  const leavingPort = (block: Block, edge: Edge, index: number): number => {
    const exit = block.exits?.get(index);
    if (exit !== undefined) {
      return exit;
    }
    const leaving = anchor(block, edge.source);
    return leaving.top + ((leaving.own ? edge.port : undefined) ?? HEADER_PORT);
  };

  /** The calls among a level's blocks, each at the card it leaves (or the side it leaves a frame by) and the card it reaches. */
  const callsAmong = (blocks: ReadonlyMap<string, Block>, container: string | null): ArrangeCall[] =>
    edges.flatMap((edge, index) => {
      const from = childOf(edge.source, container);
      const to = childOf(edge.target, container);
      const source = from !== undefined ? blocks.get(from) : undefined;
      const target = to !== undefined ? blocks.get(to) : undefined;
      if (!source || !target || source === target) {
        return [];
      }
      const entering = anchor(target, edge.target);
      return [
        {
          source: source.id,
          target: target.id,
          port: leavingPort(source, edge, index),
          order: edge.order,
          from: edge.source,
          ...(edge.link ? { link: true } : {}),
          ...(entering.top !== 0 ? { entry: entering.top + HEADER_PORT } : {}),
        },
      ];
    });

  // Arrange runs twice: the first time places every block at its level (`levels`), so the second
  // knows which lines leave each frame by its east side (`leavesEast`), and gives them lanes there.
  let blocks = new Map<string, Block>();
  let levels = new Map<string, Point>();
  const firstPass = new Map<string, Point>();
  /** How each level lays out: the page's, or a frame's direction. */
  const directionOf = (container: string | null): FrameDirection => (container === null ? (input.direction ?? "right") : (frames.get(container)?.direction ?? "right"));
  /**
   * Whether `edge` leaves frame `id` (which holds its start) by its east side: at the first level
   * that holds both its ends, the first pass put what it reaches in a column past what holds its
   * start, and every level from there in lays out to the right.
   */
  const leavesEast = (id: string, edge: Edge): boolean => {
    const seen = new Set<string>();
    for (let inner = id; firstPass.size > 0 && !seen.has(inner); ) {
      seen.add(inner);
      const container = parentOf(inner) ?? null;
      if (directionOf(container) !== "right" || (container !== null && frames.get(container)?.collapsed === true)) {
        return false;
      }
      const other = childOf(edge.target, container);
      if (other !== undefined) {
        return (firstPass.get(other)?.x ?? 0) > (firstPass.get(inner)?.x ?? 0);
      }
      if (container === null) {
        return false;
      }
      inner = container;
    }
    return false;
  };

  /**
   * The free notes of a level at its top right: in a row ending at the right edge of what it laid out
   * (`laid`, else from `origin`), above it, `STACK_GAP` apart; what it laid out moves down under them.
   */
  const topRight = (notes: readonly Note[], laid: Rect | null, origin: Point): { readonly notes: [string, Point][]; readonly down: number } => {
    if (notes.length === 0) {
      return { notes: [], down: 0 };
    }
    const sizes = notes.map((note) => sizeOf(note.id));
    let x = laid ? laid.x + laid.width : origin.x + sizes.reduce((sum, size) => sum + size.width + STACK_GAP, -STACK_GAP);
    const top = laid?.y ?? origin.y;
    const placed = notes.map((note, index): [string, Point] => {
      x -= sizes[index]?.width ?? 0;
      const at = { x, y: top };
      x -= STACK_GAP;
      return [note.id, at];
    });
    return { notes: placed, down: laid ? Math.max(...sizes.map((size) => size.height)) + STACK_GAP : 0 };
  };

  /**
   * `members` laid out as one block: by call depth in its direction inside its padding, with the free
   * notes at its top right; its content's size, what goes where, and where lines leave its east side
   * (a frame's, laid out to the right).
   */
  const layoutInside = (id: string, members: readonly string[], inside: Inside, free: readonly Note[], seen: ReadonlySet<string>) => {
    const inner = new Set([...seen, id]);
    const children = new Map(
      members.flatMap((member) => {
        const item = parentOf(member) === id ? member : levelItem(member);
        const block = seen.has(item) || item === id ? undefined : blockOf(item, inner);
        return block ? [[item, block] as const] : [];
      }),
    );
    const ordered = [...children.values()].sort(byPriority((item) => corner(item)));
    // The lines that leave a frame by its east side end at exits there, so they get lanes to it.
    const frame = frames.get(id);
    const exits =
      frame && frame.collapsed !== true && inside.direction === "right"
        ? edges.flatMap((edge, index) => {
            const from = childOf(edge.source, id);
            const source = from !== undefined ? children.get(from) : undefined;
            return source && childOf(edge.target, id) === undefined && leavesEast(id, edge)
              ? [{ index, call: { source: source.id, target: `${EXIT_ID}${index}`, port: leavingPort(source, edge, index), order: edge.order, from: edge.source, ...(edge.link ? { link: true } : {}) } }]
              : [];
          })
        : [];
    const positions = layoutLevel(
      ordered,
      [...callsAmong(children, id), ...exits.map(({ call }) => call)],
      inside.direction,
      { x: 0, y: 0 },
      exits.map(({ call }) => call.target),
    );
    for (const block of ordered) {
      levels.set(block.id, positions.get(block.id) ?? { x: 0, y: 0 });
    }
    const laid = boundsOf(ordered.map((block) => ({ ...(positions.get(block.id) ?? { x: 0, y: 0 }), ...block.size })));
    const band = topRight(free, laid, { x: 0, y: 0 });
    const content = boundsOf([
      ...ordered.map((block) => ({ ...(positions.get(block.id) ?? { x: 0, y: 0 }), ...block.size, y: (positions.get(block.id)?.y ?? 0) + band.down })),
      ...band.notes.map(([note, at]) => ({ ...at, ...sizeOf(note) })),
    ]);
    const { padding } = inside;
    const shift = { x: padding.left - (content?.x ?? 0), y: padding.top - (content?.y ?? 0) };
    const offsets = new Map<string, Point>();
    const drawn = new Map<string, Drawn>();
    for (const block of ordered) {
      const at = positions.get(block.id) ?? { x: 0, y: 0 };
      const x = at.x + shift.x;
      const y = at.y + band.down + shift.y;
      if (leaves.has(block.id)) {
        offsets.set(block.id, { x, y });
      }
      drawn.set(block.id, { top: y, own: leaves.has(block.id) });
      for (const [item, point] of block.offsets) {
        offsets.set(item, { x: x + point.x, y: y + point.y });
      }
      const collapsed = frames.get(block.id)?.collapsed === true;
      for (const [item, place] of block.drawn) {
        drawn.set(item, collapsed ? { top: y, own: false } : { top: y + place.top, own: place.own });
      }
    }
    for (const [note, at] of band.notes) {
      offsets.set(note, { x: at.x + shift.x, y: at.y + shift.y });
    }
    const size = { width: (content?.width ?? 0) + padding.left + padding.right, height: (content?.height ?? 0) + padding.top + padding.bottom };
    const sides = new Map(exits.map(({ index, call }) => [index, (positions.get(call.target)?.y ?? 0) + band.down + shift.y] as const));
    return { offsets, drawn, size, exits: sides };
  };
  /** A frame laid out inside (`layoutInside`), then its size (a collapsed frame's node's) with its notes under it. */
  const frameBlock = (frame: Frame, seen: ReadonlySet<string>): Block => {
    const { offsets, drawn, size: box, exits } = layoutInside(frame.id, frame.members, { padding: framePadding(frame), direction: frame.direction ?? "right" }, freeNotes.get(frame.id) ?? [], seen);
    const { size, notes } = withNotes(frame.id, frame.collapsed === true ? sizeOf(frame.id) : box);
    for (const [id, point] of notes) {
      offsets.set(id, point);
    }
    return { id: frame.id, size, entrance: [...offsets.keys()].some((id) => entrances.has(id)), spaced: frame.collapsed !== true, offsets, drawn, exits };
  };
  const blockOf = (id: string, seen: ReadonlySet<string>): Block | undefined => {
    const known = blocks.get(id);
    if (known) {
      return known;
    }
    const frame = frames.get(id);
    const group = tables.get(id);
    const leaf = leaves.has(id) ? withNotes(id, sizeOf(id)) : undefined;
    let block: Block | undefined;
    if (frame) {
      block = frameBlock(frame, seen);
    } else if (group) {
      const { offsets, drawn, size } = layoutInside(id, group, TABLES, [], seen);
      block = { id, size, entrance: false, offsets, drawn };
    } else if (leaf) {
      block = { id, size: leaf.size, entrance: entrances.has(id), group: files.get(id), offsets: new Map(leaf.notes), drawn: new Map() };
    }
    if (block) {
      blocks.set(id, block);
    }
    return block;
  };

  /** Lays the page out (or only `scope`, keeping its box's corner): where everything goes. */
  const layoutAll = (scope: Frame | undefined): Map<string, Point> => {
    const result = new Map<string, Point>();
    const place = (block: Block, at: Point) => {
      if (leaves.has(block.id)) {
        result.set(block.id, at);
      }
      for (const [id, offset] of block.offsets) {
        result.set(id, { x: Math.round(at.x + offset.x), y: Math.round(at.y + offset.y) });
      }
    };
    if (scope) {
      place(frameBlock(scope, new Set()), corner(scope.id));
      return result;
    }
    const top = new Map(
      [...leaves.keys(), ...frames.keys()].flatMap((id) => {
        const item = levelItem(id);
        const block = holder.has(id) ? undefined : blockOf(item, new Set());
        return block ? [[item, block] as const] : [];
      }),
    );
    const ordered = [...top.values()].sort(byPriority((id) => corner(id)));
    const all = ordered.map((block) => corner(block.id));
    const origin = { x: Math.min(...all.map((point) => point.x)), y: Math.min(...all.map((point) => point.y)) };
    const positions = layoutLevel(ordered, callsAmong(top, null), directionOf(null), origin);
    for (const block of ordered) {
      levels.set(block.id, positions.get(block.id) ?? origin);
    }
    const band = topRight(
      freeNotes.get("") ?? [],
      boundsOf(ordered.map((block) => ({ ...(positions.get(block.id) ?? origin), ...block.size }))),
      origin,
    );
    for (const block of ordered) {
      const at = positions.get(block.id) ?? origin;
      place(block, { x: at.x, y: at.y + band.down });
    }
    for (const [note, at] of band.notes) {
      result.set(note, { x: Math.round(at.x), y: Math.round(at.y) });
    }
    return result;
  };
  layoutAll(undefined);
  for (const [id, at] of levels) {
    firstPass.set(id, at);
  }
  blocks = new Map();
  levels = new Map();
  return layoutAll(input.frame !== undefined ? frames.get(input.frame) : undefined);
}

/**
 * Where Arrange puts the notes on links, once the items they join are placed (`page` as arranged):
 * each beside its line's middle as routed (`routes`, asked for only when a note needs them;
 * `lineMiddle`, `lineNoteSpot`), in id order, clear of the cards, resource cards, collapsed frames,
 * and other notes drawn, the notes it placed before, and the frames the line doesn't start or end in.
 * A note whose line isn't drawn between its own ends (one is hidden in a collapsed frame) stays as it
 * is. `links`, when given, limits it to the notes on those links.
 */
export function lineNoteSpots(
  page: FramedPage & { readonly links: readonly Connection[] },
  sizeOf: (id: string) => Size,
  routes: () => ReadonlyMap<string, RoutedLine>,
  links?: ReadonlySet<string>,
): Record<string, [number, number]> {
  const lines = new Map(page.links.filter((link) => links?.has(link.id) ?? true).map((link) => [link.id, link]));
  const notes = (page.notes ?? []).filter((note) => note.attach !== undefined && lines.has(note.attach)).sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const placing = new Set(notes.map((note) => note.id));
  const { rects } = pageGeometry(page, sizeOf);
  const occupied = [...rects].filter(([id]) => !placing.has(id));
  const spots: Record<string, [number, number]> = {};
  const routed = notes.length > 0 ? routes() : new Map<string, RoutedLine>();
  for (const note of notes) {
    const link = note.attach !== undefined ? lines.get(note.attach) : undefined;
    const route = link && rects.has(link.from) && rects.has(link.to) ? routed.get(link.id) : undefined;
    if (!link || !route || !rects.has(note.id)) {
      continue;
    }
    // The frames around its ends hold the line, so the note may sit in them.
    const around = new Set([...frameChain(page, link.from), ...frameChain(page, link.to)].map((frame) => frame.id));
    const size = sizeOf(note.id);
    const [x, y] = lineNoteSpot(lineMiddle(route.points), size, occupied.flatMap(([id, rect]) => (around.has(id) ? [] : [rect])));
    spots[note.id] = [x, y];
    occupied.push([note.id, { x, y, ...size }]);
  }
  return spots;
}
