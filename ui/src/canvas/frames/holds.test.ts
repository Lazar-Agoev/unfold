import { describe, expect, it } from "vitest";
import type { Card } from "../../ipc/generated";
import { holdsText } from "./holds";

const block = (id: string, key: string): Card => ({ id, ref: { block: key }, at: [0, 0] });

describe("what a frame holds", () => {
  it("counts every card and resource card inside it, nested frames' too, in a fixed order", () => {
    const page = {
      cards: [
        block("create", "src/a.ts#function:createOrder"),
        block("place", "src/s.ts#class:OrderService/method:place"),
        block("service", "src/s.ts#class:OrderService"),
        block("cart", "src/t.ts#interface:Cart"),
        { id: "route", ref: { entrance: 'entrance:["express.route","http","POST","/orders"]' }, at: [0, 0] as [number, number] },
        block("outside", "src/x.ts#function:outside"),
      ],
      frames: [
        { id: "server", title: "server", members: ["route", "create", "inner", "orders"], look: "box" as const, author: "agent:claude" },
        { id: "inner", title: "inner", members: ["place", "service", "cart"], look: "box" as const, author: "agent:claude" },
      ],
      resources: [{ id: "orders", kind: "table" as const, name: "orders", anchors: [], at: [0, 0] as [number, number], author: "agent:claude" }],
    };
    expect(holdsText(page, "server")).toBe("1 entrance · 2 functions · 1 class · 1 type · 1 table");
    expect(holdsText(page, "inner")).toBe("1 function · 1 class · 1 type");
    expect(holdsText({ cards: [], frames: [{ id: "n", title: "n", members: ["note"], look: "box", author: "agent:claude" }] }, "n")).toBe("nothing yet");
  });
});
