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
/** Arrange: the gap between depth columns (it carries the arrows) and between separate call trees. */
const COLUMN_GAP = 120;
const BAND_GAP = 72;
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

/**
 * Lays cards out left to right by call depth from the cards nothing calls (entrances first).
 * `cards` comes in priority order; every root starts a band, and bands stack downward. A callee
 * sits in the column after its deepest caller, its entry (its header) level with the median of the
 * lines that reach it when there is room (a card's first, when it calls it more than once; ties in
 * source order). Cards without calls go in a compact grid underneath, grouped by file. The result
 * depends only on the cards, their sizes, and the calls, never on where cards are now, so arranging
 * twice changes nothing.
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

  // Depth first from each root, in order. An arrow into a card still on the stack is a back edge.
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

  // Longest path, so every forward arrow points right and a shared callee sits past its deepest caller.
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
  const columnWidth = Array.from({ length: columns }, (_, column) =>
    cards.filter((card) => depth.get(card.id) === column).reduce((widest, card) => Math.max(widest, card.width), 0),
  );
  const columnX: number[] = [];
  let x = origin.x;
  for (let column = 0; column < columns; column += 1) {
    columnX.push(Math.round(x));
    x += (columnWidth[column] ?? 0) + COLUMN_GAP;
  }

  const positions = new Map<string, Point>();
  const placed: { y: number; height: number }[][] = Array.from({ length: columns }, () => []);
  const fit = (column: number, want: number, height: number): number => {
    let top = want;
    for (let moved = true; moved; ) {
      moved = false;
      for (const other of placed[column] ?? []) {
        if (top < other.y + other.height + STACK_GAP && top + height + STACK_GAP > other.y) {
          top = other.y + other.height + STACK_GAP;
          moved = true;
        }
      }
    }
    return top;
  };
  let bottom = origin.y - BAND_GAP;
  for (let current = 0; current < bands; current += 1) {
    const bandTop = bottom + BAND_GAP;
    for (let column = 0; column < columns; column += 1) {
      const members = cards
        .filter((card) => band.get(card.id) === current && depth.get(card.id) === column)
        .map((card) => {
          const callers = (incoming.get(card.id) ?? []).filter((call) => positions.has(call.source));
          const order = preorder.get(card.id) ?? 0;
          // Level with the middle of the lines that reach it (a card's first line, when it has several),
          // never above its topmost caller.
          const first = new Map<string, ArrangeCall>();
          for (const call of [...callers].sort((a, b) => a.order - b.order || a.port - b.port)) {
            const from = call.from ?? call.source;
            if (!first.has(from)) {
              first.set(from, call);
            }
          }
          const lines = [...first.values()].map((call) => (positions.get(call.source)?.y ?? bandTop) + call.port - (call.entry ?? HEADER_PORT));
          const middle = median(lines);
          if (middle === undefined) {
            return { card, want: bandTop, order };
          }
          const lowestCaller = Math.min(...callers.map((call) => positions.get(call.source)?.y ?? bandTop));
          return { card, want: Math.max(bandTop, lowestCaller, middle), order };
        })
        .sort((a, b) => a.want - b.want || a.order - b.order);
      const columnLeft = columnX[column] ?? origin.x;
      for (const { card, want } of members) {
        const y = Math.round(fit(column, want, card.height));
        positions.set(card.id, { x: columnLeft, y });
        placed[column]?.push({ y, height: card.height });
        bottom = Math.max(bottom, y + card.height);
      }
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
