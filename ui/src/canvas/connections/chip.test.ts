import { Position } from "@xyflow/react";
import { describe, expect, it } from "vitest";
import { cardinalityText, chipForms, chipText, type ChipInput } from "./chip";
import { arrowHead, cardinalityMark, manyEnds } from "./ends";

const forms = (input: ChipInput) => chipForms(input).map(chipText);

describe("a connection's chip", () => {
  it("says its kind's word, then what travels: the channel, the type it carries, and its label, with shorter forms", () => {
    expect(forms({ kind: "queue", channel: "orders", carries: "OrderJob", label: "place-order job" })).toEqual([
      "queue · orders · OrderJob · place-order job",
      "queue · orders · OrderJob",
      "queue · orders",
      "queue",
    ]);
    expect(forms({ kind: "http", channel: "POST /orders" })).toEqual(["HTTP · POST /orders", "HTTP"]);
    expect(forms({ kind: "http" })).toEqual(["HTTP"]);
  });

  it("names the table reads and writes touch when they name no channel", () => {
    expect(forms({ kind: "writes", subject: "orders" })).toEqual(["writes · orders", "writes"]);
    expect(forms({ kind: "reads", subject: "orders", channel: "orders_v2" })[0]).toBe("reads · orders_v2");
  });

  it("reads 'runtime call' for calls, the cardinality for a relationship, and the label for other", () => {
    expect(forms({ kind: "calls", label: "Stripe at runtime" })).toEqual(["runtime call · Stripe at runtime", "runtime call"]);
    expect(forms({ kind: "relates", cardinality: "many-to-one" })).toEqual(["N : 1"]);
    expect(forms({ kind: "relates" })).toEqual(["relates"]);
    expect(forms({ kind: "other", label: "nightly export" })).toEqual(["nightly export"]);
    expect(chipForms({ kind: "other", label: "nightly export" })[0]?.[0]?.tone).toBe("text");
  });

  it("counts the connections of one kind merged at a collapsed frame", () => {
    expect(forms({ kind: "queue", count: 3 })).toEqual(["queue · 3 connections", "queue"]);
  });
});

describe("a relationship's ends", () => {
  it("read their cardinality as marks", () => {
    expect(cardinalityText("one-to-one")).toBe("1 : 1");
    expect(cardinalityText("one-to-many")).toBe("1 : N");
    expect(cardinalityText("many-to-many")).toBe("N : N");
    expect(manyEnds("one-to-many")).toEqual({ from: false, to: true });
  });

  it("draw a crow's foot for many (three strokes into the card) and a bar for one", () => {
    // Leaving a card's right side at (100, 50): the foot's point stands 10px out, its strokes reach the edge.
    expect(cardinalityMark(100, 50, Position.Right, true)).toBe("M110 50 L100 56 M110 50 L100 50 M110 50 L100 44");
    expect(cardinalityMark(100, 50, Position.Right, false)).toBe("M107 56 L107 44");
    // Entering a card's left side: the marks stand out to the left.
    expect(cardinalityMark(100, 50, Position.Left, false)).toBe("M93 44 L93 56");
  });

  it("share drawn links' arrowheads", () => {
    expect(arrowHead(100, 50, [1, 0], 7, 8)).toBe("M93 54 L100 50 L93 46 Z");
  });
});
