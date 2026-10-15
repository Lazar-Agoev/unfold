/**
 * The page's lines, routed (docs/analysis/arranging-and-lines.md, Part 5): every line is orthogonal,
 * leaves and enters by the sides its ends allow, and goes around every box it neither starts nor ends
 * at (`boxes`: cards, resource cards, notes, collapsed frames, and the frames that hold neither end).
 *
 * - Each line is found on a sparse grid (the boxes' edges `CLEAR` out, the lines' ends, and the
 *   middles between them) as the cheapest path: its length, `BEND` for each bend, `CROSS` for each
 *   line it crosses. Short lines go first. Where an end may use several sides, the cheapest pair wins:
 *   fewest bends, then the shortest.
 * - Ends: a fixed end sits where it says (a call's row, a column's). The others on one side of a box
 *   spread along it `SPREAD` apart, from the side's header (top and bottom: around the middle), in the
 *   order of where their lines come from. Lines that merge (arrows into one card) share their end and
 *   join into one trunk before it. A line whose ends are within `SNAP` of level runs straight.
 * - Then lines that share a run each get their own track, `TRACK` apart, in the order that crosses
 *   least; the lines from one end, or merging into one, stay together.
 *
 * Pure and deterministic: the same boxes and lines always route the same way.
 */

import type { Point, Rect } from "../layout";

/** A side of a box a line leaves or enters by. */
export type Side = "left" | "right" | "top" | "bottom";

/** One end of a line: the box it leaves or enters, the sides it may use, and where on the side. */
export interface LineEnd {
  readonly item: string;
  /** The sides it may use; the router picks the cheapest. */
  readonly sides: readonly Side[];
  /** Where along a left or right side, below the box's top (a call's row, a column's); absent: spread from the header (top and bottom: around the middle). */
  readonly at?: number;
}

/** A line to route. */
export interface LineRequest {
  readonly id: string;
  readonly from: LineEnd;
  readonly to: LineEnd;
  /** Lines that merge into one end (arrows into one card) share it and join into one trunk; others spread along the side. */
  readonly merge: boolean;
}

/** A routed line: its points from its start to its end, and the sides it leaves and enters by. */
export interface RoutedLine {
  readonly points: readonly Point[];
  readonly from: Side;
  readonly to: Side;
}

export interface RouteInput {
  /** Every drawn box by id: cards, resource cards, notes, collapsed frames' nodes, and expanded frames' boxes. */
  readonly boxes: ReadonlyMap<string, Rect>;
  /** Each expanded frame and what is drawn inside it: a line stays out of a frame unless an end is in it. */
  readonly frames: ReadonlyMap<string, readonly string[]>;
  readonly lines: readonly LineRequest[];
  /** Where each box's header line is, below its top: a side's first spread end (default `HEADER`). */
  readonly headers?: ReadonlyMap<string, number>;
}

/** How far a line keeps from a box it passes, and how far it runs straight out of its end first. */
const CLEAR = 10;
const STUB = 14;
/** What a bend and a crossing cost, in pixels of length; a pixel along another line's run costs `ALONG` more. */
const BEND = 40;
const CROSS = 200;
const ALONG = 0.6;
/** Lines sharing a run are this far apart; ends spread along a side this far apart. */
const TRACK = 8;
const SPREAD = 14;
/** A line whose ends are this close to level runs straight. */
const SNAP = 6;
/** A side's first spread end, below the box's top, unless `headers` says otherwise. */
const HEADER = 22;
/** How close a nudged run may come to a box, and to the end it leaves from. */
const MARGIN = 3;

/** The four ways a line can go, as unit vectors: east, west, south, north. */
const DIRECTIONS: readonly (readonly [number, number])[] = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
];

/** Which way a line leaves a box by `side`. */
const OUT: Readonly<Record<Side, readonly [number, number]>> = { left: [-1, 0], right: [1, 0], top: [0, -1], bottom: [0, 1] };

const EMPTY: Rect = { x: 0, y: 0, width: 0, height: 0 };

function grown(rect: Rect, by: number): Rect {
  return { x: rect.x - by, y: rect.y - by, width: rect.width + 2 * by, height: rect.height + 2 * by };
}

/** Rounded to the half pixel, the grid's resolution. */
function half(value: number): number {
  return Math.round(value * 2) / 2;
}

/** A point on `side` of `rect`: `at` below its top on the left or right; on the top or bottom, `along` from its middle. */
function sidePoint(rect: Rect, side: Side, at: number, along: number): Point {
  switch (side) {
    case "left":
      return { x: rect.x, y: half(rect.y + at) };
    case "right":
      return { x: rect.x + rect.width, y: half(rect.y + at) };
    case "top":
      return { x: half(rect.x + rect.width / 2 + along), y: rect.y };
    case "bottom":
      return { x: half(rect.x + rect.width / 2 + along), y: rect.y + rect.height };
  }
}

/** A binary min-heap of numbers by cost. */
class Heap {
  private readonly items: number[] = [];
  private readonly costs: number[] = [];

  get size(): number {
    return this.items.length;
  }

  push(item: number, cost: number): void {
    const { items, costs } = this;
    let at = items.length;
    items.push(item);
    costs.push(cost);
    while (at > 0) {
      const parent = (at - 1) >> 1;
      if ((costs[parent] ?? 0) <= cost) {
        break;
      }
      items[at] = items[parent] ?? 0;
      costs[at] = costs[parent] ?? 0;
      at = parent;
    }
    items[at] = item;
    costs[at] = cost;
  }

  pop(): number {
    const { items, costs } = this;
    const top = items[0] ?? 0;
    const item = items.pop() ?? 0;
    const cost = costs.pop() ?? 0;
    if (items.length === 0) {
      return top;
    }
    let at = 0;
    for (;;) {
      const left = 2 * at + 1;
      if (left >= items.length) {
        break;
      }
      const right = left + 1;
      const least = right < items.length && (costs[right] ?? 0) < (costs[left] ?? 0) ? right : left;
      if ((costs[least] ?? 0) >= cost) {
        break;
      }
      items[at] = items[least] ?? 0;
      costs[at] = costs[least] ?? 0;
      at = least;
    }
    items[at] = item;
    costs[at] = cost;
    return top;
  }
}

/** The sorted distinct values (to the half pixel), with the middle between each two farther apart than `2 * CLEAR`. */
function axis(values: Iterable<number>): number[] {
  const sorted = [...new Set([...values].map(half))].sort((a, b) => a - b);
  const out: number[] = [];
  sorted.forEach((value, index) => {
    const next = sorted[index + 1];
    out.push(value);
    if (next !== undefined && next - value > 2 * CLEAR) {
      out.push(half((value + next) / 2));
    }
  });
  return out;
}

/** The index of the value nearest `value` in sorted `values`. */
function nearest(values: readonly number[], value: number): number {
  let lo = 0;
  let hi = values.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((values[mid] ?? 0) < value) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo > 0 && Math.abs((values[lo - 1] ?? 0) - value) < Math.abs((values[lo] ?? 0) - value) ? lo - 1 : lo;
}

/** The grid lines are routed on: its coordinates, the steps each layer of boxes blocks, and the runs of lines routed so far. */
class Grid {
  readonly xs: number[];
  readonly ys: number[];
  /** Per layer (0: the boxes every line avoids; then one per frame): whether the step east of, or south of, each node is blocked. */
  private readonly east: Uint8Array[] = [];
  private readonly south: Uint8Array[] = [];
  /** The bundles of the lines routed through each node, running across (horizontally) and down (vertically). */
  readonly runs: readonly [Map<number, Set<string>>, Map<number, Set<string>>] = [new Map(), new Map()];

  constructor(xs: Iterable<number>, ys: Iterable<number>, layers: readonly (readonly Rect[])[]) {
    this.xs = axis(xs);
    this.ys = axis(ys);
    const width = this.xs.length;
    for (const rects of layers) {
      const east = new Uint8Array(width * this.ys.length);
      const south = new Uint8Array(width * this.ys.length);
      for (const rect of rects) {
        const [x0, x1, y0, y1] = [rect.x, rect.x + rect.width, rect.y, rect.y + rect.height];
        // Only the rows and columns the box covers, give or take one.
        const [c0, c1] = [Math.max(0, nearest(this.xs, x0) - 1), Math.min(width - 1, nearest(this.xs, x1) + 1)];
        const [r0, r1] = [Math.max(0, nearest(this.ys, y0) - 1), Math.min(this.ys.length - 1, nearest(this.ys, y1) + 1)];
        for (let row = r0; row <= r1; row += 1) {
          const y = this.ys[row] ?? 0;
          for (let column = c0; y > y0 && y < y1 && column + 1 < width && column <= c1; column += 1) {
            const mid = ((this.xs[column] ?? 0) + (this.xs[column + 1] ?? 0)) / 2;
            if (mid > x0 && mid < x1) {
              east[row * width + column] = 1;
            }
          }
        }
        for (let column = c0; column <= c1; column += 1) {
          const x = this.xs[column] ?? 0;
          for (let row = r0; x > x0 && x < x1 && row + 1 < this.ys.length && row <= r1; row += 1) {
            const mid = ((this.ys[row] ?? 0) + (this.ys[row + 1] ?? 0)) / 2;
            if (mid > y0 && mid < y1) {
              south[row * width + column] = 1;
            }
          }
        }
      }
      this.east.push(east);
      this.south.push(south);
    }
  }

  node(point: Point): number {
    return nearest(this.ys, point.y) * this.xs.length + nearest(this.xs, point.x);
  }

  point(node: number): Point {
    return { x: this.xs[node % this.xs.length] ?? 0, y: this.ys[Math.floor(node / this.xs.length)] ?? 0 };
  }

  /** The node one step from `node` in direction `d` (east, west, south, north), or -1 off the grid or blocked by `layers`. */
  step(node: number, d: number, layers: readonly number[]): number {
    const width = this.xs.length;
    const column = node % width;
    const row = Math.floor(node / width);
    let to: number;
    let from: number;
    if (d === 0) {
      [to, from] = column + 1 < width ? [node + 1, node] : [-1, -1];
    } else if (d === 1) {
      [to, from] = column > 0 ? [node - 1, node - 1] : [-1, -1];
    } else if (d === 2) {
      [to, from] = row + 1 < this.ys.length ? [node + width, node] : [-1, -1];
    } else {
      [to, from] = row > 0 ? [node - width, node - width] : [-1, -1];
    }
    if (to < 0) {
      return -1;
    }
    const blocks = d < 2 ? this.east : this.south;
    for (const layer of layers) {
      if (blocks[layer]?.[from] === 1) {
        return -1;
      }
    }
    return to;
  }

  /** Marks a routed line's runs, so later lines pay to cross them, and share a bundle's. */
  mark(points: readonly Point[], bundle: string): void {
    for (let index = 1; index < points.length; index += 1) {
      const [a, b] = [points[index - 1], points[index]];
      if (!a || !b) {
        continue;
      }
      const across = Math.abs(a.y - b.y) < 0.01;
      const values = across ? this.xs : this.ys;
      const [lo, hi] = across ? [Math.min(a.x, b.x), Math.max(a.x, b.x)] : [Math.min(a.y, b.y), Math.max(a.y, b.y)];
      for (let at = nearest(values, lo); at < values.length && (values[at] ?? Infinity) <= hi + 0.01; at += 1) {
        const node = across ? this.node({ x: values[at] ?? 0, y: a.y }) : this.node({ x: a.x, y: values[at] ?? 0 });
        const map = this.runs[across ? 0 : 1];
        const set = map.get(node) ?? new Set<string>();
        set.add(bundle);
        map.set(node, set);
      }
    }
  }
}

/** One end of a line, placed: its side, its point on the box, and its stub's point out of it. */
interface PlacedEnd {
  readonly side: Side;
  readonly point: Point;
  readonly stub: Point;
}

/** A line's polyline without repeated points or points in the middle of a straight run. */
function simplify(points: readonly Point[]): Point[] {
  const out: Point[] = [];
  for (const point of points) {
    const last = out[out.length - 1];
    if (last && Math.abs(last.x - point.x) < 0.01 && Math.abs(last.y - point.y) < 0.01) {
      continue;
    }
    const before = out[out.length - 2];
    const straight = (a: Point, b: Point, c: Point) => (Math.abs(a.x - b.x) < 0.01 && Math.abs(b.x - c.x) < 0.01) || (Math.abs(a.y - b.y) < 0.01 && Math.abs(b.y - c.y) < 0.01);
    if (last && before && straight(before, last, point)) {
      out[out.length - 1] = point;
      continue;
    }
    out.push(point);
  }
  return out;
}

/** A polyline's length. */
function lengthOf(points: readonly Point[]): number {
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    const [a, b] = [points[index - 1], points[index]];
    total += a && b ? Math.abs(a.x - b.x) + Math.abs(a.y - b.y) : 0;
  }
  return total;
}

/**
 * The cheapest path on `grid` from `from`'s stub to `to`'s, around `layers`: its points from end to end,
 * and its cost (Infinity where there is none: then it runs straight between the stubs). Running along
 * the runs of its `bundles` (lines from its start, or into its end) is cheap, and crossing them free,
 * so they leave as a tree and join into one trunk.
 */
function search(grid: Grid, from: PlacedEnd, to: PlacedEnd, layers: readonly number[], bundles: readonly string[]): { points: Point[]; cost: number } {
  const mine = (set: ReadonlySet<string>) => bundles.some((bundle) => set.has(bundle));
  const start = grid.node(from.stub);
  const goal = grid.node(to.stub);
  const target = grid.point(goal);
  const out = DIRECTIONS.findIndex(([dx, dy]) => dx === OUT[from.side][0] && dy === OUT[from.side][1]);
  const into = DIRECTIONS.findIndex(([dx, dy]) => dx === -OUT[to.side][0] && dy === -OUT[to.side][1]);
  const best = new Map<number, number>();
  const came = new Map<number, number>();
  const heap = new Heap();
  const h = (node: number) => {
    const point = grid.point(node);
    return Math.abs(point.x - target.x) + Math.abs(point.y - target.y);
  };
  best.set(start * 4 + out, 0);
  heap.push(start * 4 + out, h(start));
  let found = -1;
  let foundCost = Infinity;
  while (heap.size > 0) {
    const current = heap.pop();
    const node = current >> 2;
    const d = current & 3;
    const cost = best.get(current) ?? Infinity;
    if (cost + h(node) >= foundCost) {
      break;
    }
    if (node === goal) {
      const total = cost + (d === into ? 0 : BEND);
      if (total < foundCost) {
        found = current;
        foundCost = total;
      }
      continue;
    }
    const here = grid.point(node);
    for (let next = 0; next < 4; next += 1) {
      if (next >> 1 === d >> 1 && next !== d) {
        continue;
      }
      const neighbor = grid.step(node, next, layers);
      if (neighbor < 0) {
        continue;
      }
      const there = grid.point(neighbor);
      const length = Math.abs(here.x - there.x) + Math.abs(here.y - there.y);
      let step = length + (next === d ? 0 : BEND);
      if (neighbor !== goal) {
        const crossed = grid.runs[next < 2 ? 1 : 0].get(neighbor);
        if (crossed && !mine(crossed)) {
          step += CROSS;
        }
        const along = grid.runs[next < 2 ? 0 : 1].get(neighbor);
        if (along) {
          step += mine(along) ? -length / 2 : length * ALONG;
        }
      }
      const key = neighbor * 4 + next;
      const total = cost + Math.max(step, 0.01);
      if (total < (best.get(key) ?? Infinity)) {
        best.set(key, total);
        came.set(key, current);
        heap.push(key, total + h(neighbor));
      }
    }
  }
  if (found < 0) {
    return { points: simplify([from.point, from.stub, to.stub, to.point]), cost: Infinity };
  }
  const nodes: Point[] = [];
  for (let at: number | undefined = found; at !== undefined; at = came.get(at)) {
    nodes.push(grid.point(at >> 2));
  }
  nodes.reverse();
  return { points: simplify([from.point, from.stub, ...nodes, to.stub, to.point]), cost: foundCost };
}

/** A straight run of a routed line: the points it joins (`index`, `index + 1`), its axis and place, and its extent. */
interface Run {
  readonly line: string;
  readonly index: number;
  /** Across (horizontal: `at` is its y) or down (vertical: `at` is its x). */
  readonly across: boolean;
  readonly at: number;
  readonly lo: number;
  readonly hi: number;
}

/**
 * Gives lines that share a run their own tracks, `TRACK` apart around where they ran (closer where boxes
 * leave less room), in the order that crosses least. Runs of one bundle (lines from one end, or merging
 * into one) stay together. A line's first and last runs (out of and into its ends) stay put.
 */
function nudge(routed: Map<string, Point[]>, bundles: ReadonlyMap<string, readonly string[]>, boxes: readonly Rect[]): void {
  for (const across of [false, true]) {
    const runs: Run[] = [];
    for (const [line, points] of routed) {
      for (let index = 1; index + 2 < points.length; index += 1) {
        const [a, b] = [points[index], points[index + 1]];
        if (!a || !b || (across ? Math.abs(a.y - b.y) > 0.01 : Math.abs(a.x - b.x) > 0.01)) {
          continue;
        }
        const [lo, hi] = across ? [Math.min(a.x, b.x), Math.max(a.x, b.x)] : [Math.min(a.y, b.y), Math.max(a.y, b.y)];
        runs.push({ line, index, across, at: across ? a.y : a.x, lo, hi });
      }
    }
    // Runs on one line of the grid that overlap share it.
    const byPlace = new Map<number, Run[]>();
    for (const run of runs) {
      byPlace.set(half(run.at), [...(byPlace.get(half(run.at)) ?? []), run]);
    }
    for (const [place, onLine] of [...byPlace].sort(([a], [b]) => a - b)) {
      for (const group of overlapping(onLine)) {
        separate(routed, group, place, bundles, boxes);
      }
    }
  }
}

/** The groups of runs that overlap one another (by more than a pixel), each in order along the line. */
function overlapping(runs: readonly Run[]): Run[][] {
  const sorted = [...runs].sort((a, b) => a.lo - b.lo || (a.line < b.line ? -1 : a.line > b.line ? 1 : a.index - b.index));
  const groups: Run[][] = [];
  let reach = -Infinity;
  for (const run of sorted) {
    const last = groups[groups.length - 1];
    if (last && run.lo < reach - 1) {
      last.push(run);
      reach = Math.max(reach, run.hi);
    } else {
      groups.push([run]);
      reach = run.hi;
    }
  }
  return groups;
}

/** Where a run's line goes on past each of its two ends: the place along the run, and the side it turns to (-1 or 1), or 0 at an end. */
function turns(points: readonly Point[], run: Run): [readonly [number, number], readonly [number, number]] {
  const turn = (end: number, other: number): readonly [number, number] => {
    const point = points[end];
    const next = points[other];
    if (!point || !next) {
      return [0, 0];
    }
    return run.across ? [point.x, Math.sign(next.y - point.y)] : [point.y, Math.sign(next.x - point.x)];
  };
  return [turn(run.index, run.index - 1), turn(run.index + 1, run.index + 2)];
}

/** Moves the runs of one group onto tracks of their own (`nudge`). */
function separate(routed: Map<string, Point[]>, group: readonly Run[], place: number, bundles: ReadonlyMap<string, readonly string[]>, boxes: readonly Rect[]): void {
  // Units: runs of one bundle that overlap move together.
  const units: Run[][] = [];
  for (const run of group) {
    const mine = bundles.get(run.line) ?? [];
    const unit = units.find((other) => other.some((item) => item.lo < run.hi - 1 && run.lo < item.hi - 1 && (bundles.get(item.line) ?? []).some((bundle) => mine.includes(bundle))));
    if (unit) {
      unit.push(run);
    } else {
      units.push([run]);
    }
  }
  if (units.length < 2) {
    return;
  }
  // The crossings when unit a's track is before b's (smaller x, or smaller y).
  const crossings = (a: readonly Run[], b: readonly Run[]): number => {
    let count = 0;
    for (const run of a) {
      for (const [at, side] of turns(routed.get(run.line) ?? [], run)) {
        // a's line turns toward b's track (+1) at `at`: it crosses b's runs that reach past it.
        if (side > 0 && b.some((other) => other.lo < at - 0.5 && at + 0.5 < other.hi)) {
          count += 1;
        }
      }
    }
    for (const run of b) {
      for (const [at, side] of turns(routed.get(run.line) ?? [], run)) {
        if (side < 0 && a.some((other) => other.lo < at - 0.5 && at + 0.5 < other.hi)) {
          count += 1;
        }
      }
    }
    return count;
  };
  // Insertion order by the fewest crossings with each unit before it.
  const ordered: Run[][] = [];
  for (const unit of units) {
    let at = ordered.length;
    while (at > 0) {
      const before = ordered[at - 1];
      if (!before || crossings(before, unit) <= crossings(unit, before)) {
        break;
      }
      at -= 1;
    }
    ordered.splice(at, 0, unit);
  }
  // The room around the place: up to the nearest box each way along the group's extent.
  const lo = Math.min(...group.map((run) => run.lo));
  const hi = Math.max(...group.map((run) => run.hi));
  const across = group[0]?.across ?? false;
  let low = -Infinity;
  let high = Infinity;
  for (const box of boxes) {
    const [from, to, near, far] = across ? [box.x, box.x + box.width, box.y, box.y + box.height] : [box.y, box.y + box.height, box.x, box.x + box.width];
    if (to <= lo || from >= hi) {
      continue;
    }
    if (far <= place) {
      low = Math.max(low, far + MARGIN);
    } else if (near >= place) {
      high = Math.min(high, near - MARGIN);
    }
  }
  const room = high - low;
  const gap = Number.isFinite(room) ? Math.min(TRACK, room / ordered.length) : TRACK;
  let first = place - ((ordered.length - 1) * gap) / 2;
  first = Math.max(first, low + gap / 2);
  first = Math.min(first, high - gap / 2 - (ordered.length - 1) * gap);
  ordered.forEach((unit, index) => {
    const value = half(first + index * gap);
    for (const run of unit) {
      const points = routed.get(run.line);
      const [a, b, before, after] = [points?.[run.index], points?.[run.index + 1], points?.[run.index - 1], points?.[run.index + 2]];
      if (!points || !a || !b || !before || !after) {
        continue;
      }
      // Keep the runs out of and into the ends going the way they go.
      const keeps = (end: Point, beyond: Point, last: boolean) => !last || (across ? Math.sign(value - beyond.y) === Math.sign(end.y - beyond.y) && Math.abs(value - beyond.y) >= MARGIN : Math.sign(value - beyond.x) === Math.sign(end.x - beyond.x) && Math.abs(value - beyond.x) >= MARGIN);
      if (!keeps(a, before, run.index === 1) || !keeps(b, after, run.index + 2 === points.length - 1)) {
        continue;
      }
      points[run.index] = across ? { x: a.x, y: value } : { x: value, y: a.y };
      points[run.index + 1] = across ? { x: b.x, y: value } : { x: value, y: b.y };
    }
  });
}

/** Routes every line (`RouteInput`), by id. A line whose ends aren't both drawn, or that starts and ends at one box, has none. */
export function routeLines(input: RouteInput): Map<string, RoutedLine> {
  const { boxes } = input;
  const frameIds = [...input.frames.keys()].filter((id) => boxes.has(id));
  const inside = new Map(frameIds.map((id) => [id, new Set(input.frames.get(id) ?? [])]));
  const header = (item: string) => input.headers?.get(item) ?? HEADER;
  const lines = input.lines.filter((line) => line.from.item !== line.to.item && boxes.has(line.from.item) && boxes.has(line.to.item));
  const rectOf = (item: string) => boxes.get(item) ?? EMPTY;
  const center = (item: string): Point => {
    const rect = rectOf(item);
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  };

  // What lines go around: every box but expanded frames, CLEAR out; each frame is a layer of its own.
  const solidRects = [...boxes].filter(([id]) => !inside.has(id)).map(([, rect]) => rect);
  const solid = solidRects.map((rect) => grown(rect, CLEAR));
  const frameRects = frameIds.map((id) => grown(rectOf(id), CLEAR / 2));
  const layers = [solid, ...frameRects.map((rect) => [rect])];
  /** The layers a line goes around: the boxes, and each frame that holds neither end. */
  const layersOf = (line: LineRequest): number[] => [
    0,
    ...frameIds.flatMap((id, index) => {
      const held = inside.get(id);
      return held?.has(line.from.item) === true || held?.has(line.to.item) === true || id === line.from.item || id === line.to.item ? [] : [index + 1];
    }),
  ];
  const place = (end: LineEnd, side: Side, at: number | undefined, along: number): PlacedEnd => {
    const point = sidePoint(rectOf(end.item), side, at ?? end.at ?? header(end.item), along);
    const [dx, dy] = OUT[side];
    return { side, point, stub: { x: point.x + dx * STUB, y: point.y + dy * STUB } };
  };
  const coordinates = (ends: readonly PlacedEnd[]) => {
    const xs = [...solid, ...frameRects].flatMap((rect) => [rect.x, rect.x + rect.width]);
    const ys = [...solid, ...frameRects].flatMap((rect) => [rect.y, rect.y + rect.height]);
    return { xs: [...xs, ...ends.map((end) => end.stub.x)], ys: [...ys, ...ends.map((end) => end.stub.y)] };
  };
  // Short lines first.
  const order = [...lines].sort((a, b) => {
    const [fa, ta, fb, tb] = [center(a.from.item), center(a.to.item), center(b.from.item), center(b.to.item)];
    return Math.abs(fa.x - ta.x) + Math.abs(fa.y - ta.y) - (Math.abs(fb.x - tb.x) + Math.abs(fb.y - tb.y)) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  });

  // 1. Sides: where an end may use several, the pair whose line, routed alone, bends least, then is shortest.
  const choices = new Map(lines.map((line) => [line.id, { from: line.from.sides.map((side) => place(line.from, side, undefined, 0)), to: line.to.sides.map((side) => place(line.to, side, undefined, 0)) }]));
  const sides = new Map<string, { readonly from: Side; readonly to: Side }>();
  const open = lines.filter((line) => line.from.sides.length > 1 || line.to.sides.length > 1);
  if (open.length > 0) {
    const { xs, ys } = coordinates([...choices.values()].flatMap(({ from, to }) => [...from, ...to]));
    const grid = new Grid(xs, ys, layers);
    for (const line of open) {
      const { from, to } = choices.get(line.id) ?? { from: [], to: [] };
      let chosen: { from: Side; to: Side; bends: number; length: number } | null = null;
      for (const a of from) {
        for (const b of to) {
          const { points, cost } = search(grid, a, b, layersOf(line), []);
          const score = { from: a.side, to: b.side, bends: cost === Infinity ? Infinity : points.length - 2, length: lengthOf(points) };
          if (!chosen || score.bends < chosen.bends || (score.bends === chosen.bends && score.length < chosen.length - 0.5)) {
            chosen = score;
          }
        }
      }
      if (chosen) {
        sides.set(line.id, { from: chosen.from, to: chosen.to });
      }
    }
  }
  const sideOf = (line: LineRequest, which: "from" | "to"): Side => sides.get(line.id)?.[which] ?? line[which].sides[0] ?? (which === "from" ? "right" : "left");

  // 2. Ends: fixed ones where they say; the rest on one side of a box spread along it in the order
  // their lines come from; merging lines into one box side share the first one's end.
  interface Slot {
    readonly line: LineRequest;
    readonly which: "from" | "to";
  }
  const free = new Map<string, Slot[]>();
  for (const line of lines) {
    for (const which of ["from", "to"] as const) {
      const end = line[which];
      if (end.at === undefined && !(line.merge && which === "to")) {
        const key = `${end.item}\u0000${sideOf(line, which)}`;
        free.set(key, [...(free.get(key) ?? []), { line, which }]);
      }
    }
  }
  const spread = new Map<string, number>();
  for (const [key, slots] of free) {
    const side = key.slice(key.indexOf("\u0000") + 1) as Side;
    const upright = side === "left" || side === "right";
    const other = (slot: Slot) => center(slot.line[slot.which === "from" ? "to" : "from"].item);
    const sorted = [...slots].sort((a, b) => (upright ? other(a).y - other(b).y : other(a).x - other(b).x) || (a.line.id < b.line.id ? -1 : a.line.id > b.line.id ? 1 : 0));
    sorted.forEach((slot, index) => {
      const item = slot.line[slot.which].item;
      spread.set(`${slot.line.id}\u0000${slot.which}`, upright ? Math.min(rectOf(item).height - 6, header(item) + index * SPREAD) : (index - (sorted.length - 1) / 2) * SPREAD);
    });
  }
  const ends = new Map<string, { from: PlacedEnd; to: PlacedEnd }>();
  for (const line of lines) {
    const at = (which: "from" | "to"): PlacedEnd => {
      const side = sideOf(line, which);
      const value = spread.get(`${line.id}\u0000${which}`);
      return side === "left" || side === "right" ? place(line[which], side, value, 0) : place(line[which], side, undefined, value ?? 0);
    };
    let from = at("from");
    let to = at("to");
    // Ends within SNAP of level, facing each other: the one that may move does, and the line runs straight.
    const facing = (from.side === "right" && to.side === "left" && to.point.x > from.point.x) || (from.side === "left" && to.side === "right" && to.point.x < from.point.x);
    const off = to.point.y - from.point.y;
    if (facing && off !== 0 && Math.abs(off) <= SNAP) {
      if (!line.merge && line.to.at === undefined) {
        to = { ...to, point: { x: to.point.x, y: from.point.y }, stub: { x: to.stub.x, y: from.point.y } };
      } else if (line.from.at === undefined) {
        from = { ...from, point: { x: from.point.x, y: to.point.y }, stub: { x: from.stub.x, y: to.point.y } };
      }
    }
    ends.set(line.id, { from, to });
  }
  const trunks = new Map<string, PlacedEnd>();
  for (const line of order) {
    const end = ends.get(line.id);
    if (end && line.merge) {
      const key = `${line.to.item}\u0000${end.to.side}\u0000${line.to.at ?? ""}`;
      const trunk = trunks.get(key) ?? end.to;
      trunks.set(key, trunk);
      ends.set(line.id, { from: end.from, to: trunk });
    }
  }
  // A line's bundles: the lines leaving its start's spot, and (merging) those into its end's.
  const bundles = new Map<string, string[]>();
  for (const line of lines) {
    const end = ends.get(line.id);
    if (end) {
      const out = `out\u0000${line.from.item}\u0000${end.from.point.x}\u0000${end.from.point.y}`;
      bundles.set(line.id, line.merge ? [out, `in\u0000${line.to.item}\u0000${end.to.point.x}\u0000${end.to.point.y}`] : [out]);
    }
  }

  // 3. Routes: short lines first, each paying to cross the ones before it, and drawn to share its bundles' runs.
  const { xs, ys } = coordinates([...ends.values()].flatMap(({ from, to }) => [from, to]));
  const grid = new Grid(xs, ys, layers);
  const routed = new Map<string, Point[]>();
  for (const line of order) {
    const end = ends.get(line.id);
    const own = bundles.get(line.id) ?? [];
    if (!end) {
      continue;
    }
    const { points } = search(grid, end.from, end.to, layersOf(line), own);
    routed.set(line.id, points);
    for (const bundle of own) {
      grid.mark(points, bundle);
    }
  }

  // 4. Tracks.
  nudge(routed, bundles, solidRects);

  const result = new Map<string, RoutedLine>();
  for (const line of lines) {
    const points = routed.get(line.id);
    const end = ends.get(line.id);
    if (points && end) {
      result.set(line.id, { points: simplify(points), from: end.from.side, to: end.to.side });
    }
  }
  return result;
}
