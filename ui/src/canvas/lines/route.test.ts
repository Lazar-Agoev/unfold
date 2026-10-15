import { describe, expect, it } from "vitest";
import type { Point, Rect } from "../layout";
import { routeLines, type LineRequest } from "./route";

const box = (x: number, y: number, width = 200, height = 44): Rect => ({ x, y, width, height });

/** Whether segment a–b passes through the inside of `rect`. */
function through(a: Point, b: Point, rect: Rect): boolean {
  const [x0, x1, y0, y1] = [Math.min(a.x, b.x), Math.max(a.x, b.x), Math.min(a.y, b.y), Math.max(a.y, b.y)];
  return x0 < rect.x + rect.width && x1 > rect.x && y0 < rect.y + rect.height && y1 > rect.y;
}

function under(points: readonly Point[], rect: Rect): boolean {
  return points.some((point, index) => index > 0 && through(points[index - 1] ?? point, point, rect));
}

const arrow = (id: string, from: string, to: string, at = 22): LineRequest => ({ id, from: { item: from, sides: ["right"], at }, to: { item: to, sides: ["left"] }, merge: true });

describe("routing lines", () => {
  it("runs a line straight between ends that are level", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["b", box(400, 0)],
    ]);
    const route = routeLines({ boxes, frames: new Map(), lines: [arrow("l", "a", "b")] }).get("l");
    expect(route?.points).toEqual([
      { x: 200, y: 22 },
      { x: 400, y: 22 },
    ]);
    expect([route?.from, route?.to]).toEqual(["right", "left"]);
  });

  it("goes around a box between its ends, with right-angle bends", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["wall", box(300, -100, 100, 300)],
      ["b", box(600, 0)],
    ]);
    const points = routeLines({ boxes, frames: new Map(), lines: [arrow("l", "a", "b")] }).get("l")?.points ?? [];
    expect(points[0]).toEqual({ x: 200, y: 22 });
    expect(points[points.length - 1]).toEqual({ x: 600, y: 22 });
    expect(under(points, box(300, -100, 100, 300))).toBe(false);
    for (let index = 1; index < points.length; index += 1) {
      const [p, q] = [points[index - 1], points[index]];
      expect(p?.x === q?.x || p?.y === q?.y).toBe(true);
    }
  });

  it("keeps a line out of a frame that holds neither end, but not out of one that holds an end", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["frame", box(280, -60, 240, 200)],
      ["inner", box(300, 20, 200, 44)],
      ["b", box(700, 0)],
    ]);
    const frames = new Map([["frame", ["inner"]]]);
    const routes = routeLines({ boxes, frames, lines: [arrow("past", "a", "b"), arrow("in", "a", "inner")] });
    expect(under(routes.get("past")?.points ?? [], box(280, -60, 240, 200))).toBe(false);
    expect(routes.get("in")?.points.at(-1)).toEqual({ x: 300, y: 42 });
  });

  it("merges arrows into one card into one end, and spreads connections along a side", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["c", box(0, 300)],
      ["t", box(500, 150)],
    ]);
    const routes = routeLines({ boxes, frames: new Map(), lines: [arrow("one", "a", "t"), arrow("two", "c", "t")] });
    expect(routes.get("one")?.points.at(-1)).toEqual({ x: 500, y: 172 });
    expect(routes.get("two")?.points.at(-1)).toEqual({ x: 500, y: 172 });
    const link = (id: string, from: string): LineRequest => ({ id, from: { item: from, sides: ["right"] }, to: { item: "t", sides: ["left"] }, merge: false });
    const spread = routeLines({ boxes, frames: new Map(), lines: [link("one", "a"), link("two", "c")] });
    // The line from above enters higher; 14 apart from the header.
    expect(spread.get("one")?.points.at(-1)).toEqual({ x: 500, y: 172 });
    expect(spread.get("two")?.points.at(-1)).toEqual({ x: 500, y: 186 });
  });

  it("picks the sides that bend least where an end may use several", () => {
    const boxes = new Map([
      ["code", box(0, 0)],
      ["table", box(600, 300)],
    ]);
    const route = routeLines({
      boxes,
      frames: new Map(),
      lines: [{ id: "l", from: { item: "code", sides: ["right"] }, to: { item: "table", sides: ["left", "top"] }, merge: false }],
    }).get("l");
    // Into the top it bends once; into the left side, twice.
    expect(route?.to).toBe("top");
    expect(route?.points).toHaveLength(3);
  });

  it("still routes a line orthogonally when a box sits right by its end", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["note", box(150, -50, 200, 66)],
      ["b", box(400, 100)],
    ]);
    const points = routeLines({ boxes, frames: new Map(), lines: [arrow("l", "a", "b")] }).get("l")?.points ?? [];
    expect(points.at(-1)).toEqual({ x: 400, y: 122 });
    for (let index = 1; index < points.length; index += 1) {
      const [p, q] = [points[index - 1], points[index]];
      expect(p?.x === q?.x || p?.y === q?.y).toBe(true);
    }
    expect(under(points, box(150, -50, 200, 66))).toBe(false);
  });

  it("gives lines that share a run their own tracks", () => {
    const boxes = new Map([
      ["a", box(0, 0)],
      ["b", box(0, 100)],
      ["x", box(500, 300)],
      ["y", box(500, 400)],
    ]);
    const routes = routeLines({ boxes, frames: new Map(), lines: [arrow("ax", "a", "x"), arrow("by", "b", "y")] });
    const downs = [...routes.values()].map(({ points }) => points.find((point, index) => index > 0 && point.x === points[index - 1]?.x && point.x > 200 && point.x < 500)?.x);
    expect(downs.every((x) => x !== undefined)).toBe(true);
    expect(Math.abs((downs[0] ?? 0) - (downs[1] ?? 0))).toBeGreaterThanOrEqual(4);
  });
});
