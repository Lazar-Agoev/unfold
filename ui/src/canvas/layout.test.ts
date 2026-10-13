import { describe, expect, it } from "vitest";
import type { CardView, CodeText } from "../ipc/generated";
import { shortPath } from "../code/text";
import { fixture } from "../test/engine";
import { CODE_FONT, estimateText } from "./measure";
import {
  CARD_MAX_WIDTH,
  CARD_MIN_WIDTH,
  ENTRANCE_MAX_WIDTH,
  ENTRANCE_MIN_WIDTH,
  FOLLOW_GAP,
  HEADER_PORT,
  STACK_GAP,
  arrange,
  callerPlacement,
  clearSpot,
  fallbackSize,
  followPlacement,
  handlerPlacement,
  reflow,
} from "./layout";
import {
  blockCardWidth,
  codeContentWidth,
  entranceCardWidth,
  entranceHandlerRows,
  estimatedBlockSize,
  estimatedLineOffset,
  gutterWidth,
  headerWidth,
} from "./sizing";

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
  it("keeps block cards between 280 and 880 px wide, and entrance cards between 200 and 320", () => {
    expect(blockCardWidth({ name: "f", location: "a.ts", body: 10 }, estimateText)).toBe(CARD_MIN_WIDTH);
    expect(blockCardWidth({ name: "f", location: "a.ts", body: 5000 }, estimateText)).toBe(CARD_MAX_WIDTH);
    const entrance = (handlers: string[]) => entranceCardWidth({ method: "POST", title: "/orders", location: "src/routes/orders.ts:22", handlers }, estimateText);
    expect(entrance([])).toBeGreaterThanOrEqual(ENTRANCE_MIN_WIDTH);
    expect(entrance(["x".repeat(200)])).toBe(ENTRANCE_MAX_WIDTH);
    expect(entranceCardWidth({ method: null, title: "a", location: "", handlers: [] }, estimateText)).toBe(ENTRANCE_MIN_WIDTH);
  });

  it("sizes a Code card to its longest line after a gutter of at least 40 px, and the chevron widens the header", () => {
    const width = codeContentWidth(code, estimateText);
    const widest = Math.max(...code.lines.map((line) => line.end - line.start));
    expect(width).toBeGreaterThan(widest * estimateText("0", CODE_FONT) + 40);
    expect(gutterWidth(2, estimateText)).toBe(40);
    expect(gutterWidth(5, estimateText)).toBeGreaterThan(40);
    const plain = headerWidth({ name: "createOrder", location: "orders/create.ts" }, estimateText);
    expect(headerWidth({ name: "createOrder", location: "orders/create.ts", chevron: true }, estimateText)).toBe(plain + 22);
  });
});

describe("estimates before a card is drawn", () => {
  it("sizes a block card from its model: its width, and a row per line under the header", () => {
    const size = estimatedBlockSize(createOrder, estimateText);
    expect(size.width).toBe(blockCardWidth({ name: createOrder.header.qualifiedName, location: shortPath(createOrder.header.path), body: codeContentWidth(code, estimateText) }, estimateText));
    expect(size.height).toBe(44 + 18 + code.lines.length * 20);
    expect(estimatedBlockSize(undefined, estimateText)).toEqual(fallbackSize(false));
  });

  it("finds an entrance card's handler rows below its header, location, and heading", () => {
    const text = { method: "POST", title: "/orders", location: "src/routes/orders.ts:22", handlers: ["requireUser", "createOrder"] };
    // Border, header (padding, the 18px method badge, rule), body padding, location, gap, heading, gap, half a row.
    expect(entranceHandlerRows(text, 240, estimateText)).toEqual([102, 122]);
    expect(entranceHandlerRows({ ...text, method: null }, 240, estimateText)).toEqual([100, 120]);
    expect(entranceHandlerRows({ ...text, location: "x".repeat(60) }, 240, estimateText)).toEqual([116, 136]);
  });
});

describe("placing a card for an agent", () => {
  it("finds a Code line's row before the card is drawn, and nothing for a line it doesn't show", () => {
    // createOrder's Code starts at line 25: the header, half the code's padding, then 20 px rows.
    expect(estimatedLineOffset(createOrder, 25)).toBe(44 + 9 + 10);
    expect(estimatedLineOffset(createOrder, 32)).toBe(44 + 9 + 7 * 20 + 10);
    expect(estimatedLineOffset(createOrder, 24)).toBeUndefined();
    expect(estimatedLineOffset(createOrder, 36)).toBeUndefined();
    expect(estimatedLineOffset(undefined, 25)).toBeUndefined();
  });

  it("moves a spot down until it keeps clear of every card", () => {
    const size = { width: 200, height: 100 };
    expect(clearSpot({ x: 0, y: 0 }, size, [])).toEqual([0, 0]);
    const occupied = [
      { x: 0, y: 0, width: 300, height: 100 },
      { x: 100, y: 120, width: 300, height: 200 },
      { x: 600, y: 0, width: 100, height: 1000 },
    ];
    // Below the first card, then below the second (which the first move ran into); the third is far right.
    expect(clearSpot({ x: 50, y: 10 }, size, occupied)).toEqual([50, 120 + 200 + STACK_GAP]);
    // Right beside a card, but within the gap: still moved.
    expect(clearSpot({ x: 300 + STACK_GAP - 1, y: 0 }, size, occupied.slice(0, 1))).toEqual([331, 100 + STACK_GAP]);
    expect(clearSpot({ x: 300 + STACK_GAP, y: 0 }, size, occupied.slice(0, 1))).toEqual([332, 0]);
  });
});

describe("Show callers", () => {
  it("puts each caller's right edge 80 px left of the card, stacked from its top", () => {
    const card = { x: 1000, y: 100, width: 400, height: 300 };
    expect(callerPlacement(card, [{ width: 300, height: 200 }, { width: 500, height: 100 }])).toEqual([
      [1000 - FOLLOW_GAP - 300, 100],
      [1000 - FOLLOW_GAP - 500, 100 + 200 + STACK_GAP],
    ]);
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
    const [first] = handlerPlacement(entrance, [180], [{ width: 520, height: 200 }]);
    expect(first).toEqual([400 + FOLLOW_GAP, 100 + 180 - HEADER_PORT]);
  });

  it("stacks the next handlers level with their rows, or below the one above, never overlapping", () => {
    const positions = handlerPlacement(entrance, [180, 204, 600], [{ width: 520, height: 200 }, { width: 520, height: 100 }, { width: 520, height: 50 }]);
    const firstY = 100 + 180 - HEADER_PORT;
    expect(positions[1]).toEqual([400 + FOLLOW_GAP, firstY + 200 + STACK_GAP]);
    expect(positions[2]).toEqual([400 + FOLLOW_GAP, 100 + 600 - HEADER_PORT]);
  });
});
