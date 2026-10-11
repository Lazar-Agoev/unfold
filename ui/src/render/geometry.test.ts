// @vitest-environment jsdom
import type { ReactFlowState } from "@xyflow/react";
import { describe, expect, it } from "vitest";
import type { CardView, FlowNode } from "../ipc/generated";
import { page } from "../test/workspace";
import { drawnGeometry, drawnNodeIds, unlabeledSteps } from "./geometry";

type State = Pick<ReactFlowState, "nodeLookup" | "edges">;

function node(id: string, x: number, y: number, size: { width?: number; height?: number }, hidden = false) {
  return [id, { id, hidden, measured: size, internals: { positionAbsolute: { x, y } } }] as const;
}

function state(nodes: ReturnType<typeof node>[], edges: { id: string; source: string; target: string }[]): State {
  return { nodeLookup: new Map(nodes), edges } as unknown as State;
}

function canvas(html: string): HTMLElement {
  const element = document.createElement("div");
  element.innerHTML = html;
  return element;
}

describe("drawnGeometry", () => {
  it("reads each drawn node's box, and each Flow card's steps as drawn", () => {
    const container = canvas(`
      <div class="react-flow__node" data-id="flow"><div class="flow-view">
        <div class="flow-pill kind-start" data-flow-node="s"></div>
        <div class="flow-node" data-flow-node="1"><span class="flow-label fallback">x</span></div>
        <div class="flow-node" data-flow-node="2"><span class="flow-label">Check it</span></div>
        <div class="flow-node kind-group" data-flow-node="3"><span class="flow-label fallback">y</span></div>
        <div class="flow-pill kind-end" data-flow-node="e"></div>
      </div></div>
      <div class="react-flow__node" data-id="code"><div class="code-view"></div></div>`);
    const geometry = drawnGeometry(
      container,
      state([node("flow", 10, 20, { width: 300, height: 200 }), node("code", 400, 0, { width: 200, height: 100 }), node("folded", 0, 0, { width: 100, height: 40 }, true), node("pending", 0, 0, {})], []),
      (id) => (id === "flow" ? 5 : 0),
    );
    expect([...geometry.boxes]).toEqual([
      ["flow", { x: 10, y: 20, width: 300, height: 200 }],
      ["code", { x: 400, y: 0, width: 200, height: 100 }],
    ]);
    expect([...geometry.flows]).toEqual([["flow", { steps: 3, unlabeled: 5 }]]);
    expect(drawnNodeIds(container)).toEqual(new Set(["flow", "code"]));
  });

  it("samples each line's path, and reads none where paths can't be measured", () => {
    const container = canvas(`<svg><g class="react-flow__edge" data-id="arrow"><path d="M0 0 L100 0"></path></g><g class="react-flow__edge" data-id="link"><path></path></g></svg>`);
    const path = container.querySelector<SVGPathElement>('[data-id="arrow"] path');
    Object.assign(path ?? {}, { getTotalLength: () => 24, getPointAtLength: (at: number) => ({ x: at, y: 0 }) });
    const geometry = drawnGeometry(
      container,
      state([], [
        { id: "arrow", source: "a", target: "b" },
        { id: "link", source: "b", target: "c" },
      ]),
      () => 0,
    );
    expect(geometry.lines).toEqual([
      { id: "arrow", from: "a", to: "b", points: [{ x: 0, y: 0 }, { x: 12, y: 0 }, { x: 24, y: 0 }] },
      { id: "link", from: "b", to: "c", points: [] },
    ]);
  });
});

describe("unlabeledSteps", () => {
  const KEY = "src/a.ts#function:alpha";
  const step = (id: number, kind: FlowNode["kind"], key?: string): FlowNode => ({ id, kind, label: "x", fallback: true, lane: 0, ...(key !== undefined ? { key } : {}) });
  const model = (source: "current" | "changed"): CardView => ({
    type: "block",
    header: { key: KEY, kind: "function", name: "alpha", qualifiedName: "alpha", path: "src/a.ts", startLine: 1, endLine: 9, language: "typescript", fileState: "current", source: "current", owner: "src/a.ts#" },
    code: null,
    flow: {
      key: KEY,
      path: "src/a.ts",
      language: "typescript",
      kind: "function",
      source,
      nodes: [step(0, "start", KEY), step(1, "statement", `${KEY}/call[0]`), step(2, "condition", `${KEY}/if[0]`), step(3, "statement"), step(4, "end", KEY)],
      routes: [],
      tree: [],
      start: 0,
      lanes: 1,
      warnings: [],
    },
  });

  it("counts the keyed steps the page doesn't label, as the page's outline does", () => {
    const card = { id: "alpha", ref: { block: KEY }, at: [0, 0] as [number, number] };
    const labeled = page("p", [card], { labels: [{ key: `${KEY}/if[0]`, text: "Is it paid?", author: "person:ana", signature: "" }] });
    expect(unlabeledSteps(page("p", [card]), card, model("current"))).toBe(2);
    expect(unlabeledSteps(labeled, card, model("current"))).toBe(1);
    expect(unlabeledSteps(labeled, card, model("changed"))).toBe(0);
    expect(unlabeledSteps(labeled, card, undefined)).toBe(0);
  });

  it("leaves out the steps a collapsed step group of the card hides, and counts a return", () => {
    const card = { id: "alpha", ref: { block: KEY }, at: [0, 0] as [number, number] };
    const flow = model("current");
    if (flow.type === "block" && flow.flow) {
      flow.flow.nodes.push(step(5, "statement", `${KEY}/if[0]/call[0]`), step(6, "return", `${KEY}/return[0]`));
    }
    const group = (collapsed: boolean, owner = "alpha") => ({ id: "g", card: owner, title: "Check it", members: [`${KEY}/if[0]`], collapsed });
    expect(unlabeledSteps(page("p", [card], { groups: [group(false)] }), card, flow)).toBe(4);
    expect(unlabeledSteps(page("p", [card], { groups: [group(true)] }), card, flow)).toBe(2);
    expect(unlabeledSteps(page("p", [card], { groups: [group(true, "other")] }), card, flow)).toBe(4);
  });
});
