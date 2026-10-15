import { describe, expect, it } from "vitest";
import type { Connection } from "../../ipc/generated";
import { lineHandles, relatedColumns, stepPorts } from "./routing";

const STEP = "src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add";
const left = { x: 0, y: 0, width: 300, height: 200 };
const right = { x: 600, y: 0, width: 300, height: 200 };
const queue: Connection = { id: "enqueue", from: "create-order", to: "worker", kind: "queue", step: STEP, author: "agent:claude" };
const relates: Connection = { id: "owner", from: "orders", to: "customers", kind: "relates", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one", author: "agent:claude" };
/** Whether create-order draws the step (expanded, its flow drawing it). */
const draws = (drawn: boolean) => (id: string, step: string) => drawn && id === "create-order" && step === STEP;

describe("where a connection leaves and enters", () => {
  it("leaves from its step's call on an expanded card that draws it, and from the card's edge otherwise", () => {
    const own = { from: queue.from, to: queue.to };
    expect(lineHandles(queue, own, left, right, draws(true))).toEqual({ source: `step:${STEP}`, target: "link-in-left" });
    expect(lineHandles(queue, own, left, right, draws(false))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("attaches to a collapsed frame's edge when an end is hidden in it", () => {
    expect(lineHandles(queue, { from: "server", to: queue.to }, left, right, draws(true))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("joins a relationship's columns, leaving and entering by the edges facing each other", () => {
    const own = { from: relates.from, to: relates.to };
    expect(lineHandles(relates, own, left, right, () => false)).toEqual({ source: "col-out-right:customerId", target: "col-in-left:id" });
    expect(lineHandles(relates, own, right, left, () => false)).toEqual({ source: "col-out-left:customerId", target: "col-in-right:id" });
    expect(lineHandles(relates, { from: relates.from, to: "data" }, left, right, () => false)).toEqual({ source: "col-out-right:customerId", target: "link-in-left" });
  });

  it("leaves a connection without a step by the facing sides", () => {
    const below = { id: "l", from: "a", to: "b", kind: "http" as const, author: "agent:claude" };
    expect(lineHandles(below, { from: "a", to: "b" }, left, { ...left, y: 400 }, draws(true))).toEqual({ source: "link-out-bottom", target: "link-in-top" });
  });
});

describe("the ports and columns connections need", () => {
  it("give a card a step port only where it draws the step expanded, as itself, and never for a relationship", () => {
    expect([...stepPorts([queue], new Map(), draws(true))].map(([card, ports]) => [card, ports.map((port) => port.id)])).toEqual([["create-order", [`step:${STEP}`]]]);
    expect([...stepPorts([queue], new Map(), draws(false))]).toEqual([]);
    expect([...stepPorts([queue], new Map([["create-order", "server"]]), draws(true))]).toEqual([]);
    expect([...stepPorts([{ ...relates, from: "create-order", step: STEP }], new Map(), draws(true))]).toEqual([]);
  });

  it("give a table the columns its relationships use", () => {
    expect([...relatedColumns([relates, { ...relates, id: "other", fromColumn: "email", toColumn: "email" }], new Map())]).toEqual([
      ["orders", ["customerId", "email"]],
      ["customers", ["id", "email"]],
    ]);
    expect([...relatedColumns([relates], new Map([["orders", "data"]]))]).toEqual([["customers", ["id"]]]);
  });
});
