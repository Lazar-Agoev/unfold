import { describe, expect, it } from "vitest";
import type { CardView, FlowModel, FlowNode } from "../ipc/generated";
import { fixture } from "../test/engine";
import { INSET_GAP, PROPOSAL_INDENT, PROPOSAL_RAIL, UNDRAWN_NOTE, insideNote, layoutFlow, roundedPath, wrapLines, type FlowLayout, type Measure, type PlacedNode, type ProposalSpec, type StepGroup } from "./layout";

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

const FN = "src/routes/orders.ts#function:createOrder";
const IF = `${FN}/if[0]`;

function group(id: string, members: readonly string[], extra: Partial<StepGroup> = {}): StepGroup {
  return { id, title: `Group ${id}`, members, collapsed: false, returns: false, throws: false, ...extra };
}

/** The design's two step groups of `createOrder` (design §4.3): the queued branch and the direct one. */
const queue = group("queue-it", [`${IF}/variable:job`, `${IF}/expression_statement[0]`, `${IF}/return[0]`], {
  title: "Queue it for a worker, reply 202",
  returns: true,
});
const place = group("place-it", [`${FN}/variable:order`, `${FN}/expression_statement[0]`, `${FN}/expression_statement[1]`], {
  title: "Place it now and reply 201",
});
const collapsed = (item: StepGroup): StepGroup => ({ ...item, collapsed: true });

const edgeIds = (layout: FlowLayout) => layout.edges.map((edge) => edge.id).sort();
const placed = (layout: FlowLayout, id: string) => layout.nodes.find((item) => item.id === id);

function regionOf(layout: FlowLayout, id: string) {
  const region = layout.regions.find((item) => item.kind === "group" && item.group === id);
  if (!region) {
    throw new Error(`no region for ${id}`);
  }
  return region;
}

function contains(region: FlowLayout["regions"][number], node: PlacedNode | undefined): boolean {
  return (
    node !== undefined &&
    node.x >= region.x &&
    node.x + node.width <= region.x + region.width &&
    node.y >= region.y &&
    node.y + node.height <= region.y + region.height
  );
}

/** Whether every region and node lies inside the diagram. */
function inBounds(layout: FlowLayout): boolean {
  return [...layout.regions, ...layout.nodes].every((box) => box.x >= 0 && box.y >= 0 && box.x + box.width <= layout.width && box.y + box.height <= layout.height);
}

/** `for (const x of xs) { log(x); if (x) break; } throw new Error();` */
const loop: FlowModel = {
  key: "src/a.ts#function:a",
  path: "src/a.ts",
  language: "typescript",
  kind: "function",
  source: "current",
  nodes: [node(0, "start", { label: "a" }), node(1, "loop"), node(2, "call"), node(3, "condition"), node(4, "break"), node(5, "throw")],
  routes: [
    { from: 0, to: 1, kind: "sequence" },
    { from: 1, to: 2, kind: "each" },
    { from: 2, to: 3, kind: "sequence" },
    { from: 3, to: 4, kind: "yes" },
    { from: 3, to: 1, kind: "next" },
    { from: 4, to: 5, kind: "break" },
    { from: 1, to: 5, kind: "done" },
  ],
  tree: [
    {
      type: "loop",
      node: 1,
      body: [
        { type: "node", node: 2, ends: false, lanes: 1 },
        { type: "branch", node: 3, yes: [{ type: "node", node: 4, ends: true, lanes: 1 }], no: [], yesEnds: true, noEnds: false, ends: false, lanes: 2, yesLane: 1, elseIf: false },
      ],
      testAfter: false,
      ends: false,
      lanes: 2,
    },
    { type: "node", node: 5, ends: true, lanes: 1 },
  ],
  start: 0,
  lanes: 2,
  warnings: [],
};
const loopBody = group("log-and-stop", ["src/a.ts#function:a/n2", "src/a.ts#function:a/n3"]);
const fail = group("fail", ["src/a.ts#function:a/n5"], { throws: true });

describe("Flow layout with step groups", () => {
  it("draws an expanded step group as a titled region around its steps, keeping every step and route", () => {
    const layout = layoutFlow(createOrder, { ...options, groups: [queue, place] });
    expect(layoutFlow(createOrder, { ...options, groups: [queue, place] })).toEqual(layout);
    const queueRegion = regionOf(layout, "queue-it");
    const placeRegion = regionOf(layout, "place-it");
    expect(queueRegion.title).toBe("Queue it for a worker, reply 202");
    for (const id of ["3", "4", "5"]) {
      expect(contains(queueRegion, placed(layout, id))).toBe(true);
    }
    for (const id of ["6", "7", "8"]) {
      expect(contains(placeRegion, placed(layout, id))).toBe(true);
    }
    for (const id of ["1", "2", "9"]) {
      expect(contains(queueRegion, placed(layout, id)) || contains(placeRegion, placed(layout, id))).toBe(false);
    }
    expect(layout.nodes.map((item) => item.id).sort()).toEqual(createOrder.nodes.map((item) => String(item.id)).sort());
    expect(edgeIds(layout)).toEqual(edgeIds(layoutFlow(createOrder, options)));
    expect(overlaps(layout)).toBe(false);
    expect(inBounds(layout)).toBe(true);
    expect(layout.width).toBeLessThanOrEqual(options.maxWidth);
    // The route into a region turns above it, so it crosses the title band straight down.
    const into = layout.edges.find((edge) => edge.id === "2>6>no");
    expect(into?.points.slice(0, -1).every((point) => point.y < placeRegion.y)).toBe(true);
  });

  it("keeps the labels of the steps inside an expanded group", () => {
    const layout = layoutFlow(createOrder, { ...options, groups: [queue], labels: { [`${IF}/variable:job`]: "Queue the order" } });
    expect(placed(layout, "3")?.label).toBe("Queue the order");
    expect(placed(layout, "3")?.fallback).toBe(false);
  });

  it("collapses createOrder's eight steps into four with two collapsed groups and two labels (design §4.3)", () => {
    const labels = { [`${FN}/variable:draft`]: "Read the order draft", [IF]: "Did the client ask for an async reply?" };
    const layout = layoutFlow(createOrder, { ...options, labels, groups: [collapsed(queue), collapsed(place)] });
    expect(layout.nodes.map((item) => item.id)).toEqual(["0", "1", "2", "group:queue-it", "group:place-it", "9"]);
    expect(layout.nodes.filter((item) => item.node.kind !== "start" && item.node.kind !== "end")).toHaveLength(4);
    expect(edgeIds(layout)).toEqual([
      "0>1>sequence",
      "1>2>sequence",
      "2>group:place-it>no",
      "2>group:queue-it>yes",
      "group:place-it>9>sequence",
      "group:queue-it>9>return",
    ]);
    const queued = placed(layout, "group:queue-it");
    const direct = placed(layout, "group:place-it");
    const condition = placed(layout, "2");
    expect(queued?.label).toBe("Queue it for a worker, reply 202");
    expect(queued?.group).toMatchObject({ id: "queue-it", steps: 3, returns: true, throws: false });
    expect(direct?.group).toMatchObject({ id: "place-it", steps: 3, returns: false, throws: false });
    expect(queued && condition && queued.x > condition.x).toBe(true);
    expect(direct?.x).toBe(condition?.x);
    expect(layout.regions).toEqual([]);
    expect(overlaps(layout)).toBe(false);
  });

  it("makes room for a collapsed step's marks", () => {
    const quiet = layoutFlow(createOrder, { ...options, groups: [collapsed({ ...queue, returns: false })] });
    const marked = layoutFlow(createOrder, { ...options, groups: [collapsed(queue)] });
    expect((placed(marked, "group:queue-it")?.height ?? 0) - (placed(quiet, "group:queue-it")?.height ?? 0)).toBe(19);
  });

  it("carries the chips of the steps a collapsed group hides, so their arrows leave from its step", () => {
    const layout = layoutFlow(createOrder, { ...options, groups: [collapsed(place)] });
    const hidden = [6, 7, 8].flatMap((id) => createOrder.nodes[id]?.chips ?? []);
    expect(placed(layout, "group:place-it")?.node.chips).toEqual(hidden);
  });

  it("draws a branch that starts inside a group and leaves it from the group's step", () => {
    const head = group("read-and-ask", [`${FN}/variable:draft`, IF]);
    const open = layoutFlow(createOrder, { ...options, groups: [head] });
    const region = regionOf(open, "read-and-ask");
    for (const id of ["1", "2", "3", "4", "5"]) {
      expect(contains(region, placed(open, id))).toBe(true);
    }
    expect(contains(region, placed(open, "6"))).toBe(false);
    expect(overlaps(open)).toBe(false);
    // The `if` has no else: its `no` leaves the collapsed group as the step's plain next route.
    const shut = layoutFlow(createOrder, { ...options, groups: [collapsed({ ...head, returns: true })] });
    expect(edgeIds(shut)).toEqual([
      "0>group:read-and-ask>sequence",
      "6>7>sequence",
      "7>8>sequence",
      "8>9>sequence",
      "group:read-and-ask>6>sequence",
      "group:read-and-ask>9>return",
    ]);
  });

  it("draws a group around a branch with both lanes inside its region", () => {
    const branch = group("ask-then-place", [IF, `${FN}/variable:order`]);
    const layout = layoutFlow(createOrder, { ...options, groups: [branch] });
    const region = regionOf(layout, "ask-then-place");
    for (const id of ["2", "3", "4", "5", "6"]) {
      expect(contains(region, placed(layout, id))).toBe(true);
    }
    expect(layout.width).toBeLessThanOrEqual(options.maxWidth);
    expect(inBounds(layout)).toBe(true);
    const shut = layoutFlow(createOrder, { ...options, groups: [collapsed(branch)] });
    expect(edgeIds(shut)).toEqual([
      "0>1>sequence",
      "1>group:ask-then-place>sequence",
      "7>8>sequence",
      "8>9>sequence",
      "group:ask-then-place>7>sequence",
      "group:ask-then-place>9>return",
    ]);
  });

  it("draws groups at the start and at the end of a body", () => {
    const first = group("read", [`${FN}/variable:draft`]);
    const layout = layoutFlow(createOrder, { ...options, groups: [first, place] });
    const start = placed(layout, "0");
    const end = placed(layout, "9");
    const top = regionOf(layout, "read");
    const bottom = regionOf(layout, "place-it");
    expect(start && start.y + start.height <= top.y).toBe(true);
    expect(end && bottom.y + bottom.height <= end.y).toBe(true);
    expect(inBounds(layout)).toBe(true);
    const shut = layoutFlow(createOrder, { ...options, groups: [collapsed(first), collapsed(place)] });
    expect(edgeIds(shut)).toContain("0>group:read>sequence");
    expect(edgeIds(shut)).toContain("group:read>2>sequence");
    expect(edgeIds(shut)).toContain("group:place-it>9>sequence");
  });

  it("draws two groups in one body apart and in order, joined by a straight route", () => {
    const order = group("order", [`${FN}/variable:order`]);
    const reply = group("reply", [`${FN}/expression_statement[0]`, `${FN}/expression_statement[1]`]);
    const layout = layoutFlow(createOrder, { ...options, groups: [order, reply] });
    const upper = regionOf(layout, "order");
    const lower = regionOf(layout, "reply");
    expect(upper.y + upper.height).toBeLessThan(lower.y);
    expect(layout.edges.find((edge) => edge.id === "6>7>sequence")?.points).toHaveLength(2);
    const shut = layoutFlow(createOrder, { ...options, groups: [collapsed(order), collapsed(reply)] });
    expect(edgeIds(shut)).toContain("group:order>group:reply>sequence");
    expect(edgeIds(shut)).toContain("group:reply>9>sequence");
    expect(overlaps(shut)).toBe(false);
  });

  it("keeps a break's and a loop's routes out of a collapsed group, and a throw ends its step", () => {
    const shut = layoutFlow(loop, { ...options, groups: [collapsed(loopBody), collapsed(fail)] });
    expect(edgeIds(shut)).toEqual([
      "0>1>sequence",
      "1>group:fail>done",
      "1>group:log-and-stop>each",
      "group:log-and-stop>1>next",
      "group:log-and-stop>group:fail>break",
    ]);
    expect(placed(shut, "group:fail")?.group).toMatchObject({ id: "fail", steps: 1, returns: false, throws: true });
    const open = layoutFlow(loop, { ...options, groups: [loopBody] });
    const repeats = open.regions.find((region) => region.kind === "loop");
    const body = regionOf(open, "log-and-stop");
    expect(repeats && body.x > repeats.x && body.x + body.width < repeats.x + repeats.width).toBe(true);
    expect(overlaps(open)).toBe(false);
  });

  it("doesn't draw a group whose statements aren't one run of the diagram", () => {
    const split = group("split", [`${FN}/variable:draft`, `${FN}/variable:order`]);
    const layout = layoutFlow(createOrder, { ...options, groups: [split, collapsed({ ...split, id: "split-2" })] });
    expect(layout.regions).toEqual([]);
    expect(layout.nodes.map((item) => item.id).sort()).toEqual(createOrder.nodes.map((item) => String(item.id)).sort());
    // A group inside a folded chain is drawn once the chain is opened.
    const hidden = group("first-throw", ["src/a.ts#function:a/n2"]);
    expect(layoutFlow(chain, { ...options, groups: [hidden] }).regions).toEqual([]);
    const opened = layoutFlow(chain, { ...options, groups: [hidden], openChains: new Set([chain.nodes[1]?.key ?? ""]) });
    expect(contains(regionOf(opened, "first-throw"), placed(opened, "2"))).toBe(true);
  });

  it("collapses a group around a folded chain into one step for the whole chain", () => {
    const checks = group("checks", ["src/a.ts#function:a/n1", "src/a.ts#function:a/n7"], { throws: true });
    const layout = layoutFlow(chain, { ...options, groups: [collapsed(checks)] });
    expect(layout.nodes.map((item) => item.id)).toEqual(["0", "group:checks", "8"]);
    expect(edgeIds(layout)).toEqual(["0>group:checks>sequence", "group:checks>8>sequence"]);
    expect(placed(layout, "group:checks")?.group?.steps).toBe(7);
  });
});

describe("Flow layout with calls expanded in place", () => {
  const PLACE = `${FN}/variable:order`;
  const room = (width: number, height: number) => new Map([[PLACE, { width, height }]]);

  it("keeps room below the call's node: the node keeps its box, what follows moves down, and the route leaves below the inset", () => {
    const plain = layoutFlow(createOrder, options);
    const layout = layoutFlow(createOrder, { ...options, insets: room(200, 120) });
    const call = placed(layout, "6");
    expect(call).toEqual(placed(plain, "6"));
    expect(layout.insets).toEqual([{ node: "6", key: PLACE, x: call?.x, y: (call?.y ?? 0) + (call?.height ?? 0) + INSET_GAP, width: 200, height: 120 }]);
    expect(placed(layout, "7")?.y).toBe((placed(plain, "7")?.y ?? 0) + INSET_GAP + 120);
    expect(placed(layout, "9")?.y).toBe((placed(plain, "9")?.y ?? 0) + INSET_GAP + 120);
    expect(layout.height).toBe(plain.height + INSET_GAP + 120);
    const [inset] = layout.insets;
    const next = layout.edges.find((edge) => edge.id === "6>7>sequence");
    expect(next?.points[0]?.y).toBe((inset?.y ?? 0) + (inset?.height ?? 0));
    expect(overlaps(layout)).toBe(false);
    expect(layout.nodes.some((node) => inset && node.y < inset.y + inset.height && node.y + node.height > inset.y && node.x < inset.x + inset.width && node.x + node.width > inset.x)).toBe(false);
    expect(layout.edges.map((edge) => edge.id).sort()).toEqual(plain.edges.map((edge) => edge.id).sort());
  });

  it("widens the diagram to fit a wide inset", () => {
    const plain = layoutFlow(createOrder, options);
    const layout = layoutFlow(createOrder, { ...options, insets: room(700, 60) });
    const [inset] = layout.insets;
    expect(layout.width).toBeGreaterThan(plain.width);
    expect((inset?.x ?? 0) + (inset?.width ?? 0)).toBeLessThanOrEqual(layout.width);
  });

  it("keeps none for a call a collapsed step group hides, and draws a bare Flow without Start and End", () => {
    const layout = layoutFlow(createOrder, { ...options, groups: [collapsed(place)], insets: room(200, 120) });
    expect(layout.insets).toEqual([]);
    expect(layout.height).toBe(layoutFlow(createOrder, { ...options, groups: [collapsed(place)] }).height);
    const bare = layoutFlow(createOrder, { ...options, bare: true });
    expect(bare.nodes.map((node) => node.node.kind)).not.toContain("start");
    expect(bare.nodes.map((node) => node.node.kind)).not.toContain("end");
    expect(bare.edges.some((edge) => edge.from === "0" || edge.to === "9")).toBe(false);
    expect(inBounds(bare)).toBe(true);
  });

  it("keeps room under a loop's header inside its region", () => {
    const header = "src/a.ts#function:a/n1";
    const layout = layoutFlow(loop, { ...options, insets: new Map([[header, { width: 300, height: 90 }]]) });
    const repeats = layout.regions.find((region) => region.kind === "loop");
    const [inset] = layout.insets;
    expect(inset?.node).toBe("1");
    expect(repeats && inset && inset.x + inset.width <= repeats.x + repeats.width && inset.y + inset.height <= repeats.y + repeats.height).toBe(true);
    expect((placed(layout, "2")?.y ?? 0)).toBeGreaterThan((inset?.y ?? 0) + (inset?.height ?? 0));
  });
});

describe("Flow layout with proposed steps", () => {
  const ORDER = `${FN}/variable:order`;
  const REPLY = `${FN}/expression_statement[1]`;
  const plain = layoutFlow(createOrder, options);
  const proposal = (extra: Partial<ProposalSpec> = {}): ProposalSpec => ({ id: "email", after: ORDER, title: "Email a confirmation", ...extra });
  const edge = (layout: FlowLayout, id: string) => layout.edges.find((item) => item.id === id);
  const dashed = (layout: FlowLayout, id: string) => layout.proposalEdges.find((item) => item.id === id)?.points;
  const room = (layout: FlowLayout) => {
    const [first] = layout.proposals;
    if (!first) {
      throw new Error("no proposed step drawn");
    }
    return first;
  };
  /** Whether a proposed step's box overlaps a node's. */
  const clear = (layout: FlowLayout) =>
    layout.proposals.every((item) => !layout.nodes.some((node) => item.x < node.x + node.width && node.x < item.x + item.width && item.y < node.y + node.height && node.y < item.y + item.height));

  it("keeps room after a plain step, indented in its lane, with the compiled route still running beside it", () => {
    const layout = layoutFlow(createOrder, { ...options, proposals: [proposal()] });
    expect(layoutFlow(createOrder, { ...options, proposals: [proposal()] })).toEqual(layout);
    const anchor = placed(layout, "6");
    const next = placed(layout, "7");
    const box = room(layout);
    if (!anchor || !next) {
      throw new Error("missing nodes");
    }
    expect(box).toMatchObject({ id: "email", host: "6", title: "Email a confirmation", x: anchor.x + PROPOSAL_INDENT, y: anchor.y + anchor.height + 30, width: anchor.width - PROPOSAL_INDENT });
    expect(box.path).toBeUndefined();
    expect(next.y).toBe((placed(plain, "7")?.y ?? 0) + box.height + 30);
    // Proposals are never nodes, routes, or ports: the compiled routes are the same ones.
    expect(layout.nodes.map((item) => item.id)).toEqual(plain.nodes.map((item) => item.id));
    expect(edgeIds(layout)).toEqual(edgeIds(plain));
    // The compiled route runs straight down the lane's rail, beside the proposed step, from the step to the next.
    const rail = anchor.x + PROPOSAL_RAIL;
    expect(edge(layout, "6>7>sequence")?.points).toEqual([
      { x: rail, y: anchor.y + anchor.height },
      { x: rail, y: next.y },
    ]);
    expect(rail).toBeLessThan(box.x);
    // Dashed connectors: from the step into it, and from it to the step after.
    expect(dashed(layout, "email>in")).toEqual([
      { x: anchor.x + anchor.width / 2, y: anchor.y + anchor.height },
      { x: anchor.x + anchor.width / 2, y: box.y },
    ]);
    expect(dashed(layout, "email>out")).toEqual([
      { x: box.x + box.width / 2, y: box.y + box.height },
      { x: box.x + box.width / 2, y: next.y },
    ]);
    expect(clear(layout)).toBe(true);
    expect(overlaps(layout)).toBe(false);
  });

  it("starts a decision's chosen path: yes in the yes lane, no in the condition's own", () => {
    const yes = layoutFlow(createOrder, { ...options, proposals: [proposal({ after: IF, path: "yes" })] });
    const condition = placed(yes, "2");
    const first = placed(yes, "3");
    const box = room(yes);
    expect(box).toMatchObject({ host: "2", path: "yes", x: (first?.x ?? 0) + PROPOSAL_INDENT });
    expect((first?.y ?? 0)).toBe(box.y + box.height + 30);
    // The compiled yes still leaves the diamond's right corner and enters its first step, along the rail.
    expect(edge(yes, "2>3>yes")?.points).toEqual([
      { x: (condition?.x ?? 0) + (condition?.width ?? 0), y: (condition?.y ?? 0) + (condition?.height ?? 0) / 2 },
      { x: (first?.x ?? 0) + PROPOSAL_RAIL, y: (condition?.y ?? 0) + (condition?.height ?? 0) / 2 },
      { x: (first?.x ?? 0) + PROPOSAL_RAIL, y: first?.y },
    ]);
    expect(dashed(yes, "email>in")?.at(-1)).toEqual({ x: box.x + box.width / 2, y: box.y });
    expect(dashed(yes, "email>out")?.at(-1)).toEqual({ x: box.x + box.width / 2, y: first?.y });
    expect(clear(yes)).toBe(true);

    // `no` has no steps of its own here: its route goes on to the step after the `if`.
    const no = layoutFlow(createOrder, { ...options, proposals: [proposal({ after: IF, path: "no" })] });
    const shut = room(no);
    const diamond = placed(no, "2");
    const after = placed(no, "6");
    expect(shut).toMatchObject({ host: "2", path: "no", x: (diamond?.x ?? 0) + PROPOSAL_INDENT, y: (diamond?.y ?? 0) + (diamond?.height ?? 0) + 30 });
    const route = edge(no, "2>6>no")?.points ?? [];
    // Out of the diamond's bottom corner, onto the rail, and into the next step: still one compiled route.
    expect(route[0]).toEqual({ x: (diamond?.x ?? 0) + (diamond?.width ?? 0) / 2, y: (diamond?.y ?? 0) + (diamond?.height ?? 0) });
    expect(route.at(-1)).toEqual({ x: (after?.x ?? 0) + PROPOSAL_RAIL, y: after?.y });
    expect(route.every((point) => point.x <= (diamond?.x ?? 0) + (diamond?.width ?? 0) / 2)).toBe(true);
    expect(edge(no, "2>6>no")?.label).toBe("no");
    expect(dashed(no, "email>out")?.at(-1)).toEqual({ x: shut.x + shut.width / 2, y: after?.y });
    expect(clear(no)).toBe(true);
  });

  it("enters a target whose top isn't flat at its middle (End)", () => {
    const layout = layoutFlow(createOrder, { ...options, proposals: [proposal({ after: REPLY })] });
    const end = placed(layout, "9");
    const route = edge(layout, "8>9>sequence")?.points ?? [];
    expect(route.at(-1)).toEqual({ x: (end?.x ?? 0) + (end?.width ?? 0) / 2, y: end?.y });
    expect(route[0]?.x).toBe((placed(layout, "8")?.x ?? 0) + PROPOSAL_RAIL);
    expect(dashed(layout, "email>out")?.at(-1)).toEqual({ x: (end?.x ?? 0) + (end?.width ?? 0) / 2, y: end?.y });
  });

  it("stacks proposed steps on one step in order, each joined to the next", () => {
    const layout = layoutFlow(createOrder, { ...options, proposals: [proposal(), proposal({ id: "slack", title: "Post to the orders channel" })] });
    const [first, second] = layout.proposals;
    expect(second?.y).toBe((first?.y ?? 0) + (first?.height ?? 0) + 30);
    expect(layout.proposalEdges.map((item) => item.id)).toEqual(["email>in", "slack>in", "slack>out"]);
    expect(edge(layout, "6>7>sequence")?.points).toHaveLength(2);
    expect(clear(layout)).toBe(true);
  });

  it("goes after a collapsed step group only when its step is the group's last, joined to what follows", () => {
    const groups = [collapsed(place)];
    const last = layoutFlow(createOrder, { ...options, groups, proposals: [proposal({ after: REPLY })] });
    const step = placed(last, "group:place-it");
    const box = room(last);
    expect(box).toMatchObject({ host: "group:place-it", y: (step?.y ?? 0) + (step?.height ?? 0) + 30 });
    expect(box.inside).toBeUndefined();
    expect(box.note).toBeUndefined();
    expect(dashed(last, "email>in")?.[0]).toEqual({ x: (step?.x ?? 0) + (step?.width ?? 0) / 2, y: (step?.y ?? 0) + (step?.height ?? 0) });
    expect(dashed(last, "email>out")?.at(-1)?.y).toBe(placed(last, "9")?.y);
  });

  it("sits under a collapsed step group hiding a step before its last, saying it goes inside and after which step, without connectors", () => {
    const groups = [collapsed(place)];
    const grouped = layoutFlow(createOrder, { ...options, groups });
    const inside = layoutFlow(createOrder, { ...options, groups, proposals: [proposal()] });
    const step = placed(inside, "group:place-it");
    const box = room(inside);
    const label = createOrder.nodes.find((node) => node.key === ORDER)?.label ?? "";
    expect(label).not.toBe("");
    expect(box).toMatchObject({ host: "group:place-it", inside: "place-it", note: insideNote(place.title, label), y: (step?.y ?? 0) + (step?.height ?? 0) + 30 });
    expect(box.path).toBeUndefined();
    // Not drawn in sequence after the group: no dashed connectors into it or out of it.
    expect(inside.proposalEdges).toEqual([]);
    // Its note has room for the action that shows the group's steps.
    expect(box.height).toBeGreaterThan(room(layoutFlow(createOrder, { ...options, groups, proposals: [proposal({ after: REPLY })] })).height);
    // The compiled path is unchanged, and still runs beside it along the lane's rail.
    expect(inside.nodes.map((item) => item.id)).toEqual(grouped.nodes.map((item) => item.id));
    expect(edgeIds(inside)).toEqual(edgeIds(grouped));
    const out = inside.edges.find((item) => item.from === "group:place-it")?.points ?? [];
    expect(out.some((point) => point.x === (step?.x ?? 0) + PROPOSAL_RAIL)).toBe(true);
    expect(clear(inside)).toBe(true);
    // A label the page gives the step names it.
    const labeled = layoutFlow(createOrder, { ...options, groups, labels: { [ORDER]: "Place the order" }, proposals: [proposal()] });
    expect(room(labeled).note).toBe(insideNote(place.title, "Place the order"));

    // With one after the group's last step too: inside first, then after, its connector from below the one inside.
    const both = layoutFlow(createOrder, { ...options, groups, proposals: [proposal(), proposal({ id: "slack", after: REPLY, title: "Post to the orders channel" })] });
    const [first, second] = both.proposals;
    expect(first).toMatchObject({ id: "email", inside: "place-it" });
    expect(second?.inside).toBeUndefined();
    expect(second?.y).toBe((first?.y ?? 0) + (first?.height ?? 0) + 30);
    expect(dashed(both, "slack>in")?.[0]?.y).toBe((first?.y ?? 0) + (first?.height ?? 0));
    expect(both.proposalEdges.map((item) => item.id)).toEqual(["slack>in", "slack>out"]);

    // Expanded, the group shows its steps, and the proposed step goes after its own.
    const open = layoutFlow(createOrder, { ...options, groups: [place], proposals: [proposal()] });
    expect(room(open)).toMatchObject({ host: "6" });
    expect(room(open).inside).toBeUndefined();
  });

  it("goes below the diagram when its step isn't drawn", () => {
    const missing = layoutFlow(createOrder, { ...options, proposals: [proposal({ after: `${FN}/variable:gone`, note: "The step it went after is missing." })] });
    const box = room(missing);
    expect(box.host).toBeUndefined();
    expect(box.note).toBe("The step it went after is missing.");
    expect(box.y).toBeGreaterThan((placed(missing, "9")?.y ?? 0) + (placed(missing, "9")?.height ?? 0));
    expect(missing.proposalEdges).toEqual([]);
    expect(edgeIds(missing)).toEqual(edgeIds(plain));
    // A step folded into a chain isn't drawn either; health says nothing, so the note is the layout's own.
    const folded = layoutFlow(chain, { ...options, proposals: [{ id: "x", after: "src/a.ts#function:a/n3", path: "no", title: "Log it" }] });
    expect(room(folded)).toMatchObject({ note: UNDRAWN_NOTE });
    expect(room(folded).host).toBeUndefined();
  });

  it("keeps a held step's note under its title, and room for it", () => {
    const quiet = room(layoutFlow(createOrder, { ...options, proposals: [proposal()] }));
    const held = room(layoutFlow(createOrder, { ...options, proposals: [proposal({ note: "Held for review." })] }));
    expect(held.note).toBe("Held for review.");
    expect(held.host).toBe("6");
    expect(held.height).toBe(quiet.height + 3 + 16);
  });

  it("starts a loop's body on each, and goes after its region on done", () => {
    const header = "src/a.ts#function:a/n1";
    const each = layoutFlow(loop, { ...options, proposals: [{ id: "x", after: header, path: "each", title: "Log the item" }] });
    const repeats = each.regions.find((region) => region.kind === "loop");
    const box = room(each);
    expect(box.y).toBeGreaterThan((placed(each, "1")?.y ?? 0) + (placed(each, "1")?.height ?? 0));
    expect((placed(each, "2")?.y ?? 0)).toBe(box.y + box.height + 30);
    expect(repeats && box.y + box.height < repeats.y + repeats.height).toBe(true);
    expect(clear(each)).toBe(true);
    const done = layoutFlow(loop, { ...options, proposals: [{ id: "x", after: header, path: "done", title: "Log the end" }] });
    const region = done.regions.find((item) => item.kind === "loop");
    const after = room(done);
    expect(region && after.y > region.y + region.height).toBe(true);
    expect(placed(done, "5")?.y).toBe(after.y + after.height + 30);
    expect(dashed(done, "x>in")?.[0]).toEqual({ x: (placed(done, "1")?.x ?? 0) + (placed(done, "1")?.width ?? 0), y: (placed(done, "1")?.y ?? 0) + (placed(done, "1")?.height ?? 0) / 2 });
    expect(clear(done)).toBe(true);
  });
});
