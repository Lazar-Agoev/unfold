import { describe, expect, it } from "vitest";
import { BELOW_GAP, placeContract } from "./placeContract";
import { bezierCurve, pointAt, type Rect } from "./placeLabels";

const SIZE = { width: 316, height: 160 };
/** A straight line from (0, 0) to (400, 0): its point at t is (400t, 0). */
const CURVE = bezierCurve({ x: 0, y: 0 }, "right", { x: 400, y: 0 }, "left");

function box(top: { x: number; y: number }): Rect {
  return { x: top.x - SIZE.width / 2, y: top.y, ...SIZE };
}

function meets(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

describe("placeContract", () => {
  it("centers the contract on the label's point when it covers none of the rows", () => {
    expect(placeContract(SIZE, CURVE, [])).toEqual({ x: 200, y: -80 });
    expect(placeContract(SIZE, CURVE, [{ x: -500, y: 300, width: 200, height: 20 }])).toEqual({ x: 200, y: -80 });
  });

  it("moves along the line, off the caller's calling line", () => {
    // The caller's calling line: a row 60 above the line, reaching 100 along it.
    const callerLine = { x: -400, y: -60, width: 500, height: 20 };
    const top = placeContract(SIZE, CURVE, [callerLine]);
    expect(top.y).toBe(-80);
    expect(top.x).toBeGreaterThan(200);
    expect(meets(box(top), callerLine)).toBe(false);
  });

  it("hangs below the line when every spot along it covers a row", () => {
    // Rows just above the line, all along it: only below is clear.
    const rows = [{ x: -400, y: -40, width: 1200, height: 20 }];
    const top = placeContract(SIZE, CURVE, rows);
    expect(top).toEqual({ x: 200, y: BELOW_GAP });
    expect(meets(box(top), rows[0] as Rect)).toBe(false);
  });

  it("hangs below and to the side when the rows below the point are the callee's", () => {
    // The caller's line above, and the callee's marked rows below and right of the line's end.
    const rows = [{ x: -400, y: -40, width: 1200, height: 20 }, { x: 300, y: 60, width: 400, height: 200 }];
    const top = placeContract(SIZE, CURVE, rows);
    expect(top).toEqual({ x: 200 - SIZE.width / 2, y: BELOW_GAP });
    expect(rows.some((row) => meets(box(top), row))).toBe(false);
  });

  it("falls back to the label's point when nowhere is clear", () => {
    const rows = [{ x: -800, y: -40, width: 2000, height: 20 }, { x: -800, y: 100, width: 2000, height: 20 }];
    expect(placeContract(SIZE, CURVE, rows)).toEqual({ x: 200, y: -80 });
  });

  it("gives the same spot for the same size, curve, and rows, whatever their order", () => {
    const curve = bezierCurve({ x: 0, y: 0 }, "right", { x: 300, y: 260 }, "left");
    // A row under the label's point, and two marked rows in the callee.
    const rows = [{ x: 100, y: 120, width: 100, height: 20 }, { x: 310, y: 240, width: 400, height: 20 }, { x: 310, y: 300, width: 400, height: 20 }];
    const one = placeContract(SIZE, curve, rows);
    expect(placeContract(SIZE, curve, [...rows].reverse())).toEqual(one);
    expect(rows.some((row) => meets(box(one), row))).toBe(false);
    expect(one).not.toEqual({ x: pointAt(curve, 0.5).x, y: pointAt(curve, 0.5).y - SIZE.height / 2 });
  });
});
