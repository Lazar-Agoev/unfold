// @vitest-environment jsdom
import type { ReactFlowState } from "@xyflow/react";
import { describe, expect, it } from "vitest";
import { drawnGeometry, drawnNodeIds } from "./geometry";

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
  it("reads each drawn node's box, leaving out hidden and unmeasured nodes", () => {
    const container = canvas(`<div class="react-flow__node" data-id="flow"></div><div class="react-flow__node" data-id="code"></div>`);
    const geometry = drawnGeometry(
      container,
      state([node("flow", 10, 20, { width: 300, height: 200 }), node("code", 400, 0, { width: 200, height: 100 }), node("hidden", 0, 0, { width: 100, height: 40 }, true), node("pending", 0, 0, {})], []),
    );
    expect([...geometry.boxes]).toEqual([
      ["flow", { x: 10, y: 20, width: 300, height: 200 }],
      ["code", { x: 400, y: 0, width: 200, height: 100 }],
    ]);
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
    );
    expect(geometry.lines).toEqual([
      { id: "arrow", from: "a", to: "b", points: [{ x: 0, y: 0 }, { x: 12, y: 0 }, { x: 24, y: 0 }] },
      { id: "link", from: "b", to: "c", points: [] },
    ]);
  });
});
