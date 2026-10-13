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
  { id: "receipt", ref: { block: "src/r.ts#function:sendReceipt" }, at: [0, 0] as [number, number] },
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
const items = [review];

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
    const rows = pageRows(thePage, outline, items);
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
    const rows = pageRows(thePage, outline, items);
    const plain = rows.find((row) => row.id === "plain");
    expect(plain?.kind === "link" ? plain.label : null).toBe("link to POST /orders");
    expect(rows.filter((row) => row.review).map((row) => row.id)).toEqual(["charge"]);
  });

  it("lists each proposed step under its card, after its groups and before its links, marked when it needs review", () => {
    const proposed = page("page-1", cards, {
      groups: [{ id: "hold", card: "create-order", title: "Hold stock, then charge", members: [], collapsed: false }],
      links: [{ id: "to-receipt", from: "create-order", to: "receipt" }],
      proposals: [
        { id: "email", card: "create-order", after: `${KEY}/variable:order`, title: "Email a confirmation", author: "agent:claude" },
        { id: "retry", card: "charge", after: `${CHARGE}/call[0]`, title: "Retry once", author: "person:maya" },
      ],
    });
    const missing: HealthItem = { page: "page-1", kind: "missing", target: { proposal: "retry" }, message: "The step this proposal goes after is missing", repairs: ["reconnect", "removeProposal"] };
    const rows = pageRows(proposed, outline, [missing]);
    expect(rows.map((row) => [row.kind, row.id, row.depth, row.review])).toEqual([
      ["card", "post-orders", 0, false],
      ["card", "create-order", 1, false],
      ["group", "hold", 2, false],
      ["proposal", "email", 2, false],
      ["link", "to-receipt", 2, false],
      ["card", "charge", 2, false],
      ["proposal", "retry", 3, true],
      ["card", "receipt", 0, false],
    ]);
    const email = rows.find((row) => row.id === "email");
    expect(email?.kind === "proposal" ? email.proposal.title : null).toBe("Email a confirmation");
  });

  it("lists frames as headings where their first card comes, what they hold under them, nested", () => {
    const framed = page("page-1", cards, {
      frames: [
        { id: "system", title: "System", members: ["server", "charge"], look: "zone", author: "agent:claude" },
        { id: "server", title: "Server", members: ["post-orders", "create-order"], look: "box", author: "agent:claude" },
        { id: "empty", title: "Notes", members: ["a-note"], look: "box", author: "agent:claude" },
      ],
    });
    const rows = pageRows(framed, outline, [{ page: "page-1", kind: "outdated", target: { frame: "server" }, message: "Changed", repairs: [] }]);
    expect(rows.map((row) => [row.kind, row.id, row.depth, row.review])).toEqual([
      ["frame", "system", 0, false],
      ["frame", "server", 1, true],
      ["card", "post-orders", 2, false],
      ["card", "create-order", 3, false],
      ["card", "charge", 1, false],
      ["card", "receipt", 0, false],
      ["frame", "empty", 0, false],
    ]);
  });
});
