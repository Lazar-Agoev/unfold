import { describe, expect, it } from "vitest";
import type { BoardDocument, HealthItem } from "../../ipc/generated";
import { isValidName, publishedPath, slugOf } from "./files";
import { repairEffect, repairSpec } from "./health";

const KEY = "src/routes/orders.ts#function:createOrder";
const DOC: BoardDocument = {
  format: "unfold-board",
  version: 1,
  title: "Orders",
  defaultView: "code",
  cards: [{ id: "create-order", ref: { block: KEY }, at: [0, 0] }],
  groups: [{ id: "queue", card: "create-order", title: "Queue it", members: [`${KEY}/if[0]`], collapsed: false }],
  labels: {},
  guards: {},
};

describe("board file names", () => {
  it("slugs titles the way the engine does", () => {
    expect(slugOf("Order placement")).toBe("order-placement");
    expect(slugOf("Café Crème")).toBe("cafe-creme");
    expect(slugOf("  POST /orders  ")).toBe("post-orders");
    expect(slugOf("***")).toBe("board");
    expect(publishedPath("order-placement")).toBe(".unfold/boards/order-placement.json");
  });

  it("accepts only plain stems", () => {
    expect(isValidName("orders-2")).toBe(true);
    expect(isValidName("v1.2_final")).toBe(true);
    expect(isValidName("-orders")).toBe(false);
    expect(isValidName("../orders")).toBe(false);
    expect(isValidName("bad name")).toBe(false);
  });
});

describe("repairs", () => {
  const missing: HealthItem = { kind: "missing", target: { card: "create-order" }, message: "not in the graph", repairs: ["reconnect", "removeCard"] };
  const member: HealthItem = { kind: "review", target: { group: "queue" }, key: `${KEY}/if[0]`, message: "the code changed", repairs: ["accept", "reconnect", "ungroup"] };

  it("builds the request for each repair", () => {
    expect(repairSpec("removeCard", missing, DOC)).toEqual({ kind: "remove_card", card: "create-order" });
    expect(repairSpec("reconnect", missing, DOC)).toBeNull();
    expect(repairSpec("reconnect", missing, DOC, "src/routes/orders.ts#function:placeOrder")).toEqual({ kind: "reconnect", from: KEY, to: "src/routes/orders.ts#function:placeOrder" });
    expect(repairSpec("accept", member, DOC)).toEqual({ kind: "accept", key: `${KEY}/if[0]` });
    expect(repairSpec("ungroup", member, DOC)).toEqual({ kind: "ungroup", group: "queue" });
    expect(repairSpec("resign", { ...missing, kind: "outdated" }, DOC)).toEqual({ kind: "resign", target: { card: "create-order" } });
    expect(repairSpec("removeLabel", { kind: "missing", target: { label: `${KEY}/if[0]` }, message: "", repairs: [] }, DOC)).toEqual({ kind: "remove_label", key: `${KEY}/if[0]` });
  });

  it("says what each repair does, naming what it is about", () => {
    expect(repairEffect("removeCard", missing, DOC)).toBe("Removes the card createOrder from the board, with the groups inside it.");
    expect(repairEffect("ungroup", member, DOC)).toContain('Removes the group "Queue it"');
  });
});
