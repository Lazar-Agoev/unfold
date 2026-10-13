import { describe, expect, it } from "vitest";
import { authorText, cardText, joined, keyFile, keyKind, keyName, place, typeText } from "./format";

describe("names", () => {
  it("name keys as boards do", () => {
    expect(keyName("src/services/order-service.ts#class:OrderService/method:place")).toBe("OrderService.place");
    expect(keyName("src/routes/orders.ts#function:createOrder/if[0]")).toBe("createOrder.if");
    expect(keyName("entrance:configured:import-orders")).toBe("import-orders");
    expect(keyName('entrance:["express.route","http","POST","/orders","src/routes/orders.ts"]')).toBe("POST /orders");
    expect(keyName("src/routes/orders.ts#")).toBe("src/routes/orders.ts");
    expect(keyFile("src/routes/orders.ts#function:createOrder")).toBe("src/routes/orders.ts");
    expect(keyFile('entrance:["x"]')).toBeUndefined();
    // A path may hold `#`; names escape it.
    expect(keyFile("src/#lib/cart.ts#interface:Cart")).toBe("src/#lib/cart.ts");
    expect(keyName("src/#lib/cart.ts#interface:Cart")).toBe("Cart");
    expect(keyKind("src/#lib/cart.ts#interface:Cart")).toBe("interface");
    expect(place("src/a.ts", 3, 9)).toBe("src/a.ts:3–9");
    expect(place("package.json", null)).toBe("package.json");
  });

  it("write types with what their names link to", () => {
    expect(
      typeText({
        text: "Map<string, Money>",
        spans: [
          { start: 0, end: 3, style: "builtin" },
          { start: 4, end: 10, style: "builtin" },
          { start: 12, end: 17, style: "unresolved", note: "a variable, not a type" },
        ],
      }),
    ).toBe("Map<string, Money> (Money: unresolved, a variable, not a type)");
    expect(typeText({ text: "Queue<OrderJob>", spans: [{ start: 0, end: 5, style: "external", module: "bullmq", note: "bullmq" }] })).toBe("Queue<OrderJob> (Queue: from bullmq)");
  });

  it("say where following stopped at a card", () => {
    expect(
      cardText({
        kind: "card",
        id: "place",
        status: "created",
        key: "k",
        name: "OrderService.place",
        stops: [
          { reason: "unresolved", call: "this.payments.charge", line: 18, note: "overridden by a subclass" },
          { reason: "depth", count: 3 },
          { reason: "builtin", type: "Date", field: "createdAt" },
        ],
      }),
    ).toBe("OrderService.place (card id place, created); stopped: this.payments.charge (line 18) is unresolved: overridden by a subclass; 3 more not followed, past the depth asked; Date (createdAt) is built in");
    expect(cardText({ kind: "card", id: "a", status: "kept", key: "k", name: "a", frame: "server" })).toBe("a (card id a, kept, in frame server)");
  });

  it("name a key's kind, who wrote something, and facts", () => {
    expect(keyKind("src/cart.ts#interface:Cart")).toBe("interface");
    expect(keyKind("src/routes/orders.ts#function:createOrder/if[0]")).toBe("if");
    expect(keyKind('entrance:["x"]')).toBe("entrance");
    const claude = { author: "agent:claude", name: "Claude" } as const;
    expect([authorText("agent:claude", claude), authorText("agent:codex", claude), authorText("person:lazar", claude), authorText(undefined, claude)]).toEqual([
      "you",
      "Codex (an agent)",
      "Lazar (a person)",
      "a person",
    ]);
    expect(joined(["a", null, "", undefined, false, "b"])).toBe("a; b");
  });
});
