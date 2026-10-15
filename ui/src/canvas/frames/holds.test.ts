import { describe, expect, it } from "vitest";
import type { Card } from "../../ipc/generated";
import { holdsText } from "./holds";

const block = (id: string): Card => ({ id, ref: { block: `shop/x.py#${id}` }, at: [0, 0] });

describe("what a frame holds", () => {
  it("counts every card and resource card inside it, nested frames' too, and no note", () => {
    const page = {
      cards: [block("create"), block("place"), block("service"), { id: "route", ref: { entrance: "entrance:shop/route" }, at: [0, 0] as [number, number] }, block("outside")],
      frames: [
        { id: "server", title: "server", members: ["route", "create", "inner", "orders", "note"], look: "box" as const, author: "agent:claude" },
        { id: "inner", title: "inner", members: ["place", "service"], look: "box" as const, author: "agent:claude" },
      ],
      resources: [{ id: "orders", kind: "table" as const, name: "orders", anchors: [], at: [0, 0] as [number, number], author: "agent:claude" }],
      notes: [{ id: "note", text: "Why?", at: [0, 0] as [number, number], author: "agent:claude" }],
    };
    expect(holdsText(page, "server")).toBe("5 cards");
    expect(holdsText(page, "inner")).toBe("2 cards");
    expect(holdsText({ cards: [block("one")], frames: [{ id: "f", title: "f", members: ["one"], look: "box", author: "agent:claude" }] }, "f")).toBe("1 card");
    expect(holdsText({ cards: [], frames: [{ id: "n", title: "n", members: ["note"], look: "box", author: "agent:claude" }] }, "n")).toBe("nothing yet");
  });
});
