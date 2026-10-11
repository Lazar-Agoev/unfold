import { describe, expect, it } from "vitest";
import { bezierCurve, placeLabels, pointAt, type LabelSpec, type Placement, type Rect } from "./placeLabels";

const HEIGHT = 20;

/** A line from the right edge of one card at (x1, y1) into the left of another at (x2, y2). */
function line(id: string, x1: number, y1: number, x2: number, y2: number, widths: readonly number[] = [190, 110, 80]): LabelSpec {
  return { id, curve: bezierCurve({ x: x1, y: y1 }, "right", { x: x2, y: y2 }, "left"), widths, height: HEIGHT };
}

function box(label: LabelSpec, placement: Placement): Rect {
  const width = label.widths[placement.form] ?? 0;
  return { x: placement.x - width / 2, y: placement.y - HEIGHT / 2, width, height: HEIGHT };
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** Every placed label's box: none may meet a card or another label. */
function expectClear(labels: readonly LabelSpec[], cards: readonly Rect[], placements: Map<string, Placement | null>): void {
  const boxes = labels.flatMap((label) => {
    const placement = placements.get(label.id);
    return placement ? [box(label, placement)] : [];
  });
  for (const [index, one] of boxes.entries()) {
    for (const card of cards) {
      expect(overlaps(one, card)).toBe(false);
    }
    for (const other of boxes.slice(index + 1)) {
      expect(overlaps(one, other)).toBe(false);
    }
  }
}

describe("bezierCurve", () => {
  it("has React Flow's label point at its middle", () => {
    // getBezierPath's label point for (0, 0) → (400, 100), right to left.
    const curve = bezierCurve({ x: 0, y: 0 }, "right", { x: 400, y: 100 }, "left");
    expect(pointAt(curve, 0.5)).toEqual({ x: 200, y: 50 });
    expect(pointAt(curve, 0)).toEqual({ x: 0, y: 0 });
    expect(pointAt(curve, 1)).toEqual({ x: 400, y: 100 });
  });
});

describe("placeLabels", () => {
  it("puts a label at its curve's midpoint when the canvas is free there", () => {
    const label = line("a", 0, 0, 500, 200);
    expect(placeLabels([label], [])).toEqual(new Map([["a", { x: 250, y: 100, form: 0 }]]));
  });

  it("never covers a card in a short gap: a shorter form fits, else the label isn't drawn", () => {
    const caller = { x: -400, y: -100, width: 400, height: 300 };
    const callee = { x: 130, y: -100, width: 400, height: 300 };
    // 130px between the cards: the full form (190) can't fit, `(+2) → Hold` (110) can.
    const label = line("a", 0, 0, 130, 0);
    const placed = placeLabels([label], [caller, callee]);
    expect(placed.get("a")?.form).toBe(1);
    expectClear([label], [caller, callee], placed);

    // 40px between them: nothing fits.
    const close = { ...callee, x: 40 };
    expect(placeLabels([line("b", 0, 0, 40, 0)], [caller, close]).get("b")).toBeNull();
  });

  it("hangs the full label beside the line, over free canvas above the callee, before shortening it", () => {
    // Arrange's 120px column gap, narrower than the full label (190); the line enters the callee 22px
    // below its top, as a header port does, and the canvas above the callee is free.
    const caller = { x: -400, y: -100, width: 400, height: 300 };
    const callee = { x: 120, y: -22, width: 400, height: 200 };
    const label = line("a", 0, 0, 120, 0);
    const placed = placeLabels([label], [caller, callee]);
    // Hanging right of the midpoint (60, 0): left edge at 64, 36 up, clear of the callee's top.
    expect(placed.get("a")).toEqual({ x: 60 + 4 + 95, y: -36, form: 0 });
    expectClear([label], [caller, callee], placed);
  });

  it("finds free canvas beside the card headers when the callees are stacked", () => {
    const caller = { x: -400, y: -150, width: 400, height: 400 };
    const upper = { x: 240, y: -160, width: 380, height: 150 };
    const lower = { x: 240, y: 20, width: 380, height: 260 };
    const labels = [line("up", 0, 0, 240, -140), line("down", 0, 40, 240, 40)];
    const placed = placeLabels(labels, [caller, upper, lower]);
    expect(placed.get("up")).not.toBeNull();
    expect(placed.get("down")).not.toBeNull();
    expectClear(labels, [caller, upper, lower], placed);
  });

  it("keeps labels of crossing lines apart", () => {
    // Two lines crossing at (250, 100), and a third close by.
    const labels = [line("a", 0, 0, 500, 200), line("b", 0, 200, 500, 0), line("c", 0, 20, 500, 180)];
    const placed = placeLabels(labels, []);
    expect([...placed.values()].every((placement) => placement !== null)).toBe(true);
    expectClear(labels, [], placed);
    // All three cross at (250, 100): the first by id keeps it, the others move along their curves.
    expect(placed.get("a")).toEqual({ x: 250, y: 100, form: 0 });
    expect(placed.get("b")?.form).toBe(0);
    expect(placed.get("b")?.x).not.toBe(250);
  });

  it("gives the same placements whatever order the labels come in", () => {
    const cards = [{ x: 400, y: 60, width: 300, height: 120 }];
    const labels = [line("a", 0, 0, 500, 200), line("b", 0, 200, 500, 0), line("c", 0, 100, 400, 100)];
    const forward = placeLabels(labels, cards);
    const backward = placeLabels([...labels].reverse(), cards);
    expect([...backward.entries()].sort()).toEqual([...forward.entries()].sort());
  });
});
