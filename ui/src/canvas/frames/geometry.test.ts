import { describe, expect, it } from "vitest";
import type { Card, Frame, Note, Resource } from "../../ipc/generated";
import { STACK_GAP, fallbackSize } from "../layout";
import { estimateText } from "../measure";
import { frameNodeSize } from "../sizing";
import { NOTE_GAP, attachPoint, collapsedPlaces, fallbackItemSize, framePadding, pageGeometry, type FramedPage } from "./geometry";

const card = (id: string, at: [number, number]): Card => ({ id, ref: { block: `src/a.ts#function:${id}` }, at });
const frame = (id: string, members: string[], extra: Partial<Frame> = {}): Frame => ({ id, title: id, members, look: "box", author: "agent:claude", ...extra });

const SIZE = { width: 200, height: 100 };
const NODE = { width: 260, height: 80 };
/** Cards are 200 × 100, collapsed frames' nodes 260 × 80. */
const sizeOf = (page: FramedPage) => (id: string) => (page.cards.some((item) => item.id === id) ? SIZE : NODE);

const BOX = framePadding({ look: "box" });
const ZONE = framePadding({ look: "zone" });

describe("frame boxes", () => {
  it("leave room for a box's title bar and a zone's title band, more with a description", () => {
    expect(BOX).toEqual({ top: 44, right: 16, bottom: 16, left: 16 });
    expect(ZONE).toEqual({ top: 56, right: 24, bottom: 24, left: 24 });
    expect(framePadding({ look: "zone", description: { text: "Everything.", author: "agent:claude", signature: "" } }).top).toBe(76);
  });

  it("are their members' bounds plus padding, nested from the inside out, outermost first", () => {
    const page: FramedPage = {
      cards: [card("a", [0, 0]), card("b", [400, 0]), card("c", [1000, 0])],
      frames: [frame("zone", ["server", "c"], { look: "zone" }), frame("server", ["a", "b"])],
    };
    const { rects, frames } = pageGeometry(page, sizeOf(page));
    const server = { x: -BOX.left, y: -BOX.top, width: 600 + BOX.left + BOX.right, height: 100 + BOX.top + BOX.bottom };
    expect(rects.get("server")).toEqual(server);
    expect(rects.get("zone")).toEqual({ x: server.x - ZONE.left, y: server.y - ZONE.top, width: 1200 - server.x + ZONE.left + ZONE.right, height: server.height + ZONE.top + ZONE.bottom });
    expect(frames.map((item) => [item.frame.id, item.depth, item.collapsed])).toEqual([
      ["zone", 0, false],
      ["server", 1, false],
    ]);
  });

  it("aren't drawn for a frame with nothing drawn in it", () => {
    const page: FramedPage = { cards: [card("a", [0, 0])], frames: [frame("notes-only", ["n"])] };
    expect(pageGeometry(page, sizeOf(page)).frames).toEqual([]);
  });
});

describe("collapsed frames", () => {
  const page: FramedPage = {
    cards: [card("a", [100, 100]), card("b", [500, 100]), card("c", [100, 60])],
    frames: [frame("outer", ["inner"], { collapsed: true }), frame("inner", ["a", "b"], { collapsed: true })],
  };

  it("hide what they hold, drawn as the outermost collapsed frame", () => {
    expect([...collapsedPlaces(page)]).toEqual([
      ["a", "outer"],
      ["b", "outer"],
      ["inner", "outer"],
    ]);
    const { rects, frames, shownAs } = pageGeometry(page, sizeOf(page));
    expect(shownAs.get("a")).toBe("outer");
    expect(rects.has("a")).toBe(false);
    expect(rects.has("inner")).toBe(false);
    expect(frames.map((item) => item.frame.id)).toEqual(["outer"]);
  });

  it("are one node where the top-left of their box would be, and reflow like a card", () => {
    const { rects } = pageGeometry(page, sizeOf(page));
    // The inner frame would be a node at its box's corner; the outer box's corner is around that.
    const inner = { x: 100 - BOX.left, y: 100 - BOX.top };
    expect(rects.get("outer")).toEqual({ x: inner.x - BOX.left, y: inner.y - BOX.top, ...NODE });
    // Card c, under the node, is pushed below it.
    expect(rects.get("c")?.y).toBe(inner.y - BOX.top + NODE.height + STACK_GAP);
  });
});

describe("dragging", () => {
  const page: FramedPage = { cards: [card("a", [0, 0]), card("b", [400, 0]), card("c", [0, 600])], frames: [frame("server", ["a", "b"])] };

  it("moves a frame's box and everything in it with the pointer", () => {
    const at = pageGeometry(page, sizeOf(page)).rects.get("server");
    const { rects, inside } = pageGeometry(page, sizeOf(page), { id: "server", x: (at?.x ?? 0) + 50, y: (at?.y ?? 0) + 20 });
    expect(rects.get("a")).toMatchObject({ x: 50, y: 20 });
    expect(rects.get("b")).toMatchObject({ x: 450, y: 20 });
    expect(rects.get("c")).toMatchObject({ x: 0, y: 600 });
    expect(rects.get("server")).toMatchObject({ x: (at?.x ?? 0) + 50, y: (at?.y ?? 0) + 20 });
    expect(inside.get("server")).toEqual(["a", "b"]);
  });

  it("moves a card alone, and its frame's box follows", () => {
    const { rects } = pageGeometry(page, sizeOf(page), { id: "b", x: 400, y: 300 });
    expect(rects.get("b")).toMatchObject({ x: 400, y: 300 });
    expect(rects.get("server")?.height).toBe(400 + BOX.top + BOX.bottom);
  });
});

describe("sizes before anything is measured", () => {
  it("come from the card, the collapsed frame's words, a resource card's words and columns, or a note's words", () => {
    const orders: Resource = {
      id: "orders",
      kind: "table",
      name: "orders",
      columns: [{ name: "id", type: "string", key: "primary" }, { name: "customerId", type: "string" }],
      anchors: [],
      at: [0, 0],
      author: "agent:claude",
    };
    const page: FramedPage = {
      defaultView: "expanded",
      cards: [{ id: "route", ref: { entrance: "entrance:shop/place" }, at: [0, 0], view: "compact" }, card("a", [0, 0])],
      frames: [frame("server", ["a"], { collapsed: true })],
      resources: [orders, { ...orders, id: "queue", kind: "queue", name: "orders" }],
      notes: [{ id: "n", text: "The client gets a job id, not the order.", at: [0, 0], author: "agent:claude" }],
    };
    // A card in its own view, else the page's.
    expect(fallbackItemSize(page, "route")).toEqual(fallbackSize("compact"));
    expect(fallbackItemSize(page, "a")).toEqual(fallbackSize("expanded"));
    const estimated = frameNodeSize(frame("server", ["a"]), estimateText);
    expect(estimated.width).toBeGreaterThanOrEqual(240);
    const table = fallbackItemSize(page, "orders");
    const queue = fallbackItemSize(page, "queue");
    expect(table.width).toBeGreaterThanOrEqual(240);
    // Two column rows (20px each) and the grid's padding over the queue's header alone.
    expect(table.height - queue.height).toBe(18 + 2 * 20);
    expect(fallbackItemSize(page, "n").width).toBeGreaterThanOrEqual(140);
  });
});

describe("notes", () => {
  const note = (id: string, at: [number, number], extra: Partial<Note> = {}): Note => ({ id, text: id, at, author: "agent:claude", ...extra });
  const NOTE = { width: 160, height: 40 };
  const sized = (page: FramedPage) => (id: string) => ((page.notes ?? []).some((item) => item.id === id) ? NOTE : sizeOf(page)(id));

  it("stand where they are put when free, and a frame holding one takes it in", () => {
    const page: FramedPage = { cards: [card("a", [0, 0])], notes: [note("free", [0, 300])], frames: [frame("server", ["a", "free"])] };
    const { rects } = pageGeometry(page, sized(page));
    expect(rects.get("free")).toEqual({ x: 0, y: 300, ...NOTE });
    expect(rects.get("server")).toEqual({ x: -BOX.left, y: -BOX.top, width: 200 + BOX.left + BOX.right, height: 340 + BOX.top + BOX.bottom });
  });

  it("move with the card, resource card, frame, or link they are attached to", () => {
    const resource: Resource = { id: "orders", kind: "table", name: "orders", anchors: [], at: [600, 0], author: "agent:claude" };
    const page: FramedPage = {
      cards: [card("a", [0, 0]), card("b", [400, 200])],
      resources: [resource],
      frames: [frame("server", ["b"])],
      links: [{ id: "l", from: "a", to: "b", kind: "calls", author: "agent:claude" }],
      notes: [note("on-card", [0, 120], { attach: "a" }), note("on-table", [10, -50], { attach: "orders" }), note("on-frame", [0, -100], { attach: "server" }), note("on-link", [0, 30], { attach: "l" })],
    };
    const sizes = (id: string) => (id === "orders" ? NODE : sized(page)(id));
    const { rects } = pageGeometry(page, sizes);
    expect(rects.get("on-card")).toMatchObject({ x: 0, y: 120 });
    expect(rects.get("on-table")).toMatchObject({ x: 610, y: -50 });
    // A frame's offset starts where what it holds starts: its cards' top-left.
    expect(rects.get("on-frame")).toMatchObject({ x: 400, y: 100 });
    // A link's starts at the midpoint of its ends' corners.
    expect(rects.get("on-link")).toMatchObject({ x: 200, y: 130 });
    // Dragging the card takes its note along.
    const dragged = pageGeometry(page, sizes, { id: "a", x: 50, y: 500 }).rects;
    expect(dragged.get("on-card")).toMatchObject({ x: 50, y: 620 });
    expect(attachPoint(page, dragged, "a")).toEqual({ x: 50, y: 500 });
  });

  it("on a frame start from its cards' and nested frames' points, as the file's rule says, not from boxes or notes", () => {
    const page: FramedPage = {
      cards: [card("a", [100, 100]), card("b", [400, 300])],
      frames: [frame("zone", ["box", "free"], { look: "zone" }), frame("box", ["a", "b"])],
      notes: [note("free", [0, 0]), note("over-a", [0, -60], { attach: "a" }), note("on-zone", [0, 600], { attach: "zone" })],
    };
    const { rects } = pageGeometry(page, sized(page));
    expect(attachPoint(page, rects, "box")).toEqual({ x: 100, y: 100 });
    expect(attachPoint(page, rects, "zone")).toEqual({ x: 100, y: 100 });
    expect(rects.get("on-zone")).toMatchObject({ x: 100, y: 700 });
  });

  it("sit under their item where their offset would put them over it (the item grew), without a new offset", () => {
    const resource: Resource = { id: "orders", kind: "table", name: "orders", anchors: [], at: [600, 0], author: "agent:claude" };
    const page: FramedPage = {
      cards: [card("a", [0, 0]), card("b", [400, 400])],
      resources: [resource],
      frames: [frame("server", ["b"])],
      notes: [
        note("on-card", [0, 60], { attach: "a" }),
        note("beside", [210, 0], { attach: "a" }),
        note("on-table", [0, 40], { attach: "orders" }),
        note("on-frame", [0, 50], { attach: "server" }),
      ],
    };
    const { rects } = pageGeometry(page, (id) => (id === "orders" ? NODE : sized(page)(id)));
    expect(rects.get("on-card")).toMatchObject({ x: 0, y: 100 + NOTE_GAP });
    expect(rects.get("beside")).toMatchObject({ x: 210, y: 0 });
    expect(rects.get("on-table")).toMatchObject({ x: 600, y: 80 + NOTE_GAP });
    expect(rects.get("on-frame")).toMatchObject({ x: 400, y: 400 + 100 + BOX.bottom + NOTE_GAP });
    expect(page.notes?.[0]?.at).toEqual([0, 60]);
    // While it is dragged, it follows the pointer.
    expect(pageGeometry(page, sized(page), { id: "on-card", x: 20, y: 30 }).rects.get("on-card")).toMatchObject({ x: 20, y: 30 });
  });

  it("on an item a frame holds are taken in by its box, and the frames around it; one on the frame itself isn't", () => {
    const page: FramedPage = {
      cards: [card("a", [0, 0])],
      frames: [frame("server", ["a"]), frame("zone", ["server"], { look: "zone" })],
      notes: [note("under", [0, 112], { attach: "a" }), note("on-server", [0, 200], { attach: "server" })],
    };
    const { rects } = pageGeometry(page, sized(page));
    expect(rects.get("under")).toEqual({ x: 0, y: 112, ...NOTE });
    expect(rects.get("server")).toEqual({ x: -BOX.left, y: -BOX.top, width: 200 + BOX.left + BOX.right, height: 152 + BOX.top + BOX.bottom });
    // The note on the frame sits inside the zone, which takes it in too.
    const zone = rects.get("zone");
    expect(rects.get("on-server")).toMatchObject({ x: 0, y: 200 });
    expect(zone?.y).toBe(-BOX.top - ZONE.top);
    expect((zone?.y ?? 0) + (zone?.height ?? 0)).toBe(240 + ZONE.bottom);
  });

  it("aren't drawn inside a collapsed frame, nor on an item it hides", () => {
    const page: FramedPage = {
      cards: [card("a", [0, 0])],
      frames: [frame("server", ["a", "inside"], { collapsed: true })],
      notes: [note("inside", [0, 300]), note("on-hidden", [0, 120], { attach: "a" })],
    };
    const { rects, shownAs } = pageGeometry(page, sized(page));
    expect(shownAs.get("inside")).toBe("server");
    expect(rects.has("inside")).toBe(false);
    expect(rects.has("on-hidden")).toBe(false);
  });
});
