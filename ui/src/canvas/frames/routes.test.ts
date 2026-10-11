import { describe, expect, it } from "vitest";
import type { Link } from "../../ipc/generated";
import { NODE_PORT, type Arrow } from "../arrows";
import { mergedText } from "../lines/labelText";
import { hasLineLabel } from "../lines/useLineLabels";
import { routeArrows, routeLinks } from "./routes";

function arrow(source: string, port: string, target: string, extra: Partial<Arrow> = {}): Arrow {
  return { id: `${source}|${port}|${target}`, source, sourceHandle: port, target, project: "p", occurrence: port, seq: 0, kind: "calls", dashed: false, order: 0, line: 12, ...extra };
}

/** `checks` (collapsed) holds a and b; `api` (collapsed) holds c. */
const SHOWN = new Map([
  ["a", "checks"],
  ["b", "checks"],
  ["c", "api"],
]);

describe("arrows at collapsed frames", () => {
  it("are the same arrows when nothing is collapsed", () => {
    const arrows = [arrow("x", "s1", "a")];
    expect(routeArrows(arrows, new Map())).toBe(arrows);
  });

  it("attach to the collapsed frame, keeping one call's id, label, and cards", () => {
    const routed = routeArrows([arrow("x", "s1", "a"), arrow("c", "s2", "y")], SHOWN);
    expect(routed).toEqual([
      { ...arrow("x", "s1", "a"), target: "checks", cards: { from: "x", to: "a" } },
      { ...arrow("c", "s2", "y"), source: "api", sourceHandle: NODE_PORT, cards: { from: "c", to: "y" } },
    ]);
    expect(routed.every(hasLineLabel)).toBe(true);
  });

  it("aren't drawn inside one collapsed frame, and merge where they would overlap, counting what they stand for", () => {
    const routed = routeArrows(
      [
        arrow("a", "s1", "b"),
        arrow("a", "s1", "c"),
        arrow("b", "s2", "c", { calls: [{ source: "s2", seq: 0, line: 4, kind: "calls" }, { source: "s3", seq: 0, line: 5, kind: "calls" }] }),
        arrow("b", "field:0", "c", { kind: "type", line: null }),
      ],
      SHOWN,
    );
    expect(routed).toHaveLength(1);
    expect(routed[0]).toMatchObject({ id: `checks|${NODE_PORT}|api`, source: "checks", target: "api", line: null, merged: { calls: 3, fields: 1 } });
    expect(routed[0]?.calls).toBeUndefined();
    expect(routed.some(hasLineLabel)).toBe(false);
    expect(mergedText(3, 1)).toBe("3 calls · 1 field");
    expect(mergedText(1, 0)).toBe("1 call");
  });
});

describe("drawn links at collapsed frames", () => {
  const link = (id: string, from: string, to: string): Link => ({ id, from, to });

  it("attach to the frame, aren't drawn inside it, and merge into one that counts them", () => {
    const routed = routeLinks([link("one", "x", "a"), link("inside", "a", "b"), link("two", "c", "y"), link("three", "c", "y")], SHOWN);
    expect(routed).toEqual([
      { id: "one", from: "x", to: "checks", link: link("one", "x", "a"), count: 1 },
      { id: "api|links|y", from: "api", to: "y", count: 2 },
    ]);
  });
});
