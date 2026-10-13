import type { GroupStepsOp } from "@unfold/desktop/agent-api";
import { beforeEach, describe, expect, it } from "vitest";
import { cardQuery } from "../canvas/cardData";
import { queryClient } from "../ipc/client";
import type { CardView, FlowNode, Group, Key, OutlineItem, Page } from "../ipc/generated";
import { AgentData } from "../state/boards/agent/data";
import { applyGroupSteps } from "../state/boards/agent/groups";
import { projectIndex } from "../state/workspace";
import { fixture } from "../test/engine";
import { page, projectRow, workspaceView } from "../test/workspace";
import { combineChoice, stepRun } from "./combine";
import type { SourceContext } from "./viewCode";

type BlockView = Extract<CardView, { type: "block" }>;

const P = "p-orders0000";
const CREATE = "src/routes/orders.ts#function:createOrder";
const IF = `${CREATE}/if[0]`;
const ORDER = `${CREATE}/variable:order`;
const SLACK = `${CREATE}/expression_statement[0]`;
const REPLY = `${CREATE}/expression_statement[1]`;
const JOB = `${IF}/variable:job`;
const createOrder = fixture("card", "card_create_order") as BlockView;

/**
 * `f`: a statement, an if/else, a nested function, and a last statement.
 *
 *     1 function f() {
 *     2   a();
 *     3   if (x) {
 *     4     b();
 *     5   } else {
 *     6     c();
 *     7   }
 *     8   function g() {
 *    10   }
 *    11   d();
 *    12 }
 */
const F = "src/f.ts#function:f";
const A = `${F}/a`;
const IF_X = `${F}/if[0]`;
const B = `${IF_X}/b`;
const C = `${IF_X}/c`;
const G = `${F}/g`;
const D = `${F}/d`;
const fn: BlockView = (() => {
  const item = (key: Key, kind: OutlineItem["kind"], role: OutlineItem["role"], startLine: number, endLine: number, extra: Partial<OutlineItem> = {}): OutlineItem => ({
    key,
    kind,
    ...(role !== undefined ? { role } : {}),
    startLine,
    endLine,
    start: 0,
    end: 1,
    groupable: true,
    ...extra,
  });
  const outline = [
    item(A, "expression_statement", "body", 2, 2),
    item(IF_X, "if", "body", 3, 7),
    item(B, "expression_statement", "then", 4, 4, { parent: 1 }),
    item(C, "expression_statement", "else", 6, 6, { parent: 1 }),
    item(G, "function", "body", 8, 10, { groupable: false }),
    item(D, "expression_statement", "body", 11, 11),
  ];
  const node = (id: number, key: Key | undefined, kind: FlowNode["kind"], startLine?: number, endLine?: number): FlowNode => ({
    id,
    ...(key !== undefined ? { key } : {}),
    kind,
    label: key ?? "End",
    lane: 0,
    ...(startLine !== undefined ? { startLine, endLine: endLine ?? startLine } : {}),
  });
  return {
    type: "block",
    header: { key: F, kind: "function", name: "f", qualifiedName: "f", path: "src/f.ts", startLine: 1, endLine: 12, language: "typescript", fileState: "current", source: "current", owner: "src/f.ts#" },
    code: { key: F, path: "src/f.ts", language: "typescript", kind: "function", startLine: 1, endLine: 12, source: { state: "current", text: "", dedent: 0, lines: [], spans: [], outline, links: [] } },
    flow: {
      key: F,
      path: "src/f.ts",
      language: "typescript",
      kind: "function",
      source: "current",
      nodes: [
        node(0, F, "start", 1, 12),
        node(1, A, "statement", 2),
        node(2, IF_X, "condition", 3, 7),
        node(3, B, "statement", 4),
        node(4, C, "statement", 6),
        node(5, G, "definition", 8, 10),
        node(6, D, "statement", 11),
        node(7, undefined, "end"),
      ],
      routes: [],
      tree: [],
      start: 0,
      end: 7,
      lanes: 1,
      warnings: [],
    },
  };
})();

const MODELS: Readonly<Record<Key, BlockView>> = { [CREATE]: createOrder, [F]: fn };
const context: SourceContext = { project: P, model: (key) => MODELS[key] };

function pageWith(groups: readonly Group[] = []): Page {
  return page(
    "page-1",
    [
      { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
      { id: "f", ref: { block: F }, at: [600, 0] },
    ],
    { groups: [...groups] },
  );
}

describe("a run of steps", () => {
  it("takes the whole statements between its first and last step, and their steps", () => {
    const run = stepRun(createOrder, ORDER, REPLY, []);
    expect(run?.members).toEqual([ORDER, SLACK, REPLY]);
    expect(run?.steps.map((step) => step.key)).toEqual([ORDER, SLACK, REPLY]);
    expect([run?.startLine, run?.endLine]).toEqual([32, 34]);
    // Either order: the last step may come first.
    expect(stepRun(createOrder, REPLY, ORDER, [])?.members).toEqual([ORDER, SLACK, REPLY]);
  });

  it("is never the function's own Start, and a statement with branches counts every step inside it", () => {
    expect(stepRun(fn, F, A, [])).toBeNull();
    const run = stepRun(fn, A, IF_X, []);
    expect(run?.members).toEqual([A, IF_X]);
    expect(run?.steps.map((step) => step.key)).toEqual([A, IF_X, B, C]);
  });

  it("can't be combined from steps shown inside a call expanded in place, or from one step alone", () => {
    expect(combineChoice(pageWith(), { kind: "card", cardId: "create-order", key: ORDER, owner: "src/x.ts#function:x", last: REPLY }, context)).toEqual({
      reason: "These steps belong to the called function. Open it as its own card to combine its steps.",
    });
    expect(combineChoice(pageWith(), { kind: "card", cardId: "create-order", key: ORDER }, context)).toEqual({ reason: "Shift-click the last step of the run to combine it." });
  });
});

describe("Combine and MCP group_steps", () => {
  const index = projectIndex(workspaceView([projectRow(P, "orders")]));
  let data: AgentData;

  beforeEach(async () => {
    queryClient.clear();
    data = new AgentData();
    for (const [key, model] of Object.entries(MODELS)) {
      queryClient.setQueryData(cardQuery(P, key).queryKey, model);
      await data.loadModel(P, key);
    }
  });

  /** group_steps over the lines a run from `first` to `last` covers: the statements it took, or its refusal. */
  function groupSteps(groups: readonly Group[], cardId: string, first: Key, last: Key): { members: Key[] } | { refusal: string } {
    const model = MODELS[cardId === "f" ? F : CREATE];
    const nodes = [first, last].map((key) => model?.flow?.nodes.find((node) => node.key === key));
    const from = Math.min(...nodes.map((node) => node?.startLine ?? 0));
    const to = Math.max(...nodes.map((node) => node?.endLine ?? node?.startLine ?? 0));
    const op: GroupStepsOp = { op: "group_steps", card: cardId, groups: [{ title: "Combined", lines: [from, to] }] };
    const ctx = { board: { project: P, id: "b.json" }, agent: { author: "agent:claude", name: "Claude" } as const, data, index, measured: {}, warnings: [] };
    try {
      const [result] = applyGroupSteps(structuredClone(pageWith(groups)), op, ctx);
      return { members: result?.kind === "group" ? (result.statements ?? []).map((statement) => statement.key) : [] };
    } catch (error) {
      return { refusal: error instanceof Error ? error.message : String(error) };
    }
  }

  const CASES: readonly { readonly name: string; readonly card: string; readonly first: Key; readonly last: Key; readonly groups?: readonly Group[] }[] = [
    { name: "consecutive statements of one body", card: "create-order", first: ORDER, last: REPLY },
    { name: "a run into a branch takes its whole statement", card: "create-order", first: JOB, last: ORDER },
    { name: "a statement and a whole if/else", card: "f", first: A, last: IF_X },
    { name: "two branches of one if", card: "f", first: B, last: C },
    { name: "a nested definition", card: "f", first: A, last: D },
    { name: "an overlap with another group", card: "f", first: A, last: IF_X, groups: [{ id: "first", card: "f", title: "First", members: [A], collapsed: false }] },
    { name: "a group nested inside", card: "f", first: A, last: IF_X, groups: [{ id: "inner", card: "f", title: "Inner", members: [B], collapsed: false }] },
  ];

  it.each(CASES)("decide alike: $name", ({ card, first, last, groups = [] }) => {
    const choice = combineChoice(pageWith(groups), { kind: "card", cardId: card, key: first, last }, context);
    const agent = groupSteps(groups, card, first, last);
    if ("members" in choice) {
      expect(agent).toEqual({ members: choice.members });
    } else {
      expect("refusal" in agent).toBe(true);
      // The same actionable reason, from the same outline rule.
      expect("refusal" in agent ? agent.refusal : "").toContain(choice.reason);
    }
  });

  it("refuses what the gutter refuses, with its reason", () => {
    const reasons = CASES.map(({ card, first, last, groups = [] }) => {
      const choice = combineChoice(pageWith(groups), { kind: "card", cardId: card, key: first, last }, context);
      return "reason" in choice ? choice.reason : "ok";
    });
    expect(reasons).toEqual([
      "ok",
      "ok",
      "ok",
      "A group can't cross a body boundary: these lines are in more than one branch.",
      "A nested definition can't be part of a group.",
      'These lines overlap the group "First". Groups never overlap or nest.',
      'These lines overlap the group "Inner". Groups never overlap or nest.',
    ]);
  });
});
