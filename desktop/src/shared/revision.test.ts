import { describe, expect, it } from "vitest";
import { pageRevision } from "./revision";

const PAGE = {
  title: "Placing an order",
  id: "page-1",
  defaultView: "expanded",
  cards: [
    {
      id: "create-order",
      ref: { block: "src/routes/orders.ts#createOrder" },
      at: [0, 0],
      description: { text: "Takes the order — fast", author: "agent:claude", signature: "" },
    },
  ],
  names: [],
  combined: [],
  guards: [],
  links: [],
};

describe("pageRevision", () => {
  it("is FNV-1a 64 over the page's JSON with sorted keys and a two-space indent, as UTF-8 (8 hex digits)", () => {
    // Computed independently: json.dumps(page, sort_keys=True, indent=2, ensure_ascii=False), FNV-1a 64.
    expect(pageRevision(PAGE)).toBe("677ab7c3");
  });

  it("doesn't depend on key order, and changes with any value", () => {
    const reordered = { links: [], guards: [], combined: [], names: [], cards: PAGE.cards, defaultView: "expanded", id: "page-1", title: "Placing an order" };
    expect(pageRevision(reordered)).toBe(pageRevision(PAGE));
    expect(pageRevision({ ...PAGE, title: "Placing an order." })).not.toBe(pageRevision(PAGE));
    expect(pageRevision({ ...PAGE, cards: [{ ...PAGE.cards[0], at: [0, 1] }] })).not.toBe(pageRevision(PAGE));
  });
});
