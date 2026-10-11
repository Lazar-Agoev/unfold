import { describe, expect, it } from "vitest";
import type { CardView, FlowModel, FlowNode } from "../ipc/generated";
import { fixture } from "../test/engine";
import { layoutFlow, roundedPath, wrapLines, type FlowLayout, type Measure, type PlacedNode, type StepGroup } from "./layout";

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
    expect(queued?.group).toEqual({ id: "queue-it", steps: 3, returns: true, throws: false });
    expect(direct?.group).toEqual({ id: "place-it", steps: 3, returns: false, throws: false });
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
    expect(placed(shut, "group:fail")?.group).toEqual({ id: "fail", steps: 1, returns: false, throws: true });
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
