import { describe, expect, it } from "vitest";
import { FOLLOW_GAP, HEADER_PORT, STACK_GAP, arrange, callerPlacement, clearSpot, fallbackSize, followPlacement, reflow, startCodePlacement, type Point } from "./layout";

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

describe("unmeasured cards", () => {
  it("are as big as a compact card's header, or an expanded card's usual size", () => {
    expect(fallbackSize()).toEqual({ width: 280, height: 44 });
    expect(fallbackSize("expanded")).toEqual({ width: 520, height: 320 });
  });
});

describe("following", () => {
  it("places the target 80 px right of the source with its header level with the calling line", () => {
    const source = { x: 40, y: 100, width: 600, height: 500 };
    expect(followPlacement(source, 250)).toEqual([40 + 600 + FOLLOW_GAP, 100 + 250 - HEADER_PORT]);
  });
});

describe("placing a card for an agent", () => {
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

describe("tracing a start", () => {
  it("puts its code in a column 80 px right of the start card, in order, stacked from its top", () => {
    const start = { x: 0, y: 0, width: 236, height: 90 };
    expect(startCodePlacement(start, [{ width: 300, height: 200 }, { width: 500, height: 100 }])).toEqual([
      [236 + FOLLOW_GAP, 0],
      [236 + FOLLOW_GAP, 200 + STACK_GAP],
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

  it("puts each card in the column after its deepest caller, level with the middle of its calling lines when there is room", () => {
    const positions = arrange(cards, calls);
    const x = (id: string) => positions.get(id)?.x ?? Number.NaN;
    const y = (id: string) => positions.get(id)?.y ?? Number.NaN;
    expect(x("entrance")).toBeLessThan(x("handler"));
    expect(x("handler")).toBeLessThan(x("callee"));
    expect(x("callee")).toBeLessThan(x("shared"));
    expect(y("handler")).toBe(120 - HEADER_PORT);
    expect(y("callee")).toBe(y("handler") + 80 - HEADER_PORT);
    // shared: halfway between its two lines: callee's (40 down it), and handler's, which crosses
    // callee's column in a lane of its own, clear of callee (under it: above, it would cross callee's line).
    const line = y("shared") + HEADER_PORT;
    expect(line).toBeGreaterThan(y("callee") + 40);
    expect(2 * line - (y("callee") + 40)).toBeGreaterThan(y("callee") + 300);
    expect(y("loose")).toBeGreaterThan(400);
  });

  it("keeps a long line straight in its own lane where nothing is in its way", () => {
    const chain = [
      { id: "root", width: 300, height: 300 },
      { id: "a", width: 200, height: 44 },
      { id: "b", width: 200, height: 44 },
      { id: "c", width: 200, height: 44 },
    ];
    const positions = arrange(chain, [
      { source: "root", target: "a", port: 40, order: 0 },
      { source: "a", target: "b", port: HEADER_PORT, order: 0 },
      { source: "b", target: "c", port: HEADER_PORT, order: 0 },
      { source: "root", target: "c", port: 200, order: 1 },
    ]);
    const y = (id: string) => positions.get(id)?.y ?? Number.NaN;
    expect([y("a"), y("b")]).toEqual([40 - HEADER_PORT, 40 - HEADER_PORT]);
    // c: halfway between b's line (40) and root's, straight at 200 past a and b.
    expect(y("c")).toBe(Math.round((40 + 200) / 2 - HEADER_PORT));
  });

  it("puts a card two roots reach at the middle of their lines, and a root with its own tree level with it", () => {
    const small = (id: string) => ({ id, width: 200, height: 44 });
    const shared = arrange(
      [small("r1"), small("r2"), small("shared")],
      [
        { source: "r1", target: "shared", port: HEADER_PORT, order: 0 },
        { source: "r2", target: "shared", port: HEADER_PORT, order: 0 },
      ],
    );
    const y = (positions: Map<string, Point>, id: string) => positions.get(id)?.y ?? Number.NaN;
    // One band for both roots: shared sits between their lines, not in the first one's band.
    expect(y(shared, "shared")).toBeGreaterThan(y(shared, "r1"));
    expect(y(shared, "shared")).toBeLessThan(y(shared, "r2"));
    // r2's own callee is pushed under r1's tall one; r2 moves level with it.
    const apart = arrange(
      [small("r1"), small("r2"), { id: "tall", width: 200, height: 400 }, small("own")],
      [
        { source: "r1", target: "tall", port: HEADER_PORT, order: 0 },
        { source: "r2", target: "own", port: HEADER_PORT, order: 0 },
      ],
    );
    expect(y(apart, "own")).toBeGreaterThan(y(apart, "tall") + 400);
    expect(y(apart, "r2")).toBe(y(apart, "own"));
  });

  it("orders each column so lines don't cross: a lane goes under the card called above it", () => {
    const chain = [
      { id: "root", width: 300, height: 60 },
      { id: "a", width: 200, height: 44 },
      { id: "b", width: 200, height: 44 },
      { id: "c", width: 200, height: 44 },
    ];
    const positions = arrange(chain, [
      { source: "root", target: "c", port: 50, order: 1 },
      { source: "root", target: "a", port: 30, order: 0 },
      { source: "a", target: "b", port: HEADER_PORT, order: 0 },
      { source: "b", target: "c", port: HEADER_PORT, order: 0 },
    ]);
    // a stays level with its call (30), and root's line to c (50) runs under it, not across it.
    expect(positions.get("a")?.y).toBe(30 - HEADER_PORT);
    expect(positions.get("b")?.y).toBe(30 - HEADER_PORT);
    expect(positions.get("c")?.y).toBeGreaterThan(positions.get("b")?.y ?? 0);
  });

  it("puts cards without calls in a compact grid underneath, a file's cards together", () => {
    const loose = ["a1", "b1", "a2", "b2"].map((id) => ({ id, width: 200, height: 50, group: `src/${id[0] ?? ""}.ts` }));
    const positions = arrange(loose, []);
    const at = (id: string) => positions.get(id);
    expect([at("a1"), at("a2"), at("b1"), at("b2")]).toEqual([
      { x: 0, y: 0 },
      { x: 200 + STACK_GAP, y: 0 },
      { x: 0, y: 50 + STACK_GAP },
      { x: 200 + STACK_GAP, y: 50 + STACK_GAP },
    ]);
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
