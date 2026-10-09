import { describe, expect, it } from "vitest";
import type { CardView, FlowModel, FlowNode } from "../ipc/generated";
import { fixture } from "../test/engine";
import { layoutFlow, roundedPath, wrapLines, type FlowLayout, type Measure } from "./layout";

/** A fixed-width measurer, so the tests don't depend on fonts. */
const measure: Measure = (text) => text.length * 7;
const options = { measure, maxWidth: 878 };

const card = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
if (!card.flow) {
  throw new Error("the fixture has a Flow model");
}
const createOrder: FlowModel = card.flow;

function overlaps(layout: FlowLayout): boolean {
  return layout.nodes.some((a, i) =>
    layout.nodes.some((b, j) => i < j && a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height),
  );
}

function node(id: number, kind: FlowNode["kind"], extra: Partial<FlowNode> = {}): FlowNode {
  return { id, kind, label: `${kind} ${id}`, lane: 0, key: `src/a.ts#function:a/n${id}`, ...extra };
}

/** `if (a) throw; else if (b) throw; else if (c) throw; done(); return;` */
const chain: FlowModel = {
  key: "src/a.ts#function:a",
  path: "src/a.ts",
  language: "typescript",
  kind: "function",
  source: "current",
  nodes: [
    node(0, "start", { label: "a" }),
    node(1, "condition", { checks: 3, outcome: "throw", fallback: true }),
    node(2, "throw"),
    node(3, "condition"),
    node(4, "throw"),
    node(5, "condition"),
    node(6, "throw"),
    node(7, "call"),
    node(8, "end", { label: "End" }),
  ],
  routes: [
    { from: 0, to: 1, kind: "sequence" },
    { from: 1, to: 2, kind: "yes" },
    { from: 1, to: 3, kind: "elseIf" },
    { from: 3, to: 4, kind: "yes" },
    { from: 3, to: 5, kind: "elseIf" },
    { from: 5, to: 6, kind: "yes" },
    { from: 5, to: 7, kind: "no" },
    { from: 7, to: 8, kind: "sequence" },
  ],
  tree: [
    {
      type: "branch",
      node: 1,
      yes: [{ type: "node", node: 2, ends: true, lanes: 1 }],
      no: [
        {
          type: "branch",
          node: 3,
          yes: [{ type: "node", node: 4, ends: true, lanes: 1 }],
          no: [{ type: "branch", node: 5, yes: [{ type: "node", node: 6, ends: true, lanes: 1 }], no: [], yesEnds: true, noEnds: false, ends: false, lanes: 2, yesLane: 1, elseIf: false }],
          yesEnds: true,
          noEnds: false,
          ends: false,
          lanes: 2,
          yesLane: 1,
          elseIf: true,
        },
      ],
      yesEnds: true,
      noEnds: false,
      ends: false,
      lanes: 2,
      yesLane: 1,
      elseIf: true,
    },
    { type: "node", node: 7, ends: false, lanes: 1 },
  ],
  start: 0,
  end: 8,
  lanes: 2,
  warnings: [],
};

describe("Flow layout", () => {
  it("is deterministic", () => {
    expect(layoutFlow(createOrder, options)).toEqual(layoutFlow(createOrder, options));
  });

  it("draws every node once, without overlaps, and every route of the model", () => {
    const layout = layoutFlow(createOrder, options);
    expect(layout.nodes.map((placed) => placed.id).sort()).toEqual(createOrder.nodes.map((n) => String(n.id)).sort());
    expect(overlaps(layout)).toBe(false);
    const drawn = new Set(layout.edges.map((edge) => `${edge.from}>${edge.to}>${edge.kind}`));
    for (const route of createOrder.routes) {
      expect(drawn.has(`${route.from}>${route.to}>${route.kind}`)).toBe(true);
    }
    expect(layout.width).toBeLessThanOrEqual(options.maxWidth);
  });

  it("puts no in the condition's lane and yes to its right, with yes/no labels", () => {
    const layout = layoutFlow(createOrder, options);
    const at = (id: number) => layout.nodes.find((placed) => placed.id === String(id));
    const condition = at(2);
    const yes = at(3);
    const no = at(6);
    expect(yes && condition && yes.x > condition.x).toBe(true);
    expect(no?.x).toBe(condition?.x);
    expect(layout.edges.find((edge) => edge.kind === "yes")?.label).toBe("yes");
    expect(layout.edges.find((edge) => edge.kind === "no")?.label).toBe("no");
  });

  it("uses authored labels and marks fallback names", () => {
    const key = createOrder.nodes[2]?.key ?? "";
    const layout = layoutFlow(createOrder, { ...options, labels: { [key]: "Did the client ask for an async reply?" } });
    const condition = layout.nodes.find((placed) => placed.id === "2");
    expect(condition?.label).toBe("Did the client ask for an async reply?");
    expect(condition?.fallback).toBe(false);
    expect(layout.nodes.find((placed) => placed.id === "1")?.fallback).toBe(true);
  });

  it("folds a chain of three or more checks into one summary until opened", () => {
    const folded = layoutFlow(chain, options);
    const ids = folded.nodes.map((placed) => placed.id);
    expect(ids).toContain("chain:1");
    expect(ids).not.toContain("3");
    expect(ids).not.toContain("4");
    expect(folded.nodes.find((placed) => placed.id === "1")?.chainFolded).toBe(true);
    expect(folded.nodes.find((placed) => placed.id === "chain:1")?.label).toBe("Throws (one per check)");
    expect(folded.edges.map((edge) => edge.id).sort()).toEqual(["0>1>sequence", "1>7>no", "1>chain:1>yes", "7>8>sequence"]);

    const opened = layoutFlow(chain, { ...options, openChains: new Set([chain.nodes[1]?.key ?? ""]) });
    expect(opened.nodes.map((placed) => placed.id)).toContain("5");
    expect(opened.nodes.find((placed) => placed.id === "1")?.chainFolded).toBe(false);
    expect(overlaps(opened)).toBe(false);
  });

  it("narrows lanes to fit the card, and wraps long labels", () => {
    const wide = layoutFlow(chain, { ...options, openChains: new Set([chain.nodes[1]?.key ?? ""]), maxWidth: 400 });
    expect(wide.laneWidth).toBeLessThan(232);
    expect(wrapLines("one two three four", 50, "x", measure)).toBeGreaterThan(1);
    expect(roundedPath([{ x: 0, y: 0 }, { x: 0, y: 10 }, { x: 10, y: 10 }])).toBe("M0 0 L0 5 Q0 10 5 10 L10 10");
  });
});
