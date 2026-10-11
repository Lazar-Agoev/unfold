import { describe, expect, it } from "vitest";
import type { CardBody } from "../../code/typeKinds";
import type { Link } from "../../ipc/generated";
import { lineHandles, relatedColumns, stepPorts } from "./routing";

const STEP = "src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]";
const left = { x: 0, y: 0, width: 300, height: 200 };
const right = { x: 600, y: 0, width: 300, height: 200 };
const queue: Link = { id: "enqueue", from: "create-order", to: "worker", kind: "queue", step: STEP, author: "agent:claude" };
const relates: Link = { id: "owner", from: "orders", to: "customers", kind: "relates", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one", author: "agent:claude" };
const bodies = (body: CardBody) => (id: string) => (id === "create-order" ? body : undefined);

describe("where a connection leaves and enters", () => {
  it("leaves from its step's line in Code and its step in Flow, and from the card's edge in Diagram", () => {
    const own = { from: queue.from, to: queue.to };
    expect(lineHandles(queue, own, left, right, bodies("code"))).toEqual({ source: `step:${STEP}`, target: "link-in-left" });
    expect(lineHandles(queue, own, left, right, bodies("flow")).source).toBe(`step:${STEP}`);
    expect(lineHandles(queue, own, left, right, bodies("node"))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("attaches to a collapsed frame's edge when an end is hidden in it", () => {
    expect(lineHandles(queue, { from: "server", to: queue.to }, left, right, bodies("code"))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("joins a relationship's columns, leaving and entering by the edges facing each other", () => {
    const own = { from: relates.from, to: relates.to };
    expect(lineHandles(relates, own, left, right, () => undefined)).toEqual({ source: "col-out-right:customerId", target: "col-in-left:id" });
    expect(lineHandles(relates, own, right, left, () => undefined)).toEqual({ source: "col-out-left:customerId", target: "col-in-right:id" });
    expect(lineHandles(relates, { from: relates.from, to: "data" }, left, right, () => undefined)).toEqual({ source: "col-out-right:customerId", target: "link-in-left" });
  });

  it("leaves a drawn link by the facing sides", () => {
    expect(lineHandles({ id: "l", from: "a", to: "b" }, { from: "a", to: "b" }, left, { ...left, y: 400 }, bodies("code"))).toEqual({ source: "link-out-bottom", target: "link-in-top" });
  });
});

describe("the ports and columns connections need", () => {
  it("give a card a step port only where it shows Code or Flow and is drawn as itself", () => {
    expect([...stepPorts([queue], new Map(), bodies("code"))]).toEqual([["create-order", [`step:${STEP}`]]]);
    expect([...stepPorts([queue], new Map(), bodies("node"))]).toEqual([]);
    expect([...stepPorts([queue], new Map([["create-order", "server"]]), bodies("code"))]).toEqual([]);
    expect([...stepPorts([{ id: "drawn", from: "create-order", to: "worker", step: STEP }], new Map(), bodies("code"))]).toEqual([]);
  });

  it("give a table the columns its relationships use", () => {
    expect([...relatedColumns([relates, { ...relates, id: "other", fromColumn: "email", toColumn: "email" }], new Map())]).toEqual([
      ["orders", ["customerId", "email"]],
      ["customers", ["id", "email"]],
    ]);
    expect([...relatedColumns([relates], new Map([["orders", "data"]]))]).toEqual([["customers", ["id"]]]);
  });
});
