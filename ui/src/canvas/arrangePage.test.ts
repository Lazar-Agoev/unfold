import { describe, expect, it } from "vitest";
import type { Card, Frame, Link, Note, Resource } from "../ipc/generated";
import { arrangePage, type ArrangeInput } from "./arrangePage";
import type { Arrow } from "./arrows";
import { framePadding, pageGeometry, type FramedPage } from "./frames/geometry";
import { HEADER_PORT, arrange, type Point, type Rect } from "./layout";

const card = (id: string, at: [number, number], entrance = false): Card => (entrance ? { id, ref: { entrance: `entrance:${id}` }, at } : { id, ref: { block: `src/a.ts#function:${id}` }, at });
const frame = (id: string, members: string[], extra: Partial<Frame> = {}): Frame => ({ id, title: id, members, look: "box", author: "agent:claude", ...extra });

function call(source: string, target: string, port: string, order = 0): Arrow {
  return { id: `${source}|${port}|${target}`, source, sourceHandle: port, target, project: "p", occurrence: port, seq: 0, kind: "calls", dashed: false, order, line: null };
}

const SIZES: Readonly<Record<string, { width: number; height: number }>> = {
  route: { width: 240, height: 160 },
  create: { width: 520, height: 400 },
  place: { width: 480, height: 300 },
  insert: { width: 400, height: 200 },
  notify: { width: 360, height: 120 },
  loose: { width: 300, height: 100 },
};
const sizeOf = (id: string) => SIZES[id] ?? { width: 280, height: 90 };
/** Calling lines' ports, below the calling card's top. */
const PORTS: Readonly<Record<string, number>> = { "create|c1|place": 120, "create|c2|notify": 300, "place|c3|insert": 80, "route|r0|create": 102 };
const portOffset = (arrow: Arrow) => PORTS[arrow.id];

interface Setup {
  readonly cards: readonly Card[];
  readonly frames?: readonly Frame[];
  readonly resources?: readonly Resource[];
  readonly notes?: readonly Note[];
  readonly links?: readonly Link[];
  readonly arrows: readonly Arrow[];
}

function input({ cards, frames = [], resources = [], notes = [], links = [], arrows }: Setup, extra: Partial<ArrangeInput> = {}): ArrangeInput {
  const page = { cards, frames, resources, notes, links };
  return { page, rects: pageGeometry(page, sizeOf).rects, sizeOf, arrows, portOffset, ...extra };
}

/** Arrange as it was before frames (the reference for pages without frames). */
function todaysArrange(cards: readonly Card[], rects: ReadonlyMap<string, Rect>, arrows: readonly Arrow[]): Map<string, Point> {
  const ordered = [...cards].sort((a, b) => {
    const entrance = Number("block" in a.ref) - Number("block" in b.ref);
    const ra = rects.get(a.id);
    const rb = rects.get(b.id);
    return entrance || (ra?.x ?? 0) - (rb?.x ?? 0) || (ra?.y ?? 0) - (rb?.y ?? 0) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  });
  const all = [...rects.values()];
  const origin = { x: Math.min(...all.map((rect) => rect.x)), y: Math.min(...all.map((rect) => rect.y)) };
  return arrange(
    ordered.map((item) => ({ id: item.id, width: rects.get(item.id)?.width ?? 520, height: rects.get(item.id)?.height ?? 320 })),
    arrows.map((arrow) => ({ source: arrow.source, target: arrow.target, port: portOffset(arrow) ?? HEADER_PORT, order: arrow.order })),
    origin,
  );
}

const CARDS = [card("create", [900, 300]), card("notify", [40, 1200]), card("route", [500, 500], true), card("place", [2000, 80]), card("insert", [0, 0]), card("loose", [700, 900])];
const ARROWS = [call("route", "create", "r0"), call("create", "place", "c1", 0), call("create", "notify", "c2", 1), call("place", "insert", "c3")];

/** The rect each card ends up in. */
function placed(positions: ReadonlyMap<string, Point>, id: string): Rect {
  const at = positions.get(id) ?? { x: Number.NaN, y: Number.NaN };
  return { ...at, ...sizeOf(id) };
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

describe("Arrange without frames", () => {
  it("gives exactly what Arrange gave before frames", () => {
    const setup = { cards: CARDS, arrows: ARROWS, links: [{ id: "drawn", from: "loose", to: "route" }] };
    const expected = todaysArrange(CARDS, pageGeometry({ cards: CARDS }, sizeOf).rects, ARROWS);
    expect([...arrangePage(input(setup))].sort()).toEqual([...expected].sort());
  });
});

describe("Arrange with frames", () => {
  const server = frame("server", ["route", "create", "place"]);
  const setup: Setup = { cards: CARDS, frames: [server], arrows: ARROWS };

  it("lays a frame out inside by call depth, then the page with the frame as one node", () => {
    const positions = arrangePage(input(setup));
    const route = placed(positions, "route");
    const create = placed(positions, "create");
    const place = placed(positions, "place");
    // Inside: route, then create, then place, left to right, create level with route's calling line.
    expect(route.x).toBeLessThan(create.x);
    expect(create.x).toBeLessThan(place.x);
    expect(create.y).toBe(route.y + 102 - HEADER_PORT);
    // Outside: insert and notify are called from inside the frame, so they sit right of its box.
    const box = pageGeometry({ cards: CARDS.map((item) => ({ ...item, at: [placed(positions, item.id).x, placed(positions, item.id).y] as [number, number] })), frames: [server] }, sizeOf).rects.get("server");
    expect(placed(positions, "insert").x).toBeGreaterThan((box?.x ?? 0) + (box?.width ?? 0));
    expect(placed(positions, "notify").x).toBeGreaterThan((box?.x ?? 0) + (box?.width ?? 0));
    // insert is level with place's calling line, across the frame's border.
    expect(placed(positions, "insert").y).toBe(place.y + 80 - HEADER_PORT);
    for (const id of ["insert", "notify", "loose"]) {
      expect(box && overlaps(box, placed(positions, id))).toBe(false);
    }
  });

  it("is deterministic, and arranging again changes nothing", () => {
    const first = arrangePage(input(setup));
    expect([...arrangePage(input(setup))]).toEqual([...first]);
    const moved = CARDS.map((item) => ({ ...item, at: [first.get(item.id)?.x ?? 0, first.get(item.id)?.y ?? 0] as [number, number] }));
    expect([...arrangePage(input({ ...setup, cards: moved }))].sort()).toEqual([...first].sort());
  });

  it("lays a frame out downward when its direction says so", () => {
    const positions = arrangePage(input({ ...setup, frames: [{ ...server, direction: "down" }] }));
    const route = placed(positions, "route");
    const create = placed(positions, "create");
    const place = placed(positions, "place");
    expect([route.x, create.x, place.x]).toEqual([route.x, route.x, route.x]);
    expect(create.y).toBeGreaterThan(route.y + route.height);
    expect(place.y).toBeGreaterThan(create.y + create.height);
  });

  it("arranges nested frames from the inside out, and keeps their boxes apart", () => {
    const frames = [frame("zone", ["server", "workers"], { look: "zone" }), server, frame("workers", ["insert", "notify"])];
    const positions = arrangePage(input({ ...setup, frames }));
    const page: FramedPage = { cards: CARDS.map((item) => ({ ...item, at: [placed(positions, item.id).x, placed(positions, item.id).y] as [number, number] })), frames };
    const { rects } = pageGeometry(page, sizeOf);
    const [outer, a, b] = ["zone", "server", "workers"].map((id) => rects.get(id));
    expect(a && b && overlaps(a, b)).toBe(false);
    // The zone holds both boxes with its padding around them.
    const zone = framePadding({ look: "zone" });
    expect(outer?.x).toBe(Math.min(a?.x ?? 0, b?.x ?? 0) - zone.left);
    // No calls join the workers: they sit in one row inside their box.
    const insert = placed(positions, "insert");
    const notify = placed(positions, "notify");
    expect(insert.y).toBe(notify.y);
    expect(overlaps(insert, notify)).toBe(false);
  });

  it("treats a collapsed frame as one node of its own size, with what it holds laid out inside it", () => {
    const collapsed = { ...server, collapsed: true };
    const positions = arrangePage(input({ ...setup, frames: [collapsed] }));
    const page: FramedPage = { cards: CARDS.map((item) => ({ ...item, at: [placed(positions, item.id).x, placed(positions, item.id).y] as [number, number] })), frames: [collapsed] };
    const node = pageGeometry(page, sizeOf).rects.get("server");
    // The node's corner is where the box's would be: its members sit inside the padding.
    const padding = framePadding(collapsed);
    expect(Math.min(...["route", "create", "place"].map((id) => placed(positions, id).x))).toBe((node?.x ?? 0) + padding.left);
    // insert and notify are called from inside it, so they go in the column after the node.
    expect(placed(positions, "insert").x).toBe(placed(positions, "notify").x);
    expect(placed(positions, "insert").x).toBeGreaterThan((node?.x ?? 0) + (node?.width ?? 0));
  });

  it("arranges only one frame when asked, keeping its box's corner", () => {
    const before = pageGeometry({ cards: CARDS, frames: [server] }, sizeOf).rects.get("server");
    const positions = arrangePage(input(setup, { frame: "server" }));
    expect([...positions.keys()].sort()).toEqual(["create", "place", "route"]);
    const padding = framePadding(server);
    expect(Math.min(...[...positions.values()].map((at) => at.x))).toBe((before?.x ?? 0) + padding.left);
    expect(Math.min(...[...positions.values()].map((at) => at.y))).toBe((before?.y ?? 0) + padding.top);
  });

  it("orders connections as data flows: after the code that writes, before the code that reads; drawn links don't count", () => {
    const resources: Resource[] = [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 0], author: "agent:claude" }];
    const links: Link[] = [
      { id: "w", from: "insert", to: "orders", kind: "writes", author: "agent:claude" },
      { id: "r", from: "loose", to: "orders", kind: "reads", author: "agent:claude" },
      { id: "drawn", from: "notify", to: "loose" },
    ];
    const positions = arrangePage(input({ cards: CARDS, resources, links, arrows: ARROWS }));
    expect(placed(positions, "orders").x).toBeGreaterThan(placed(positions, "insert").x);
    expect(placed(positions, "loose").x).toBeGreaterThan(placed(positions, "orders").x);
  });

  it("puts the notes attached to a card, resource card, or frame under it, the item taking room for them; others stay", () => {
    const resources: Resource[] = [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 0], author: "agent:claude" }];
    const note = (id: string, attach?: string): Note => ({ id, text: id, at: [5, 5], author: "agent:claude", ...(attach !== undefined ? { attach } : {}) });
    const notes = [note("b-on-create", "create"), note("a-on-create", "create"), note("on-orders", "orders"), note("on-server", "server"), note("on-link", "w"), note("free")];
    const links: Link[] = [{ id: "w", from: "insert", to: "orders", kind: "writes", author: "agent:claude" }];
    const frames = [frame("server", ["route"])];
    const setup = { cards: CARDS, frames, resources, notes, links, arrows: ARROWS };
    const positions = arrangePage(input(setup));
    const create = placed(positions, "create");
    // Under the card, in id order, 12px apart (notes are 280 × 90 here).
    expect(placed(positions, "a-on-create")).toMatchObject({ x: create.x, y: create.y + 400 + 12 });
    expect(placed(positions, "b-on-create")).toMatchObject({ x: create.x, y: create.y + 400 + 12 + 90 + 12 });
    expect(placed(positions, "on-orders").y).toBe(placed(positions, "orders").y + 90 + 12);
    // A frame's note goes under its box.
    const route = placed(positions, "route");
    const padding = framePadding(frame("server", []));
    expect(placed(positions, "on-server")).toMatchObject({ x: route.x - padding.left, y: route.y - padding.top + padding.top + 160 + padding.bottom + 12 });
    expect(positions.has("on-link")).toBe(false);
    expect(positions.has("free")).toBe(false);
    // Nothing the column holds under createOrder overlaps its notes.
    const below = [...positions].filter(([id, at]) => SIZES[id] !== undefined && id !== "create" && at.x === create.x && at.y > create.y);
    for (const [, at] of below) {
      expect(at.y).toBeGreaterThanOrEqual(create.y + 400 + 2 * (12 + 90));
    }
    expect(arrangePage(input(setup))).toEqual(positions);
  });
});
