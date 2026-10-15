/**
 * Board geometry (TypeScript lays out, docs/ui-shell.md#theme-and-components): reflow when cards change
 * size, where a followed card, a card's callers, a traced start's code, and new cards go, and Arrange by
 * call depth. Pure functions over positions and measured sizes, so they are deterministic and testable
 * without a DOM.
 */

import type { View } from "../ipc/generated";

/** Block cards size to their content between these widths. */
const CARD_MIN_WIDTH = 280;
export const CARD_MAX_WIDTH = 880;
/** A followed card opens this far to the right of the card it was followed from. */
export const FOLLOW_GAP = 80;
/** The vertical center of a card's header, where arrows attach and a followed card's header aligns. */
export const HEADER_PORT = 22;
/** The space reflow keeps between stacked cards. */
export const STACK_GAP = 32;
/**
 * Arrange: the least gap between depth columns, the room each line that turns in a gap takes there,
 * how far a line keeps from a card it passes, and the gap between separate call trees and around a
 * frame stacked with others.
 */
const COLUMN_GAP = 96;
const TRACK_GAP = 8;
const LANE_GAP = 16;
const BAND_GAP = 72;
/** Lines whose ends are this close to level run straight. */
const LEVEL_SNAP = 6;
/** How many down-and-up sweeps ordering the columns takes at most. */
const SWEEPS = 8;
/** New cards opened from the sidebar sit this far right of everything already on the board. */
const NEW_CARD_GAP = 160;

export interface Point {
  readonly x: number;
  readonly y: number;
}

export interface Size {
  readonly width: number;
  readonly height: number;
}

export interface Rect extends Point, Size {}

export interface PlacedCard {
  readonly id: string;
  readonly at: readonly [number, number];
}

/** A size for a card that hasn't been drawn or loaded yet, compact or expanded. */
export function fallbackSize(view: View = "compact"): Size {
  return view === "compact" ? { width: 280, height: 44 } : { width: 520, height: 320 };
}

/** A width clamped to `min`–`max` (default: a block card's), rounded up to whole pixels. */
export function clampCardWidth(width: number, min = CARD_MIN_WIDTH, max = CARD_MAX_WIDTH): number {
  return Math.min(max, Math.max(min, Math.ceil(width)));
}

/**
 * Display positions: saved positions plus the room grown cards above need. A card is pushed down
 * below any card above it whose horizontal range overlaps; it never sits above its saved
 * position, and moves back up when room returns. `pinned` (a card being dragged) is not moved.
 */
export function reflow(
  cards: readonly PlacedCard[],
  sizeOf: (id: string) => Size,
  pinned?: string,
): Map<string, Point> {
  const placed: Rect[] = [];
  const result = new Map<string, Point>();
  const ordered = [...cards].sort((a, b) => a.at[1] - b.at[1] || a.at[0] - b.at[0] || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  for (const card of ordered) {
    const size = sizeOf(card.id);
    const x = card.at[0];
    let y = card.at[1];
    if (card.id !== pinned) {
      for (let moved = true; moved; ) {
        moved = false;
        for (const other of placed) {
          const overlapX = x < other.x + other.width && other.x < x + size.width;
          if (overlapX && y < other.y + other.height + STACK_GAP && y + size.height > other.y) {
            y = other.y + other.height + STACK_GAP;
            moved = true;
          }
        }
      }
    }
    placed.push({ x, y, width: size.width, height: size.height });
    result.set(card.id, { x, y });
  }
  return result;
}

/**
 * Where a followed card opens: `FOLLOW_GAP` to the right of the card it was followed from, with its
 * header level with the calling line (`lineOffset` is the line's center below the source's top).
 */
export function followPlacement(source: Rect, lineOffset: number): [number, number] {
  return [Math.round(source.x + source.width + FOLLOW_GAP), Math.round(source.y + lineOffset - HEADER_PORT)];
}

/**
 * Where Show callers puts a card's callers: a column `FOLLOW_GAP` to its left, each caller's right edge
 * there, stacked from the card's top.
 */
export function callerPlacement(card: Rect, sizes: readonly Size[]): [number, number][] {
  let y = card.y;
  return sizes.map((size) => {
    const at: [number, number] = [Math.round(card.x - FOLLOW_GAP - size.width), Math.round(y)];
    y += size.height + STACK_GAP;
    return at;
  });
}

/**
 * Where tracing an app or entrance puts the cards for its code: a column `FOLLOW_GAP` to the start card's
 * right, in the order they run, stacked from its top.
 */
export function startCodePlacement(start: Rect, sizes: readonly Size[]): [number, number][] {
  let y = start.y;
  return sizes.map((size) => {
    const at: [number, number] = [Math.round(start.x + start.width + FOLLOW_GAP), Math.round(y)];
    y += size.height + STACK_GAP;
    return at;
  });
}

/** Where a card opened from the sidebar goes: to the right of everything, level with the top card. */
export function newCardPlacement(existing: readonly Rect[]): [number, number] {
  if (existing.length === 0) {
    return [0, 0];
  }
  const right = Math.max(...existing.map((rect) => rect.x + rect.width));
  const top = Math.min(...existing.map((rect) => rect.y));
  return [Math.round(right + NEW_CARD_GAP), Math.round(top)];
}

/**
 * `at`, moved down (or up) until a card of `size` there keeps `STACK_GAP` from every rect of
 * `occupied`: where a card placed for an agent goes, so it never covers another.
 */
export function clearSpot(at: Point, size: Size, occupied: readonly Rect[], direction: "down" | "up" = "down"): [number, number] {
  const x = Math.round(at.x);
  let y = Math.round(at.y);
  for (let moved = true; moved; ) {
    moved = false;
    for (const other of occupied) {
      const overlapX = x < other.x + other.width + STACK_GAP && other.x < x + size.width + STACK_GAP;
      if (overlapX && y < other.y + other.height + STACK_GAP && other.y < y + size.height + STACK_GAP) {
        y = Math.round(direction === "down" ? other.y + other.height + STACK_GAP : other.y - STACK_GAP - size.height);
        moved = true;
      }
    }
  }
  return [x, y];
}

/** The rectangle around every rect, or null for none. */
export function boundsOf(rects: readonly Rect[]): Rect | null {
  if (rects.length === 0) {
    return null;
  }
  const left = Math.min(...rects.map((rect) => rect.x));
  const top = Math.min(...rects.map((rect) => rect.y));
  const right = Math.max(...rects.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...rects.map((rect) => rect.y + rect.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

// ---------------------------------------------------------------------------------------------
// Arrange by call depth

export interface ArrangeCard extends Size {
  readonly id: string;
  /** What it is grouped with when it has no lines: its file. */
  readonly group?: string;
  /** A frame: it keeps `BAND_GAP` from what is stacked by it. */
  readonly spaced?: boolean;
}

/** A call between two cards; `port` is the calling line's offset from the source card's top. */
export interface ArrangeCall {
  readonly source: string;
  readonly target: string;
  readonly port: number;
  /** Where it enters the target, below its top (default: its header, `HEADER_PORT`). */
  readonly entry?: number;
  /** Source order inside the calling card, for ties. */
  readonly order: number;
  /** The card it really leaves, when `source` is a block holding it (a frame): a card's several lines count once. */
  readonly from?: string;
}

/** The median of `values` (the middle two's mean when there are two middles); undefined for none. */
function median(values: readonly number[]): number | undefined {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length === 0) {
    return undefined;
  }
  return sorted.length % 2 === 1 ? sorted[middle] : ((sorted[middle - 1] ?? 0) + (sorted[middle] ?? 0)) / 2;
}

/** A node of the layered layout: a card, or a placeholder that keeps a lane for a long line through a column. */
interface LayerNode {
  readonly id: string;
  readonly card: ArrangeCard | undefined;
  readonly band: number;
  readonly column: number;
  /** Its order before sorting: the depth-first order it was reached in (a placeholder's, its line's start's), then its line's order. */
  readonly rank: readonly [number, number];
}

/** A line between nodes in neighboring columns: a whole call, or a piece of a long one. */
interface LayerEdge {
  readonly source: string;
  readonly target: string;
  /** Where it leaves the source, below its top (a placeholder's: 0, its lane). */
  readonly port: number;
  /** Where it enters the target, below its top (a placeholder's: 0). */
  readonly entry: number;
  readonly order: number;
  /** The card the line really leaves (inside a block, the card), and the card its call starts at. */
  readonly from: string;
  readonly origin: string;
}

/** Where `edge` sits along its end in `side`'s node, as a rank in its column: the node's place, plus the line's place among the node's lines. */
type Slots = (edge: LayerEdge, side: "source" | "target") => number;

/** How many times the lines between two neighboring columns cross, for the columns' order (`slot`). */
function crossingsBetween(edges: readonly LayerEdge[], slot: Slots): number {
  let count = 0;
  for (let i = 0; i < edges.length; i += 1) {
    const a = edges[i];
    for (let j = i + 1; a && j < edges.length; j += 1) {
      const b = edges[j];
      if (b && (slot(a, "source") - slot(b, "source")) * (slot(a, "target") - slot(b, "target")) < 0) {
        count += 1;
      }
    }
  }
  return count;
}

/**
 * Lays cards out left to right by call depth from the cards nothing calls (entrances first), as the
 * layered method does (docs/analysis/arranging-and-lines.md, Part 3):
 *
 * 1. Cycles: a depth-first walk from each root, in priority order (`cards` comes in priority order);
 *    a call back into a card still on the walk is left out of the layout.
 * 2. Columns: a callee sits in the column after its deepest caller. Every root starts a band, and
 *    bands stack downward.
 * 3. Lanes: a call that crosses columns gets a placeholder in each column it crosses, so it has its
 *    own lane and runs under no card.
 * 4. Order: each column is sorted, sweeping down and up the columns, by the places of the lines that
 *    reach each node (a card's lines in the order they leave it), keeping the order with the fewest
 *    crossings.
 * 5. Rows: column by column, in that order, each node sits level with the median of the lines that
 *    reach it (a card's first, when it calls it more than once), never above its topmost caller, and
 *    below the node before it: a lane `LANE_GAP` from a card, a frame (`spaced`) `BAND_GAP` from
 *    what is stacked by it, else `STACK_GAP`.
 * 6. Columns are as wide as their widest card, `COLUMN_GAP` apart, wider where many lines turn.
 *
 * Cards without calls go in a compact grid underneath, grouped by file. The result depends only on
 * the cards, their sizes, and the calls, never on where cards are now, so arranging twice changes
 * nothing.
 */
export function arrange(cards: readonly ArrangeCard[], calls: readonly ArrangeCall[], origin: Point = { x: 0, y: 0 }): Map<string, Point> {
  const byId = new Map(cards.map((card) => [card.id, card]));
  const priority = new Map(cards.map((card, index) => [card.id, index]));
  const valid = calls.filter((call) => call.source !== call.target && byId.has(call.source) && byId.has(call.target));
  const outgoing = new Map<string, ArrangeCall[]>();
  for (const call of valid) {
    const list = outgoing.get(call.source) ?? [];
    list.push(call);
    outgoing.set(call.source, list);
  }
  for (const list of outgoing.values()) {
    list.sort((a, b) => a.port - b.port || a.order - b.order || (priority.get(a.target) ?? 0) - (priority.get(b.target) ?? 0));
  }
  const called = new Set(valid.map((call) => call.target));
  const connected = new Set(valid.flatMap((call) => [call.source, call.target]));

  // 1. Depth first from each root, in order. An arrow into a card still on the stack is a back edge.
  const state = new Map<string, "open" | "done">();
  const band = new Map<string, number>();
  const preorder = new Map<string, number>();
  const postorder: string[] = [];
  const back = new Set<ArrangeCall>();
  const visit = (root: string, bandIndex: number) => {
    // Iterative, so a long call chain can't overflow the stack.
    const stack: { id: string; next: number }[] = [{ id: root, next: 0 }];
    state.set(root, "open");
    band.set(root, bandIndex);
    preorder.set(root, preorder.size);
    while (stack.length > 0) {
      const frame = stack[stack.length - 1];
      if (!frame) {
        break;
      }
      const list = outgoing.get(frame.id) ?? [];
      const call = list[frame.next];
      if (!call) {
        state.set(frame.id, "done");
        postorder.push(frame.id);
        stack.pop();
        continue;
      }
      frame.next += 1;
      const seen = state.get(call.target);
      if (seen === "open") {
        back.add(call);
      } else if (seen === undefined) {
        state.set(call.target, "open");
        band.set(call.target, bandIndex);
        preorder.set(call.target, preorder.size);
        stack.push({ id: call.target, next: 0 });
      }
    }
  };
  let bands = 0;
  for (const card of cards) {
    if (connected.has(card.id) && !called.has(card.id) && !state.has(card.id)) {
      visit(card.id, bands++);
    }
  }
  for (const card of cards) {
    if (connected.has(card.id) && !state.has(card.id)) {
      visit(card.id, bands++);
    }
  }
  const forward = valid.filter((call) => !back.has(call));

  // 2. Longest path, so every forward arrow points right and a shared callee sits past its deepest caller.
  const incoming = new Map<string, ArrangeCall[]>();
  for (const call of forward) {
    const list = incoming.get(call.target) ?? [];
    list.push(call);
    incoming.set(call.target, list);
  }
  const depth = new Map<string, number>();
  for (const id of [...postorder].reverse()) {
    const callers = incoming.get(id) ?? [];
    depth.set(id, callers.reduce((deepest, call) => Math.max(deepest, (depth.get(call.source) ?? 0) + 1), 0));
  }
  const columns = connected.size > 0 ? Math.max(...depth.values()) + 1 : 0;

  // 3. The layered graph: the cards, and a placeholder in each column a call crosses.
  const nodes = new Map<string, LayerNode>();
  for (const card of cards) {
    if (connected.has(card.id)) {
      nodes.set(card.id, { id: card.id, card, band: band.get(card.id) ?? 0, column: depth.get(card.id) ?? 0, rank: [preorder.get(card.id) ?? 0, 0] });
    }
  }
  const edges: LayerEdge[] = [];
  forward.forEach((call, index) => {
    const from = call.from ?? call.source;
    const start = depth.get(call.source) ?? 0;
    const end = depth.get(call.target) ?? 0;
    const order = (outgoing.get(call.source) ?? []).indexOf(call);
    let previous = call.source;
    let port = call.port;
    for (let column = start + 1; column < end; column += 1) {
      const id = `\u0000lane:${index}:${column}`;
      nodes.set(id, { id, card: undefined, band: band.get(call.source) ?? 0, column, rank: [preorder.get(call.source) ?? 0, order + 1] });
      edges.push({ source: previous, target: id, port, entry: 0, order, from, origin: call.source });
      previous = id;
      port = 0;
    }
    edges.push({ source: previous, target: call.target, port, entry: call.entry ?? HEADER_PORT, order, from, origin: call.source });
  });

  // 4. Order: the initial order is the depth-first one; then sweeps sort each column by where its lines come from (down) or go (up).
  const layers: string[][] = Array.from({ length: columns }, () => []);
  for (const node of [...nodes.values()].sort((a, b) => a.band - b.band || a.rank[0] - b.rank[0] || a.rank[1] - b.rank[1])) {
    layers[node.column]?.push(node.id);
  }
  const ins = new Map<string, LayerEdge[]>();
  const outs = new Map<string, LayerEdge[]>();
  for (const edge of edges) {
    ins.set(edge.target, [...(ins.get(edge.target) ?? []), edge]);
    outs.set(edge.source, [...(outs.get(edge.source) ?? []), edge]);
  }
  for (const list of outs.values()) {
    list.sort((a, b) => a.port - b.port || a.order - b.order);
  }
  for (const list of ins.values()) {
    list.sort((a, b) => a.entry - b.entry || a.order - b.order);
  }
  const place = new Map<string, number>();
  const number = () => {
    for (const layer of layers) {
      layer.forEach((id, index) => place.set(id, index));
    }
  };
  number();
  // A line's place at its end: its node's place, plus its place among that node's lines (in (0, 1)).
  const slot: Slots = (edge, side) => {
    const node = side === "source" ? edge.source : edge.target;
    const list = (side === "source" ? outs : ins).get(node) ?? [];
    return (place.get(node) ?? 0) + (list.indexOf(edge) + 1) / (list.length + 1);
  };
  const gaps: LayerEdge[][] = Array.from({ length: Math.max(0, columns - 1) }, () => []);
  for (const edge of edges) {
    gaps[nodes.get(edge.source)?.column ?? 0]?.push(edge);
  }
  const crossings = () => gaps.reduce((sum, gap) => sum + crossingsBetween(gap, slot), 0);
  /** Sorts a column by `key` (nodes without one keep their places), never across bands. */
  const sortLayer = (layer: string[], key: (id: string) => number | undefined) => {
    const keyed = layer.map((id, index) => ({ id, index, band: nodes.get(id)?.band ?? 0, key: key(id) }));
    const movable = keyed.filter((item) => item.key !== undefined).sort((a, b) => a.band - b.band || (a.key ?? 0) - (b.key ?? 0) || a.index - b.index);
    const fixed = new Map(keyed.filter((item) => item.key === undefined).map((item) => [item.index, item.id]));
    let next = 0;
    for (let index = 0; index < layer.length; index += 1) {
      const kept = fixed.get(index);
      layer[index] = kept ?? movable[next++]?.id ?? layer[index] ?? "";
    }
  };
  const mean = (values: readonly number[]) => (values.length > 0 ? values.reduce((sum, value) => sum + value, 0) / values.length : undefined);
  let best = layers.map((layer) => [...layer]);
  let fewest = crossings();
  for (let sweep = 0; sweep < SWEEPS && fewest > 0; sweep += 1) {
    for (let column = 1; column < columns; column += 1) {
      sortLayer(layers[column] ?? [], (id) => mean((ins.get(id) ?? []).map((edge) => slot(edge, "source"))));
      number();
    }
    for (let column = columns - 2; column >= 0; column -= 1) {
      sortLayer(layers[column] ?? [], (id) => mean((outs.get(id) ?? []).map((edge) => slot(edge, "target"))));
      number();
    }
    const now = crossings();
    if (now >= fewest) {
      break;
    }
    fewest = now;
    best = layers.map((layer) => [...layer]);
  }
  layers.splice(0, layers.length, ...best);
  number();

  // 5. Rows: band by band, column by column, each node level with the lines that reach it.
  const height = (id: string) => nodes.get(id)?.card?.height ?? 0;
  const gapBetween = (above: string, below: string): number => {
    const [a, b] = [nodes.get(above)?.card, nodes.get(below)?.card];
    if (!a || !b) {
      return a || b ? LANE_GAP : TRACK_GAP;
    }
    return a.spaced === true || b.spaced === true ? BAND_GAP : STACK_GAP;
  };
  const tops = new Map<string, number>();
  let bottom = origin.y - BAND_GAP;
  for (let current = 0; current < bands; current += 1) {
    const bandTop = bottom + BAND_GAP;
    for (const layer of layers) {
      let above: string | undefined;
      for (const id of layer) {
        if (nodes.get(id)?.band !== current) {
          continue;
        }
        const placed = (ins.get(id) ?? []).filter((edge) => tops.has(edge.source));
        // A card's several lines count once: its first.
        const first = new Map<string, LayerEdge>();
        for (const edge of [...placed].sort((a, b) => a.order - b.order || a.port - b.port)) {
          if (!first.has(edge.from)) {
            first.set(edge.from, edge);
          }
        }
        const middle = median([...first.values()].map((edge) => (tops.get(edge.source) ?? bandTop) + edge.port - edge.entry));
        const topmost = Math.min(...placed.map((edge) => tops.get(edge.origin) ?? bandTop));
        let y = middle === undefined ? bandTop : Math.max(bandTop, topmost, middle);
        if (above !== undefined) {
          y = Math.max(y, (tops.get(above) ?? bandTop) + height(above) + gapBetween(above, id));
        }
        tops.set(id, Math.round(y));
        above = id;
        bottom = Math.max(bottom, y + height(id));
      }
    }
  }

  // 6. Columns: as wide as their widest card, apart by the lines that turn between them.
  const columnWidth = layers.map((layer) => Math.max(0, ...layer.map((id) => nodes.get(id)?.card?.width ?? 0)));
  const columnX: number[] = [];
  let x = origin.x;
  for (let column = 0; column < columns; column += 1) {
    columnX.push(Math.round(x));
    const turning = (gaps[column] ?? []).filter((edge) => Math.abs((tops.get(edge.source) ?? 0) + edge.port - (tops.get(edge.target) ?? 0) - edge.entry) > LEVEL_SNAP).length;
    x += (columnWidth[column] ?? 0) + Math.max(COLUMN_GAP, 2 * LANE_GAP + Math.max(0, turning - 1) * TRACK_GAP);
  }

  const positions = new Map<string, Point>();
  for (const node of nodes.values()) {
    if (node.card) {
      positions.set(node.id, { x: columnX[node.column] ?? origin.x, y: tops.get(node.id) ?? origin.y });
    }
  }

  // Cards with no calls: a compact grid under everything else, a file's cards together.
  const loose = cards.filter((card) => !connected.has(card.id));
  const top = connected.size > 0 ? bottom + BAND_GAP : origin.y;
  for (const [id, at] of looseGrid(loose, { x: origin.x, y: top })) {
    positions.set(id, at);
  }
  return positions;
}

/**
 * Items with no lines in a compact grid from `origin`: about as many columns as rows, filled row by row
 * in file order (then priority), each column as wide as its widest item and each row as tall as its
 * tallest, `STACK_GAP` apart.
 */
function looseGrid(items: readonly ArrangeCard[], origin: Point): Map<string, Point> {
  const ordered = items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => ((a.item.group ?? "") < (b.item.group ?? "") ? -1 : (a.item.group ?? "") > (b.item.group ?? "") ? 1 : a.index - b.index))
    .map(({ item }) => item);
  const across = Math.max(1, Math.ceil(Math.sqrt(ordered.length)));
  const widths = Array.from({ length: across }, (_, column) => Math.max(0, ...ordered.filter((_, index) => index % across === column).map((item) => item.width)));
  const heights = Array.from({ length: Math.ceil(ordered.length / across) }, (_, row) => Math.max(0, ...ordered.slice(row * across, (row + 1) * across).map((item) => item.height)));
  const positions = new Map<string, Point>();
  ordered.forEach((item, index) => {
    const column = index % across;
    const row = Math.floor(index / across);
    const x = origin.x + widths.slice(0, column).reduce((sum, width) => sum + width + STACK_GAP, 0);
    const y = origin.y + heights.slice(0, row).reduce((sum, height) => sum + height + STACK_GAP, 0);
    positions.set(item.id, { x: Math.round(x), y: Math.round(y) });
  });
  return positions;
}
