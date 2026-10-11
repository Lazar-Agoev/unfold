import { describe, expect, it } from "vitest";
import type { CallLink, CardView } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";
import { computeArrows, portsByCard } from "./arrows";

function link(source: string, target: string | undefined, kind: CallLink["kind"] = "calls", line = 1): CallLink {
  return {
    source,
    owner: "src/a.ts#function:a",
    kind,
    resolution: target ? "resolved" : "unresolved",
    ...(target ? { target } : { reason: "dynamic" as const }),
    text: source,
    line,
  };
}

function block(key: string, links: CallLink[]): CardView {
  return {
    type: "block",
    header: { key, kind: "function", name: key, qualifiedName: key, path: "src/a.ts", startLine: 1, endLine: 9, language: "typescript", fileState: "current", source: "current", owner: "src/a.ts#" },
    code: {
      key,
      path: "src/a.ts",
      language: "typescript",
      kind: "function",
      startLine: 1,
      endLine: 9,
      source: { state: "current", text: "", dedent: 0, lines: [], spans: [], outline: [], links },
    },
    flow: null,
  };
}

const A = "src/a.ts#function:a";
const B = "src/b.ts#function:b";
const C = "src/c.ts#function:c";
const P = "p-orders0000";
const Q = "p-payments00";
/** A model map keyed as the canvas keys it: by project and key. */
const inP = (entries: readonly (readonly [string, CardView])[]) => new Map(entries.map(([key, view]) => [cardRefKey(P, key), view]));

describe("arrows", () => {
  it("draws one arrow per opened occurrence, even when several calls go to one card", () => {
    const models = inP([
      [A, block(A, [link(`${A}/call[0]`, B, "calls", 2), link(`${A}/call[1]`, B, "calls", 5), link(`${A}/call[2]`, C, "imports", 1), link(`${A}/call[3]`, undefined), link(`${A}/call[4]`, A)])],
      [B, block(B, [])],
    ]);
    const cards = [
      { id: "a", key: A, project: P, entrance: false },
      { id: "b", key: B, project: P, entrance: false },
    ];
    const arrows = computeArrows(cards, models);
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target])).toEqual([
      [`${A}/call[0]`, "b"],
      [`${A}/call[1]`, "b"],
    ]);
    expect(portsByCard(arrows).get("a")).toEqual([`${A}/call[0]`, `${A}/call[1]`]);
    // A Code link is its occurrence's own edge; the arrow names it for its label and contract.
    expect(arrows.map((arrow) => [arrow.project, arrow.seq])).toEqual([
      [P, 0],
      [P, 0],
    ]);

    const withC = computeArrows([...cards, { id: "c", key: C, project: P, entrance: false }], models);
    const imported = withC.find((arrow) => arrow.target === "c");
    expect(imported?.dashed).toBe(true);
    expect(withC.filter((arrow) => arrow.target === "b").every((arrow) => !arrow.dashed)).toBe(true);
  });

  it("points an entrance at each handler on the board, from its target row", () => {
    const entrance: CardView = {
      type: "entrance",
      header: { key: "entrance:e", kind: "http", title: "POST /orders", label: null, method: "POST", trigger: "/orders", triggerStatus: "exact", detector: "express.route", language: "typescript" },
      evidence: { path: "src/server.ts", startLine: 1, endLine: 1, pointer: null, source: "current", code: null, text: null },
      targets: [
        { position: 0, role: "handler", text: "a", resolution: "resolved", target: A, name: "a", path: "src/a.ts", line: 1, note: null, reason: null, module: null },
        { position: 1, role: "handler", text: "c", resolution: "resolved", target: C, name: "c", path: "src/c.ts", line: 1, note: null, reason: null, module: null },
      ],
      applications: [],
    };
    const arrows = computeArrows(
      [
        { id: "e", key: "entrance:e", project: P, entrance: true },
        { id: "a", key: A, project: P, entrance: false },
      ],
      inP([["entrance:e", entrance]]),
    );
    expect(arrows).toEqual([expect.objectContaining({ source: "e", sourceHandle: "target:0", target: "a", kind: "handler", dashed: false })]);
  });

  it("joins only cards of the same project, and none to a card that isn't connected", () => {
    const models = inP([[A, block(A, [link(`${A}/call[0]`, B, "calls", 2)])]]);
    const cards = [
      { id: "a", key: A, project: P, entrance: false },
      // The same key in another project, and a card whose project isn't connected.
      { id: "b-elsewhere", key: B, project: Q, entrance: false },
      { id: "b-gone", key: B, project: null, entrance: false },
    ];
    expect(computeArrows(cards, models)).toEqual([]);
    const arrows = computeArrows([...cards, { id: "b", key: B, project: P, entrance: false }], models);
    expect(arrows.map((arrow) => arrow.target)).toEqual(["b"]);
  });
});
