import { describe, expect, it } from "vitest";
import type { Connection } from "../../ipc/generated";
import { lineHandles, relatedColumns, stepPorts } from "./routing";

const STEP = "src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add";
const across = { from: "right", to: "left" } as const;
const queue: Connection = { id: "enqueue", from: "create-order", to: "worker", kind: "queue", step: STEP, author: "agent:claude" };
const relates: Connection = { id: "owner", from: "orders", to: "customers", kind: "relates", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one", author: "agent:claude" };
/** Whether create-order draws the step (expanded, its flow drawing it). */
const draws = (drawn: boolean) => (id: string, step: string) => drawn && id === "create-order" && step === STEP;

describe("where a connection's edge attaches", () => {
  it("at its step's port on an expanded card that draws it, and at the card's side otherwise", () => {
    const own = { from: queue.from, to: queue.to };
    expect(lineHandles(queue, own, across, draws(true))).toEqual({ source: `step:${STEP}`, target: "link-in-left" });
    expect(lineHandles(queue, own, across, draws(false))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("at a collapsed frame's side when an end is hidden in it", () => {
    expect(lineHandles(queue, { from: "server", to: queue.to }, across, draws(true))).toEqual({ source: "link-out-right", target: "link-in-left" });
  });

  it("at a relationship's columns, on the sides it is routed on (a C on one side between stacked tables)", () => {
    const own = { from: relates.from, to: relates.to };
    expect(lineHandles(relates, own, across, () => false)).toEqual({ source: "col-out-right:customerId", target: "col-in-left:id" });
    expect(lineHandles(relates, own, { from: "left", to: "right" }, () => false)).toEqual({ source: "col-out-left:customerId", target: "col-in-right:id" });
    expect(lineHandles(relates, own, { from: "left", to: "left" }, () => false)).toEqual({ source: "col-out-left:customerId", target: "col-in-left:id" });
    expect(lineHandles(relates, { from: relates.from, to: "data" }, across, () => false)).toEqual({ source: "col-out-right:customerId", target: "link-in-left" });
  });

  it("at the sides its route uses, right to left until it is routed", () => {
    const below = { id: "l", from: "a", to: "b", kind: "http" as const, author: "agent:claude" };
    expect(lineHandles(below, { from: "a", to: "b" }, { from: "bottom", to: "top" }, draws(true))).toEqual({ source: "link-out-bottom", target: "link-in-top" });
    expect(lineHandles(below, { from: "a", to: "b" }, undefined, draws(true))).toEqual({ source: "link-out-right", target: "link-in-left" });
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
