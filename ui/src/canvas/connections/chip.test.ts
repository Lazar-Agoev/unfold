import { Position } from "@xyflow/react";
import { describe, expect, it } from "vitest";
import { cardinalityText, chipForms, chipText, connectionTitle, type ChipInput } from "./chip";
import { arrowHead, cardinalityMark, manyEnds } from "./ends";

const forms = (input: ChipInput) => chipForms(input).map(chipText);

describe("a connection's chip", () => {
  it("says its kind's word, then what it touches; its card says the type it carries and its label too", () => {
    const queue: ChipInput = { kind: "queue", channel: "orders", carries: "OrderJob", label: "place-order job" };
    expect(forms(queue)).toEqual(["queue orders", "queue"]);
    expect(chipText(connectionTitle(queue))).toBe("queue orders · OrderJob · place-order job");
    expect(forms({ kind: "http", channel: "POST /orders" })).toEqual(["HTTP POST /orders", "HTTP"]);
    expect(forms({ kind: "http" })).toEqual(["HTTP"]);
    expect(chipForms({ kind: "http", channel: "POST /orders" })[0]?.map((part) => part.tone)).toEqual(["word", "code"]);
  });

  it("names the table reads and writes touch when they name no channel", () => {
    expect(forms({ kind: "writes", subject: "orders" })).toEqual(["writes orders", "writes"]);
    expect(forms({ kind: "reads", subject: "orders", channel: "orders_v2" })[0]).toBe("reads orders_v2");
  });

  it("reads 'runtime call' for calls and the label for other; a relationship has no chip, its card says the cardinality", () => {
    expect(forms({ kind: "calls", label: "Stripe at runtime" })).toEqual(["runtime call"]);
    expect(chipText(connectionTitle({ kind: "calls", label: "Stripe at runtime" }))).toBe("runtime call · Stripe at runtime");
    expect(forms({ kind: "relates", cardinality: "many-to-one" })).toEqual([]);
    expect(chipText(connectionTitle({ kind: "relates", cardinality: "many-to-one" }))).toBe("N : 1");
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

  it("draw an ERD's ends: a crow's foot with a bar across its tip for many, two bars for one", () => {
    // Leaving a card's right side at (100, 50): the toes meet 10px out, where the bar crosses.
    expect(cardinalityMark(100, 50, Position.Right, true)).toBe("M100 57 L110 50 L100 43 M110 57 L110 43");
    expect(cardinalityMark(100, 50, Position.Right, false)).toBe("M108 56 L108 44 M113 56 L113 44");
    // Entering a card's left side: the marks stand out to the left.
    expect(cardinalityMark(100, 50, Position.Left, false)).toBe("M92 44 L92 56 M87 44 L87 56");
  });

  it("share drawn links' arrowheads", () => {
    expect(arrowHead(100, 50, [1, 0], 7, 8)).toBe("M93 54 L100 50 L93 46 Z");
  });
});
