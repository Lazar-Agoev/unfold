import { describe, expect, it } from "vitest";
import type { CardView, FlowChip, FlowModel, FlowNode, Key } from "../ipc/generated";
import { INSET_CHROME, expandableCalls, expansionState, insetRoom, layoutInsets, oneCallTarget, planInsets, type InsetModel } from "./insets";
import { DIAGRAM_PAD, INSET_GAP, layoutFlow, type Measure } from "./layout";

const measure: Measure = (text) => text.length * 7;
const MAX_DEPTH = 3;

const WORKER = "src/queues/email-worker.ts#function:worker";
const RETRY = "src/lib/retry.ts#function:withRetry";
const SEND = "src/jobs/send-email.ts#function:sendEmail";
const LOG = "src/lib/log.ts#function:log";
const ONE = `${WORKER}/arrow_function[0]`;
const TWO = `${WORKER}/arrow_function[1]`;

function chip(source: Key, target: Key | undefined, name: string, resolution: FlowChip["resolution"] = "resolved"): FlowChip {
  return { source, seq: 0, kind: "calls", resolution, ...(target !== undefined ? { target } : {}), name, line: 1 };
}

function call(id: number, owner: Key, target: Key | undefined, name: string, extra: Partial<FlowNode> = {}): FlowNode {
  const key = `${owner}/n${id}`;
  return { id, kind: "call", label: name, lane: 0, key, chips: [chip(`${key}/call[0]`, target, name, target ? "resolved" : "external")], call: 0, ...extra };
}

/** A straight Flow: Start, `steps` one after another, End. */
function straight(key: Key, steps: readonly FlowNode[]): FlowModel {
  const start: FlowNode = { id: 0, kind: "start", label: key.replace(/^.*:/, ""), lane: 0, key };
  const end: FlowNode = { id: steps.length + 1, kind: "end", label: "End", lane: 0 };
  const nodes = [start, ...steps.map((step, index) => ({ ...step, id: index + 1 })), end];
  return {
    key,
    path: key.replace(/#.*$/, ""),
    language: "typescript",
    kind: "function",
    source: "current",
    nodes,
    routes: nodes.slice(1).map((node, index) => ({ from: index, to: node.id, kind: "sequence" as const })),
    tree: steps.map((_, index) => ({ type: "node" as const, node: index + 1, ends: false, lanes: 1 })),
    start: 0,
    end: end.id,
    lanes: 1,
    warnings: [],
  };
}

function view(flow: FlowModel): CardView {
  const name = flow.key.replace(/^.*:/, "");
  return {
    type: "block",
    header: { key: flow.key, kind: "function", name, qualifiedName: name, path: flow.path, startLine: 6, endLine: 20, language: "typescript", fileState: "current", source: "current", owner: `${flow.path}#` },
    code: null,
    flow,
  };
}

const retry = call(1, WORKER, RETRY, "withRetry", {
  callbacks: [
    { key: ONE, label: "callback to withRetry", lines: 1, throws: false },
    { key: TWO, label: "callback to withRetry", lines: 3, throws: false },
  ],
});
const worker = straight(WORKER, [retry, call(2, WORKER, SEND, "sendEmail"), call(3, WORKER, SEND, "sendEmail"), call(4, WORKER, undefined, "queue.add"), call(5, WORKER, WORKER, "worker")]);
const sendEmail = straight(SEND, [call(1, SEND, LOG, "log"), call(2, SEND, WORKER, "worker")]);
const log = straight(LOG, [call(1, LOG, undefined, "console.log")]);
const oneCall = straight(ONE, [call(1, ONE, SEND, "sendEmail")]);
const twoSteps = straight(TWO, [call(1, TWO, SEND, "sendEmail"), call(2, TWO, LOG, "log")]);
const MODELS: Readonly<Record<Key, FlowModel>> = { [SEND]: sendEmail, [LOG]: log, [ONE]: oneCall, [TWO]: twoSteps, [RETRY]: straight(RETRY, []) };

const lookup = (key: Key): InsetModel => {
  const flow = MODELS[key];
  return flow ? { state: "ready", view: view(flow) } : { state: "loading" };
};
const occurrence = (flow: FlowModel, id: number) => flow.nodes[id]?.chips?.[0]?.source ?? "";

/** Node `id` of `flow` (the tests' models have it). */
function nodeOf(flow: FlowModel, id: number): FlowNode {
  const found = flow.nodes[id];
  if (!found) {
    throw new Error(`no node ${String(id)}`);
  }
  return found;
}

describe("calls that expand in place", () => {
  it("are a node's resolved main call and each callback; external and unresolved calls and conditions have none", () => {
    expect(expandableCalls(retry).map((item) => [item.key, item.target, item.name])).toEqual([
      [occurrence(worker, 1), RETRY, "withRetry"],
      [ONE, ONE, "callback to withRetry"],
      [TWO, TWO, "callback to withRetry"],
    ]);
    expect(expandableCalls(call(4, WORKER, undefined, "queue.add"))).toEqual([]);
    expect(expandableCalls({ ...call(6, WORKER, undefined, "x"), chips: [chip("u", undefined, "x", "unresolved")] })).toEqual([]);
    expect(expandableCalls({ ...call(7, WORKER, SEND, "sendEmail"), kind: "condition" })).toEqual([]);
  });

  it("have a chevron below the deepest level, except a call back to the card or an inset above", () => {
    const [send] = expandableCalls(nodeOf(worker, 2));
    const recursive = expandableCalls(nodeOf(worker, 5))[0];
    expect(send && expansionState(send, [], [WORKER], [], MAX_DEPTH)).toBe("collapsed");
    expect(send && expansionState(send, [], [WORKER], [[send.key]], MAX_DEPTH)).toBe("expanded");
    // The other call to sendEmail is its own occurrence.
    const [other] = expandableCalls(nodeOf(worker, 3));
    expect(other && expansionState(other, [], [WORKER], [[send?.key ?? ""]], MAX_DEPTH)).toBe("collapsed");
    expect(recursive && expansionState(recursive, [], [WORKER], [], MAX_DEPTH)).toBeNull();
    expect(send && expansionState(send, ["a", "b"], [WORKER, LOG, RETRY], [], MAX_DEPTH)).toBe("collapsed");
    expect(send && expansionState(send, ["a", "b", "c"], [WORKER, LOG, RETRY, LOG], [], MAX_DEPTH)).toBeNull();
    expect(send && expansionState(send, ["a"], [WORKER, SEND], [], MAX_DEPTH)).toBeNull();
  });

  it("know a one-call callback by its one step, a call resolved to a target", () => {
    expect(oneCallTarget(oneCall)?.target).toBe(SEND);
    expect(oneCallTarget(twoSteps)).toBeUndefined();
    expect(oneCallTarget(log)).toBeUndefined();
    expect(oneCallTarget({ ...oneCall, source: "changed" })).toBeUndefined();
    // An expression-bodied callback: one statement step whose one call resolved.
    const statement = (chips: FlowChip[]): FlowNode => {
      const { call: _main, ...rest } = call(1, ONE, SEND, "sendEmail");
      return { ...rest, kind: "statement", chips };
    };
    const expression = straight(ONE, [statement([chip("a", SEND, "sendEmail")])]);
    expect(oneCallTarget(expression)?.target).toBe(SEND);
    const nested = straight(ONE, [statement([chip("a", SEND, "sendEmail"), chip("b", LOG, "log")])]);
    expect(oneCallTarget(nested)).toBeUndefined();
  });
});

describe("planning the insets", () => {
  it("shows a callee's Flow, each occurrence apart, and leaves out paths whose call is gone", () => {
    const paths = [[occurrence(worker, 3)], [occurrence(worker, 2)], ["src/gone.ts#function:gone/call[0]"], [occurrence(worker, 4)]];
    const plans = planInsets(worker, WORKER, paths, lookup, MAX_DEPTH);
    expect(plans.map((plan) => [plan.path, plan.node, plan.owner])).toEqual([
      [[occurrence(worker, 2)], worker.nodes[2]?.key, SEND],
      [[occurrence(worker, 3)], worker.nodes[3]?.key, SEND],
    ]);
    expect(plans[0]?.lineage).toEqual([WORKER, SEND]);
  });

  it("shows a one-call callback as its proven callee, via the callback, and any other callback as itself", () => {
    const plans = planInsets(worker, WORKER, [[ONE], [TWO]], lookup, MAX_DEPTH);
    expect(plans.map((plan) => [plan.path, plan.owner, plan.name, plan.via])).toEqual([
      [[ONE], SEND, "sendEmail", "callback to withRetry"],
      [[TWO], TWO, "callback to withRetry", undefined],
    ]);
    // Both hang below the retry step, which stays as it is.
    expect(plans.map((plan) => plan.node)).toEqual([retry.key, retry.key]);
  });

  it("shows a one-call callback as itself when its callee is already shown above", () => {
    const callback = straight(ONE, [call(1, ONE, WORKER, "worker")]);
    const plans = planInsets(worker, WORKER, [[ONE]], (key) => (key === ONE ? { state: "ready", view: view(callback) } : lookup(key)), MAX_DEPTH);
    expect(plans.map((plan) => [plan.owner, plan.via])).toEqual([[ONE, undefined]]);
  });

  it("nests up to three levels, and waits for a model before planning inside it", () => {
    const send = occurrence(worker, 2);
    const logCall = occurrence(sendEmail, 1);
    const deep = [[send], [send, logCall], [send, logCall, occurrence(log, 1)], [send, logCall, "x", "y"]];
    const [plan] = planInsets(worker, WORKER, deep, lookup, MAX_DEPTH);
    expect(plan?.children.map((child) => [child.path, child.owner, child.lineage])).toEqual([[[send, logCall], LOG, [WORKER, SEND, LOG]]]);
    // log's only call is external: nothing at the third level.
    expect(plan?.children[0]?.children).toEqual([]);
    const asked: Key[] = [];
    const [waiting] = planInsets(worker, WORKER, deep, (key) => {
      asked.push(key);
      return { state: "loading" };
    }, MAX_DEPTH);
    expect(waiting?.model).toEqual({ state: "loading" });
    expect(waiting?.children).toEqual([]);
    expect(asked).toEqual([SEND]);
  });
});

describe("laying out the insets", () => {
  const options = () => ({ measure });

  it("size each inset from its title, caption, and own bare Flow, or a note while there is none", () => {
    const [one, send, again] = layoutInsets(planInsets(worker, WORKER, [[occurrence(worker, 2)], [ONE], [occurrence(worker, 3)]], lookup, MAX_DEPTH), options, 846);
    const bare = layoutFlow(sendEmail, { measure, maxWidth: 846 - INSET_CHROME.rule, bare: true });
    expect(bare.nodes.map((placed) => placed.node.kind)).toEqual(["call", "call"]);
    expect(send?.layout).toEqual(bare);
    expect(send?.height).toBe(INSET_CHROME.title + bare.height);
    expect(send?.width).toBe(Math.max(INSET_CHROME.minWidth, INSET_CHROME.rule + bare.width));
    expect(one?.height).toBe(INSET_CHROME.title + INSET_CHROME.caption + bare.height);
    // The second call to sendEmail has its own inset, under its own node.
    expect(again?.plan.node).toBe(worker.nodes[3]?.key);
    expect(again?.height).toBe(send?.height);
    const [waiting] = layoutInsets(planInsets(worker, WORKER, [[occurrence(worker, 2)]], () => ({ state: "loading" }), MAX_DEPTH), options, 846);
    expect(waiting?.layout).toBeNull();
    expect(waiting?.height).toBe(INSET_CHROME.title + INSET_CHROME.note);
  });

  it("add up from the inside out: an inset keeps room for the ones inside it", () => {
    const send = occurrence(worker, 2);
    const [outer] = layoutInsets(planInsets(worker, WORKER, [[send], [send, occurrence(sendEmail, 1)]], lookup, MAX_DEPTH), options, 846);
    const inner = outer?.children[0];
    expect(inner?.plan.owner).toBe(LOG);
    // The inner inset fits the outer one's content, inside its diagram's padding.
    expect(inner?.width).toBeLessThanOrEqual(846 - INSET_CHROME.rule - DIAGRAM_PAD * 2);
    const plain = layoutFlow(sendEmail, { measure, maxWidth: 846 - INSET_CHROME.rule, bare: true });
    expect(outer?.layout?.height).toBe(plain.height + INSET_GAP + (inner?.height ?? 0));
    expect(outer?.height).toBe(INSET_CHROME.title + (outer?.layout?.height ?? 0));
    expect(outer?.layout?.insets).toEqual([expect.objectContaining({ key: sendEmail.nodes[1]?.key, width: inner?.width, height: inner?.height })]);
  });

  it("stack two insets under one node", () => {
    const room = insetRoom(layoutInsets(planInsets(worker, WORKER, [[ONE], [TWO]], lookup, MAX_DEPTH), options, 846));
    const laid = layoutInsets(planInsets(worker, WORKER, [[ONE], [TWO]], lookup, MAX_DEPTH), options, 846);
    expect([...room.keys()]).toEqual([retry.key]);
    expect(room.get(retry.key ?? "")).toEqual({
      width: Math.max(...laid.map((inset) => inset.width)),
      height: (laid[0]?.height ?? 0) + INSET_GAP + (laid[1]?.height ?? 0),
    });
  });
});
