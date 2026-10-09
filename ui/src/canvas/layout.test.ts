import { describe, expect, it } from "vitest";
import type { CardView, CodeText } from "../ipc/generated";
import { fixture } from "../test/engine";
import { estimateText } from "./measure";
import { CARD_MAX_WIDTH, CARD_MIN_WIDTH, FOLLOW_GAP, HEADER_PORT, STACK_GAP, arrange, followPlacement, reflow } from "./layout";
import { placeHandlers } from "./placement";
import { blockCardWidth, codeContentWidth, entranceCardWidth } from "./sizing";

const createOrder = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
const code = (createOrder.code?.source.state === "current" ? createOrder.code.source : null) as CodeText;

describe("reflow", () => {
  it("pushes a card below a grown card in its column, and never above its saved place", () => {
    const sizes: Record<string, { width: number; height: number }> = { a: { width: 500, height: 300 }, b: { width: 500, height: 200 } };
    const cards = [
      { id: "a", at: [0, 0] as const },
      { id: "b", at: [100, 200] as const },
      { id: "c", at: [900, 0] as const },
    ];
    const sizeOf = (id: string) => sizes[id] ?? { width: 400, height: 100 };
    const placed = reflow(cards, sizeOf);
    expect(placed.get("b")).toEqual({ x: 100, y: 332 });
    expect(placed.get("c")).toEqual({ x: 900, y: 0 });
    sizes["a"] = { width: 500, height: 100 };
    expect(reflow(cards, sizeOf).get("b")).toEqual({ x: 100, y: 200 });
  });
});

describe("following", () => {
  it("places the target 80 px right of the source with its header level with the calling line", () => {
    const source = { x: 40, y: 100, width: 600, height: 500 };
    expect(followPlacement(source, 250)).toEqual([40 + 600 + FOLLOW_GAP, 100 + 250 - HEADER_PORT]);
  });
});

describe("card sizing", () => {
  it("keeps every card between 360 and 880 px wide", () => {
    expect(blockCardWidth({ name: "f", location: "a.ts:1", body: 10 }, estimateText)).toBe(CARD_MIN_WIDTH);
    expect(blockCardWidth({ name: "f", location: "a.ts:1", body: 5000 }, estimateText)).toBe(CARD_MAX_WIDTH);
    expect(entranceCardWidth("x", ["y".repeat(400)], [], estimateText)).toBe(CARD_MAX_WIDTH);
    expect(entranceCardWidth("x", [], [], estimateText)).toBe(CARD_MIN_WIDTH);
  });

  it("sizes a Code card to its longest line, with room for a port's leader", () => {
    const plain = codeContentWidth(code, new Set(), estimateText);
    const widest = Math.max(...code.lines.map((line) => line.end - line.start));
    expect(plain).toBeGreaterThan(widest * estimateText("0", '12.5px "JetBrains Mono Variable"'));
    const withPort = codeContentWidth(code, new Set(code.lines.map((line) => line.number)), estimateText);
    expect(withPort).toBeGreaterThan(plain);
    const width = blockCardWidth({ name: "createOrder", location: "routes/orders.ts:25", body: plain }, estimateText);
    expect(width).toBeGreaterThanOrEqual(CARD_MIN_WIDTH);
    expect(width).toBeLessThanOrEqual(CARD_MAX_WIDTH);
  });
});

describe("Arrange by call depth", () => {
  const cards = [
    { id: "entrance", width: 360, height: 200 },
    { id: "handler", width: 520, height: 400 },
    { id: "callee", width: 500, height: 300 },
    { id: "shared", width: 400, height: 200 },
    { id: "loose", width: 400, height: 100 },
  ];
  const calls = [
    { source: "entrance", target: "handler", port: 120, order: 0 },
    { source: "handler", target: "callee", port: 80, order: 0 },
    { source: "handler", target: "shared", port: 200, order: 1 },
    { source: "callee", target: "shared", port: 40, order: 0 },
  ];

  it("puts each card in the column after its deepest caller, level with the calling line when there is room", () => {
    const positions = arrange(cards, calls);
    const x = (id: string) => positions.get(id)?.x ?? Number.NaN;
    expect(x("entrance")).toBeLessThan(x("handler"));
    expect(x("handler")).toBeLessThan(x("callee"));
    expect(x("callee")).toBeLessThan(x("shared"));
    expect(positions.get("handler")?.y).toBe(120 - HEADER_PORT);
    expect(positions.get("loose")?.y).toBeGreaterThan(400);
  });

  it("is deterministic and depends only on cards, sizes, and calls", () => {
    expect([...arrange(cards, calls)]).toEqual([...arrange(cards, calls)]);
    expect([...arrange(cards, [...calls].reverse())]).toEqual([...arrange(cards, calls)]);
  });

  it("survives cycles", () => {
    const cyclic = [...calls, { source: "shared", target: "handler", port: 10, order: 0 }];
    expect(arrange(cards, cyclic).size).toBe(cards.length);
  });
});

describe("placing an entrance's handlers", () => {
  const entrance = { x: 0, y: 100, width: 400, height: 300 };

  it("puts the first handler level with its row, in a column to the right", () => {
    const [first] = placeHandlers(entrance, [180], [{ width: 520, height: 200 }]);
    expect(first).toEqual([400 + FOLLOW_GAP, 100 + 180 - HEADER_PORT]);
  });

  it("stacks the next handlers level with their rows, or below the one above, never overlapping", () => {
    const positions = placeHandlers(entrance, [180, 204, 600], [{ width: 520, height: 200 }, { width: 520, height: 100 }, { width: 520, height: 50 }]);
    const firstY = 100 + 180 - HEADER_PORT;
    expect(positions[1]).toEqual([400 + FOLLOW_GAP, firstY + 200 + STACK_GAP]);
    expect(positions[2]).toEqual([400 + FOLLOW_GAP, 100 + 600 - HEADER_PORT]);
  });
});
