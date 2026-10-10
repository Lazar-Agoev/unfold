import { describe, expect, it } from "vitest";
import type { HealthItem, PageOutline } from "../ipc/generated";
import { page } from "../test/workspace";
import { followOrder, pageRows } from "./pageRows";

const KEY = "src/routes/orders.ts#function:createOrder";
const ROUTE = 'entrance:["express.route","http","POST","/orders"]';
const CHARGE = "src/pay.ts#function:charge";

const cards = [
  { id: "charge", ref: { block: CHARGE }, at: [0, 0] as [number, number] },
  { id: "post-orders", ref: { entrance: ROUTE }, at: [0, 0] as [number, number] },
  { id: "create-order", ref: { block: KEY }, at: [0, 0] as [number, number] },
  { id: "receipt", ref: { block: "src/r.ts#function:sendReceipt", project: "billing" }, at: [0, 0] as [number, number] },
];
const outline: PageOutline = {
  page: "page-1",
  rows: [
    { card: "post-orders", depth: 0 },
    { card: "create-order", depth: 1 },
    { card: "charge", depth: 2 },
  ],
  unlabeled: [],
};
const thePage = page("page-1", cards, {
  groups: [{ id: "hold", card: "create-order", title: "Hold stock, then charge", members: [], collapsed: false }],
  links: [
    { id: "to-receipt", from: "create-order", to: "receipt", label: { text: "queues a receipt", author: "person:maya", signature: "" } },
    { id: "plain", from: "charge", to: "post-orders" },
  ],
});
const review: HealthItem = { page: "page-1", kind: "review", target: { card: "charge" }, message: "Changed", repairs: [] };
const notConnected: HealthItem = { page: "page-1", kind: "notConnected", target: { card: "receipt" }, message: "Not connected", repairs: [] };
const context = { boardProject: "orders", connected: (project: string | undefined) => project === undefined, items: [review, notConnected] };

describe("On this page", () => {
  it("lists the cards in follow order, with cards added since the last save last", () => {
    expect(followOrder(outline, cards).map(({ card, depth }) => [card.id, depth])).toEqual([
      ["post-orders", 0],
      ["create-order", 1],
      ["charge", 2],
      ["receipt", 0],
    ]);
  });

  it("nests a card's groups, then its links one level deeper", () => {
    const { rows } = pageRows(thePage, outline, context);
    expect(rows.map((row) => [row.kind, row.id, row.depth])).toEqual([
      ["card", "post-orders", 0],
      ["card", "create-order", 1],
      ["group", "hold", 2],
      ["link", "to-receipt", 2],
      ["card", "charge", 2],
      ["link", "plain", 3],
      ["card", "receipt", 0],
    ]);
  });

  it("names unlabeled links after their target, and marks rows that need review", () => {
    const { rows } = pageRows(thePage, outline, context);
    const plain = rows.find((row) => row.id === "plain");
    expect(plain?.kind === "link" ? plain.label : null).toBe("link to POST /orders");
    expect(rows.filter((row) => row.review).map((row) => row.id)).toEqual(["charge"]);
  });

  it("shows the project column only when the page mixes projects, and which cards aren't connected", () => {
    const mixed = pageRows(thePage, outline, context);
    expect(mixed.multiProject).toBe(true);
    const receipt = mixed.rows.find((row) => row.id === "receipt");
    expect(receipt?.kind === "card" ? [receipt.project, receipt.connected] : null).toEqual(["billing", false]);
    const own = pageRows(page("page-1", cards.slice(0, 3)), outline, context);
    expect(own.multiProject).toBe(false);
  });
});
