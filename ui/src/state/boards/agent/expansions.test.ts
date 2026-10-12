import { describe, expect, it } from "vitest";
import { planInsets, type InsetPlan } from "../../../flow/insets";
import type { CardView, FlowModel, FlowNode, Key } from "../../../ipc/generated";
import { MAX_EXPANSION_DEPTH } from "../document";
import { checkCallPath } from "./expansions";

// f() { once(() => f()); g(); }, where g calls h, and h calls i.
const F = "src/a.ts#function:f";
const ONCE = "src/a.ts#function:once";
const G = "src/a.ts#function:g";
const H = "src/a.ts#function:h";
const I = "src/a.ts#function:i";
const ONCE_STEP = `${F}/expression_statement[0]`;
const CALLBACK = `${ONCE_STEP}/call[0]/arrow_function[0]`;
const AGAIN = `${CALLBACK}/call[0]`;
const G_CALL = `${F}/expression_statement[1]/call[0]`;
const H_CALL = `${G}/expression_statement[0]/call[0]`;
const I_CALL = `${H}/expression_statement[0]/call[0]`;

function step(key: Key, source: Key, name: string, target: Key, extra: Partial<FlowNode> = {}): FlowNode {
  return { id: 0, key, kind: "call", label: name, lane: 0, chips: [{ source, seq: 0, kind: "calls", resolution: "resolved", target, name, line: 1 }], call: 0, ...extra };
}

function view(key: Key, steps: readonly FlowNode[]): CardView {
  const nodes: FlowNode[] = [
    { id: 0, key, kind: "start", label: key, lane: 0 },
    ...steps.map((node, index) => ({ ...node, id: index + 1 })),
    { id: steps.length + 1, kind: "end", label: "End", lane: 0 },
  ];
  const flow: FlowModel = { key, path: "src/a.ts", language: "typescript", kind: "function", source: "current", nodes, routes: [], tree: [], start: 0, end: steps.length + 1, lanes: 1, warnings: [] };
  const name = key.replace(/^.*:/, "");
  return {
    type: "block",
    header: { key, kind: "function", name, qualifiedName: name, path: "src/a.ts", startLine: 1, endLine: 9, language: "typescript", fileState: "current", source: "current", owner: "src/a.ts#" },
    code: null,
    flow,
  };
}

const MODELS: Readonly<Record<Key, CardView>> = {
  [F]: view(F, [
    step(ONCE_STEP, `${ONCE_STEP}/call[0]`, "once", ONCE, { callbacks: [{ key: CALLBACK, label: "callback to once", lines: 1, throws: false }] }),
    step(`${F}/expression_statement[1]`, G_CALL, "g", G),
  ]),
  // `() => f()`: a one-call callback whose callee is the card's own block.
  [CALLBACK]: view(CALLBACK, [step(`${CALLBACK}/expression[0]`, AGAIN, "f", F)]),
  [ONCE]: view(ONCE, []),
  [G]: view(G, [step(`${G}/expression_statement[0]`, H_CALL, "h", H)]),
  [H]: view(H, [step(`${H}/expression_statement[0]`, I_CALL, "i", I)]),
  [I]: view(I, []),
};

/** The canvas's plan for one path: each level's owner, innermost last; empty when it is left out. */
function canvasOwners(path: readonly Key[]): Key[] {
  const flow = MODELS[F]?.type === "block" ? MODELS[F].flow : null;
  if (!flow) {
    throw new Error("f has a Flow");
  }
  const prefixes = path.map((_, index) => path.slice(0, index + 1));
  const owners: Key[] = [];
  let level: readonly InsetPlan[] = planInsets(flow, F, prefixes, (key) => {
    const model = MODELS[key];
    return model ? { state: "ready", view: model } : { state: "loading" };
  }, MAX_EXPANSION_DEPTH);
  for (const key of path) {
    const plan = level.find((item) => item.path.at(-1) === key);
    if (!plan) {
      return [];
    }
    owners.push(plan.owner);
    level = plan.children;
  }
  return owners;
}

const mcp = (path: readonly Key[]) => checkCallPath(F, path, (key) => MODELS[key] ?? null);

describe("a callback whose one call goes back to an ancestor", () => {
  it("expands to its own Flow on the canvas and through MCP alike", () => {
    expect(canvasOwners([CALLBACK])).toEqual([CALLBACK]);
    expect(mcp([CALLBACK])).toEqual({ ok: true, blocks: [CALLBACK] });
  });

  it("leaves the repeated ancestor unexpanded in both", () => {
    expect(canvasOwners([CALLBACK, AGAIN])).toEqual([]);
    const check = mcp([CALLBACK, AGAIN]);
    expect(check.ok).toBe(false);
    expect(check.ok === false && check.reason).toContain("which already shows above it (a repeated ancestor)");
  });

  it("still expands at most three levels", () => {
    expect(canvasOwners([G_CALL, H_CALL, I_CALL])).toEqual([G, H, I]);
    expect(mcp([G_CALL, H_CALL, I_CALL])).toEqual({ ok: true, blocks: [G, H, I] });
    const deeper = mcp([G_CALL, H_CALL, I_CALL, `${I}/call[0]`]);
    expect(deeper.ok === false && deeper.reason).toContain("a call path holds 1 to 3 keys");
  });
});
