import { describe, expect, it } from "vitest";
import type { Card, CardView, DrawnFlow } from "../../ipc/generated";
import { holdsText } from "./holds";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });
const golden = GOLDENS["../../../../testdata/goldens/drawn/01-place-order.json"];
if (!golden) {
  throw new Error("no golden 01-place-order");
}
const flow = structuredClone(golden);

const block = (id: string): Card => ({ id, ref: { block: `shop/x.py#${id}` }, at: [0, 0] });
const drawn = (kind: DrawnFlow["header"]["kind"]): CardView => ({ type: "definition", flow: { ...flow, header: { ...flow.header, kind } } });
const start = (kind: "app" | "entrance"): CardView => ({ type: "start", key: `entrance:shop/${kind}`, kind, app: "Shop API", name: kind, code: [] });

describe("what a frame holds", () => {
  it("counts every card and resource card inside it, nested frames' too, in a fixed order, by what the reader drew", () => {
    const page = {
      cards: [block("create"), block("place"), block("service"), block("cart"), { id: "route", ref: { entrance: "entrance:shop/route" }, at: [0, 0] as [number, number] }, block("outside"), block("loading")],
      frames: [
        { id: "server", title: "server", members: ["route", "create", "inner", "orders", "loading"], look: "box" as const, author: "agent:claude" },
        { id: "inner", title: "inner", members: ["place", "service", "cart"], look: "box" as const, author: "agent:claude" },
      ],
      resources: [{ id: "orders", kind: "table" as const, name: "orders", anchors: [], at: [0, 0] as [number, number], author: "agent:claude" }],
    };
    const views = new Map<string, CardView>([
      ["create", drawn("function")],
      ["place", drawn("function")],
      ["service", drawn("class")],
      ["cart", drawn("type")],
      ["route", start("entrance")],
      ["outside", drawn("function")],
    ]);
    expect(holdsText(page, "server", views)).toBe("1 entrance · 2 functions · 1 class · 1 type · 1 card · 1 table");
    expect(holdsText(page, "inner", views)).toBe("1 function · 1 class · 1 type");
    expect(holdsText({ cards: [], frames: [{ id: "n", title: "n", members: ["note"], look: "box", author: "agent:claude" }] }, "n", views)).toBe("nothing yet");
  });
});
