import { describe, expect, it } from "vitest";
import type { Frame } from "../../ipc/generated";
import { estimateText } from "../measure";
import { insideZones, titleFades } from "./titles";

const frame = (id: string, members: string[], look: Frame["look"] = "box", title = id): Frame => ({ id, title, members, look, author: "agent:claude" });

describe("frame titles on zoom", () => {
  it("fade a box's title before it runs into the title of the zone around it, and leave alone boxes outside any", () => {
    const zone = frame("intake", ["collector"], "zone", "Email intake");
    const box = frame("collector", ["a"], "box", "Collector");
    const alone = frame("alone", ["b"]);
    const fades = titleFades(
      [
        { frame: zone, rect: { x: 0, y: 0, width: 900, height: 400 } },
        { frame: box, rect: { x: 20, y: 62, width: 400, height: 300 } },
        { frame: alone, rect: { x: 2000, y: 0, width: 400, height: 300 } },
      ],
      estimateText,
    );
    const at = fades.get("collector");
    // Readable at 50%, gone by 25% (07): it grows up from its corner into the zone's title.
    expect(at).toBeLessThan(0.5);
    expect(at).toBeGreaterThan(0.25);
    expect(fades.has("alone")).toBe(false);
    expect(fades.has("intake")).toBe(false);
  });

  it("know the frames inside a zone, at any depth", () => {
    expect(insideZones([frame("zone", ["box"], "zone"), frame("box", ["inner"]), frame("inner", ["a"]), frame("out", ["b"])])).toEqual(new Set(["box", "inner"]));
  });
});
