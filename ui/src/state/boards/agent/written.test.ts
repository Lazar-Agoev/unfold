import { describe, expect, it } from "vitest";
import type { Card, Description } from "../../../ipc/generated";
import { page } from "../../../test/workspace";
import { writtenItems } from "./written";

const KEY = "src/orders.ts#createOrder";
const words = (text: string): Description => ({ text, author: "agent:claude", signature: "" });
const card = (description?: string): Card => ({ id: "create", ref: { block: KEY }, at: [0, 0], ...(description !== undefined ? { description: words(description) } : {}) });

describe("what an agent's edit wrote", () => {
  it("names a card's fields whose words are new: its title, its description, the steps it draws", () => {
    const was = page("p", [card()]);
    const now = page("p", [card("Takes an order.")], {
      names: [{ key: KEY, text: "Take an order", author: "agent:claude", signature: "" }, { key: `${KEY}/call:save`, text: "Save it", author: "agent:claude", signature: "" }],
    });
    expect(writtenItems(was, now, ["create"])).toEqual([{ id: "create", fields: ["title", "description", "steps"] }]);
    // Moved, or added from code: nothing written.
    expect(writtenItems(page("p", [card("Takes an order.")]), page("p", [{ ...card("Takes an order."), at: [40, 0] }]), ["create"])).toEqual([{ id: "create", fields: [] }]);
    expect(writtenItems(page("p"), page("p", [card()]), ["create"])).toEqual([{ id: "create", fields: [] }]);
  });

  it("names what it wrote on a connection, a frame, a resource card, and a note, new or changed", () => {
    const now = page("p", [card()], {
      links: [{ id: "reads", from: "create", to: "orders", kind: "reads", author: "agent:claude" }],
      frames: [{ id: "api", title: "Order API", members: ["create"], look: "box", author: "agent:claude" }],
      resources: [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 0], description: words("One row per order."), author: "agent:claude" }],
      notes: [{ id: "note", text: "Is this idempotent?", at: [0, 0], author: "agent:claude" }],
    });
    expect(writtenItems(page("p", [card()]), now, ["reads", "api", "orders", "note", "gone"])).toEqual([
      { id: "reads", fields: ["target"] },
      { id: "api", fields: ["title"] },
      { id: "orders", fields: ["name", "description"] },
      { id: "note", fields: ["text"] },
      { id: "gone", fields: [] },
    ]);
    const described = page("p", [card()], { ...now, frames: [{ id: "api", title: "Order API", description: words("Takes orders."), members: ["create"], look: "box", author: "agent:claude" }] });
    expect(writtenItems(now, described, ["api", "orders"])).toEqual([
      { id: "api", fields: ["description"] },
      { id: "orders", fields: [] },
    ]);
  });
});
