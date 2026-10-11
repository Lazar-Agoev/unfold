import { describe, expect, it, vi } from "vitest";
import { LabelLayout } from "./labelLayout";
import { bezierCurve } from "./placeLabels";

const spec = (x2: number) => ({ curve: bezierCurve({ x: 0, y: 0 }, "right", { x: x2, y: 0 }, "left"), widths: [100, 60], height: 20 });

describe("LabelLayout", () => {
  it("places what was reported, tells listeners once per batch, and ignores reports that change nothing", async () => {
    const layout = new LabelLayout();
    const listener = vi.fn();
    layout.subscribe(listener);
    expect(layout.placement("a")).toBeUndefined();

    layout.report("a", spec(400));
    layout.report("b", spec(400));
    layout.setCards([{ x: 400, y: -100, width: 300, height: 200 }]);
    await Promise.resolve();
    expect(listener).toHaveBeenCalledTimes(1);
    const first = layout.placement("a");
    expect(first).toEqual({ x: 200, y: 0, form: 0 });
    expect(layout.placement("b")).not.toBeNull();

    // The same curve, widths, and cards again: nothing to place, nobody to tell.
    layout.report("a", spec(400));
    layout.setCards([{ x: 400, y: -100, width: 300, height: 200 }]);
    await Promise.resolve();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(layout.placement("a")).toBe(first);
  });

  it("keeps an unchanged placement's identity when another label moves, and forgets a label that is gone", async () => {
    const layout = new LabelLayout();
    layout.report("a", spec(400));
    layout.report("far", { ...spec(400), curve: bezierCurve({ x: 0, y: 500 }, "right", { x: 400, y: 500 }, "left") });
    const kept = layout.placement("a");
    layout.report("far", { ...spec(400), curve: bezierCurve({ x: 0, y: 600 }, "right", { x: 400, y: 600 }, "left") });
    expect(layout.placement("a")).toBe(kept);
    expect(layout.placement("far")).toEqual({ x: 200, y: 600, form: 0 });

    layout.report("far", null);
    await Promise.resolve();
    expect(layout.placement("far")).toBeUndefined();
  });
});
