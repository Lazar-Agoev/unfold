import { describe, expect, it } from "vitest";
import type { Frame } from "../../ipc/generated";
import { estimateText } from "../measure";
import { FLAT, edgeInsets, frameNesting, insideZones, titleWidth } from "./edges";
import { framePadding } from "./geometry";

const frame = (id: string, members: string[], look: Frame["look"] = "box", title = id): Frame => ({ id, title, members, look, author: "agent:claude" });

describe("frame edges on zoom", () => {
  it("count the frames nested along a frame's top, and anywhere inside it", () => {
    const zone = frame("intake", ["collector", "classifier", "late"], "zone", "Email intake");
    const top = framePadding(zone).top;
    const nesting = frameNesting([
      { frame: zone, rect: { x: 0, y: 0, width: 900, height: 600 } },
      { frame: frame("collector", ["a", "inner"]), rect: { x: 20, y: top, width: 400, height: 300 } },
      { frame: frame("inner", ["b"]), rect: { x: 36, y: top + framePadding({ look: "box" }).top, width: 200, height: 100 } },
      { frame: frame("classifier", ["c"]), rect: { x: 440, y: top, width: 400, height: 300 } },
      { frame: frame("late", ["d"]), rect: { x: 20, y: 400, width: 400, height: 100 } },
      { frame: frame("alone", ["e"]), rect: { x: 2000, y: 0, width: 400, height: 300 } },
    ]);
    // The zone's top holds the collector, whose top holds another box: two deep along the top.
    expect(nesting.get("intake")).toEqual({ top: 2, any: 2 });
    expect(nesting.get("collector")).toEqual({ top: 1, any: 1 });
    expect(nesting.has("classifier")).toBe(false);
    expect(nesting.has("alone")).toBe(false);
  });

  it("move an edge out by the zoom's growth, its nested frames', and, to the right, what its title needs", () => {
    expect(edgeInsets("box", FLAT, 80, 300)).toEqual({
      top: "calc(-1 * (var(--box-grow-top, 0px) + var(--box-grow-top, 0px) * 0))",
      right:
        "calc(-1 * max((var(--box-grow-side, 0px) + var(--box-grow-side, 0px) * 0), (80px * var(--box-title-scale, 1) + 2 * var(--box-title-left, 0px) - (var(--box-grow-side, 0px) + var(--box-grow-side, 0px) * 0) - 300px)))",
      bottom: "calc(-1 * (var(--box-grow-side, 0px) + var(--box-grow-side, 0px) * 0))",
      left: "calc(-1 * (var(--box-grow-side, 0px) + var(--box-grow-side, 0px) * 0))",
    });
    expect(edgeInsets("zone", { top: 1, any: 2 }, 120, 900).top).toBe("calc(-1 * (var(--zone-grow-top, 0px) + var(--box-grow-top, 0px) * 1))");
  });

  it("measure a title at its size at 100%", () => {
    expect(titleWidth({ look: "box", title: "Collector" }, estimateText)).toBeCloseTo(estimateText("Collector", "600 13px Inter"));
    expect(titleWidth({ look: "zone", title: "Email intake" }, estimateText)).toBeGreaterThan(titleWidth({ look: "box", title: "Email intake" }, estimateText));
  });

  it("know the frames inside a zone, at any depth", () => {
    expect(insideZones([frame("zone", ["box"], "zone"), frame("box", ["inner"]), frame("inner", ["a"]), frame("out", ["b"])])).toEqual(new Set(["box", "inner"]));
  });
});
