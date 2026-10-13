// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BoardView } from "../board/BoardView";
import { queryClient } from "../ipc/client";
import type { Card, CardView, FlowChip, FlowModel, FlowNode, Group, Key } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { BoardSidebar } from "../board/BoardSidebar";
import * as store from "../state/boards/store";
import { usePeekTarget } from "../state/overlays";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { stepPort } from "../code/outline";
import { insetPort } from "../canvas/arrows";
import { forgetVerifiedCode } from "../canvas/cardData";
import { FlowView, flowPortNodes, type FlowExpansion } from "./FlowView";
import { expansionState } from "./insets";
import { INSIDE_ACTION, insideNote, layoutFlow, type Measure, type StepGroup } from "./layout";

const createOrder = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
if (!createOrder.flow) {
  throw new Error("the fixture has a Flow model");
}
const flow: FlowModel = createOrder.flow;
const measure: Measure = (text) => text.length * 7;
const FN = "src/routes/orders.ts#function:createOrder";
const IF = `${FN}/if[0]`;
const QUEUE = "Queue it for a worker, reply 202";
const PLACE = "Place it now and reply 201";

const queue: StepGroup = {
  id: "queue-it",
  title: QUEUE,
  members: [`${IF}/variable:job`, `${IF}/expression_statement[0]`, `${IF}/return[0]`],
  collapsed: false,
  returns: true,
  throws: false,
};
const place: StepGroup = {
  id: "place-it",
  title: PLACE,
  members: [`${FN}/variable:order`, `${FN}/expression_statement[0]`, `${FN}/expression_statement[1]`],
  collapsed: true,
  returns: false,
  throws: false,
};

function renderFlow(groups: readonly StepGroup[], selectedGroup: string | null = null, extra: { selectedKey?: string; runKeys?: ReadonlySet<string> } = {}) {
  const handlers = {
    onSelectGroup: vi.fn(),
    onToggleGroup: vi.fn(),
    onGroupMenu: vi.fn(),
    onSelect: vi.fn(),
    onExtend: vi.fn(),
    onViewCode: vi.fn(),
    onViewGroupCode: vi.fn(),
    onStepMenu: vi.fn(),
  };
  const layout = layoutFlow(flow, { measure, maxWidth: 878, groups });
  const view = render(<FlowView model={flow} layout={layout} width={600} selectedGroup={selectedGroup} {...extra} {...handlers} />);
  return { ...view, ...handlers, layout };
}

describe("the Flow view's step groups", () => {
  it("draws an expanded group as a titled region whose toggle collapses it and whose title selects it", () => {
    const { container, onToggleGroup, onSelectGroup, onGroupMenu } = renderFlow([queue]);
    const region = container.querySelector(".flow-region.kind-group");
    expect(region).not.toBeNull();
    expect(region?.classList.contains("selected")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: `Collapse step group ${QUEUE}` }));
    expect(onToggleGroup).toHaveBeenCalledWith("queue-it");
    expect(onSelectGroup).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText(QUEUE));
    expect(onSelectGroup).toHaveBeenCalledWith("queue-it");
    fireEvent.contextMenu(screen.getByText(QUEUE));
    expect(onGroupMenu).toHaveBeenCalledWith("queue-it", expect.anything());
    // The steps inside keep their own nodes.
    expect(container.querySelector('[data-flow-node="3"]')).not.toBeNull();
  });

  it("draws a collapsed group as one step with its title, its size, and what can leave it early", () => {
    const { container, onToggleGroup, onSelectGroup, onSelect } = renderFlow([{ ...queue, collapsed: true }, place], "place-it");
    const step = container.querySelector('[data-flow-node="group:place-it"]');
    expect(step?.classList.contains("kind-group")).toBe(true);
    expect(step?.classList.contains("selected")).toBe(true);
    expect(step?.querySelector(".flow-type")?.textContent).toMatch(/^COMBINED3 steps/);
    expect(step?.textContent).toContain("3 steps");
    expect(step?.textContent).toContain(PLACE);
    expect(step?.querySelector(".flow-group-marks")).toBeNull();
    expect(container.querySelector('[data-flow-node="group:queue-it"] .flow-group-marks')?.textContent).toBe("returns");
    expect(container.querySelector('[data-flow-node="6"]')).toBeNull();
    if (step) {
      fireEvent.click(step);
    }
    expect(onSelectGroup).toHaveBeenCalledWith("place-it");
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: `Expand step group ${PLACE}` }));
    expect(onToggleGroup).toHaveBeenCalledWith("place-it");
    expect(onSelectGroup).toHaveBeenCalledTimes(1);
  });

  it("puts View code's </> on a combined block, collapsed or expanded, opening the group's source", () => {
    const { container, onViewGroupCode, onSelectGroup } = renderFlow([queue, place]);
    const collapsed = container.querySelector<HTMLElement>('[data-flow-node="group:place-it"]');
    fireEvent.click(within(collapsed ?? document.body).getByRole("button", { name: "View code" }));
    expect(onViewGroupCode).toHaveBeenCalledWith("place-it");
    const heading = container.querySelector<HTMLElement>('.flow-group-heading[data-group="queue-it"]');
    fireEvent.click(within(heading ?? document.body).getByRole("button", { name: "View code" }));
    expect(onViewGroupCode).toHaveBeenCalledWith("queue-it");
    // The button's click never selects what is under it.
    expect(onSelectGroup).not.toHaveBeenCalled();
  });

  it("marks the selected group's region", () => {
    const { container } = renderFlow([queue], "queue-it");
    expect(container.querySelector(".flow-region.kind-group.selected")).not.toBeNull();
  });

  it("puts a connection's step port on the step showing its call, the statement holding it, or the collapsed group hiding it", () => {
    const { layout } = renderFlow([place]);
    const chip = flow.nodes.find((node) => node.key?.startsWith(`${IF}/`) === true && (node.chips?.length ?? 0) > 0)?.chips?.[0];
    const holder = layout.nodes.find((placed) => placed.node.chips?.some((item) => item.source === chip?.source));
    const statement = layout.nodes.find((placed) => placed.node.key === `${IF}/variable:job`);
    expect(chip && holder && statement).toBeTruthy();
    const ports = new Map([
      [stepPort(chip?.source ?? ""), FN],
      [stepPort(`${IF}/variable:job/await[0]`), FN],
      [stepPort(`${FN}/expression_statement[1]`), FN],
      [stepPort("src/elsewhere.ts#function:other"), FN],
    ]);
    const { byNode, unplaced } = flowPortNodes(layout, ports);
    expect(byNode.get(holder?.id ?? "")).toContain(stepPort(chip?.source ?? ""));
    expect(byNode.get(statement?.id ?? "")).toContain(stepPort(`${IF}/variable:job/await[0]`));
    expect(byNode.get("group:place-it")).toEqual([stepPort(`${FN}/expression_statement[1]`)]);
    expect(unplaced).toEqual([stepPort("src/elsewhere.ts#function:other")]);
  });

  it("leads a connection's port to the card's edge in the authored style, and a call's in the arrows' color", () => {
    const layout = layoutFlow(flow, { measure, maxWidth: 878, groups: [] });
    const [first, second] = layout.nodes.filter((placed) => placed.node.kind !== "start" && placed.node.kind !== "end");
    const ports = new Map([
      [first?.id ?? "", [stepPort(`${IF}/variable:job/call[0]`)]],
      [second?.id ?? "", [stepPort(`${FN}/variable:draft`), `${FN}/variable:order/call[0]`]],
    ]);
    const { container } = render(<FlowView model={flow} layout={layout} width={600} ports={ports} renderPorts={() => null} />);
    const leaders = [...container.querySelectorAll(".flow-leader")];
    expect(leaders.map((leader) => leader.classList.contains("authored"))).toEqual([true, false]);
  });

  it("puts the ports of the steps a collapsed group hides on its step", () => {
    const { layout } = renderFlow([place]);
    const chip = flow.nodes[6]?.chips?.[0];
    expect(chip).toBeDefined();
    const { byNode, unplaced } = flowPortNodes(layout, new Map([[chip?.source ?? "", FN]]));
    expect(byNode.get("group:place-it")).toEqual([chip?.source]);
    expect(unplaced).toEqual([]);
  });
});

describe("the Flow view's proposed steps", () => {
  const ORDER = `${FN}/variable:order`;
  const proposals = [
    { id: "email", after: ORDER, title: "Email a confirmation" },
    { id: "gone", after: `${FN}/variable:gone`, title: "Log the order", note: "The step it went after is missing." },
  ];
  const marks = new Map([
    ["email", { author: "agent:claude" }],
    ["gone", { author: "person:maya", state: "missing" as const }],
  ]);

  it("draws a dashed green PROPOSED box with its title as plain words and the agent mark, joined by dashed connectors, and never a node or a port", () => {
    const onSelect = vi.fn();
    const layout = layoutFlow(flow, { measure, maxWidth: 878, proposals });
    const ports = new Map([[layout.nodes.find((placed) => placed.node.key === ORDER)?.id ?? "", [`${ORDER}/call[0]`]]]);
    const { container } = render(
      <FlowView model={flow} layout={layout} width={600} ports={ports} renderPorts={(ids) => <span className="probe-port">{ids.join(",")}</span>} proposals={{ marks, selected: "email", onSelect }} />,
    );
    const box = container.querySelector<HTMLElement>('[data-proposal="email"]');
    expect(box?.classList.contains("flow-proposal")).toBe(true);
    expect(box?.classList.contains("selected")).toBe(true);
    expect(box?.querySelector(".flow-type")?.textContent).toBe("PROPOSED");
    expect(box?.querySelector(".flow-proposal-agent")?.getAttribute("title")).toBe("Written by Claude, an agent");
    // Its title is plain words, never code.
    expect(box?.querySelector(".flow-proposal-title")?.textContent).toBe("Email a confirmation");
    expect(box?.querySelector("code, .flow-callee, .flow-pill-name")).toBeNull();
    // Not a step: no node id (Flow checks count those), no </>, and no port or leader of its own.
    expect(box?.hasAttribute("data-flow-node")).toBe(false);
    expect(box?.querySelector(".view-code")).toBeNull();
    expect(container.querySelectorAll(".flow-leader")).toHaveLength(1);
    expect(container.querySelectorAll("[data-flow-node]")).toHaveLength(flow.nodes.length);
    // Dashed green connectors in and out; the compiled route is still drawn, solid.
    expect([...container.querySelectorAll(".flow-proposal-edge")].map((path) => path.getAttribute("data-proposal-edge"))).toEqual(["email", "email"]);
    expect(container.querySelector('.flow-edge.kind-sequence path')).not.toBeNull();
    expect(container.querySelectorAll(".flow-edge")).toHaveLength(flow.routes.length);
    fireEvent.click(box ?? document.body);
    expect(onSelect).toHaveBeenCalledWith("email");

    // One whose step is missing keeps its words, in the placeholder look with why.
    const gone = container.querySelector('[data-proposal="gone"]');
    expect(gone?.classList.contains("missing")).toBe(true);
    expect(gone?.querySelector(".flow-proposal-title")?.textContent).toBe("Log the order");
    expect(gone?.querySelector(".flow-proposal-note")?.textContent).toBe("The step it went after is missing.");
    expect(gone?.querySelector(".flow-proposal-agent")).toBeNull();
  });

  it("says a proposed step goes inside a collapsed combined block, with the action that shows its steps, and draws no connectors for it", () => {
    const onSelect = vi.fn();
    const onToggleGroup = vi.fn();
    const layout = layoutFlow(flow, { measure, maxWidth: 878, groups: [place], proposals: [{ id: "email", after: ORDER, title: "Email a confirmation" }] });
    const { container } = render(<FlowView model={flow} layout={layout} width={600} onToggleGroup={onToggleGroup} proposals={{ marks, selected: null, onSelect }} />);
    const box = container.querySelector<HTMLElement>('[data-proposal="email"]');
    const label = flow.nodes.find((node) => node.key === ORDER)?.label ?? "";
    const note = box?.querySelector(".flow-proposal-note");
    expect(note?.classList.contains("inside")).toBe(true);
    expect(note?.textContent).toBe(`${insideNote(PLACE, label)} ${INSIDE_ACTION}`);
    expect(container.querySelectorAll(".flow-proposal-edge")).toHaveLength(0);
    fireEvent.click(within(box ?? document.body).getByRole("button", { name: INSIDE_ACTION }));
    expect(onToggleGroup).toHaveBeenCalledWith("place-it");
    // Showing the steps doesn't select the proposed step.
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("draws nothing for proposals unless the card draws its own (an inset never does)", () => {
    const layout = layoutFlow(flow, { measure, maxWidth: 878, proposals });
    const { container } = render(<FlowView model={flow} layout={layout} width={600} inset />);
    expect(container.querySelector("[data-proposal], .flow-proposal-edge")).toBeNull();
  });
});

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

describe("the Flow view's steps and View code", () => {
  const JOB = `${IF}/variable:job`;
  const REPLY = `${IF}/expression_statement[0]`;
  const node = (container: HTMLElement, key: string) => {
    const id = flow.nodes.find((item) => item.key === key)?.id;
    const found = container.querySelector<HTMLElement>(`[data-flow-node="${String(id)}"]`);
    if (!found) {
      throw new Error(`no node for ${key}`);
    }
    return found;
  };

  it("puts </> on each step with recorded source, never on Start or End, and its click opens that step's code", () => {
    const { container, onViewCode, onSelect } = renderFlow([]);
    fireEvent.click(within(node(container, IF)).getByRole("button", { name: "View code" }));
    expect(onViewCode).toHaveBeenCalledWith(IF);
    fireEvent.click(within(node(container, JOB)).getByRole("button", { name: "View code" }));
    expect(onViewCode).toHaveBeenLastCalledWith(JOB);
    expect(onSelect).not.toHaveBeenCalled();
    expect(within(container.querySelector<HTMLElement>(".flow-pill.kind-start") ?? document.body).queryByRole("button", { name: "View code" })).toBeNull();
    expect(within(container.querySelector<HTMLElement>(".flow-pill.kind-end") ?? document.body).queryByRole("button", { name: "View code" })).toBeNull();
  });

  it("extends the selection on Shift-click and draws every step of a run selected", () => {
    const { container, onExtend, onSelect, rerender, layout } = renderFlow([]);
    fireEvent.click(node(container, JOB), { shiftKey: true });
    expect(onExtend).toHaveBeenCalledWith(JOB);
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(node(container, JOB));
    expect(onSelect).toHaveBeenCalledWith(JOB);
    rerender(<FlowView model={flow} layout={layout} width={600} selectedKey={JOB} runKeys={new Set([JOB, REPLY])} />);
    expect(node(container, JOB).classList.contains("selected")).toBe(true);
    expect(node(container, REPLY).classList.contains("selected")).toBe(true);
    expect(node(container, IF).classList.contains("selected")).toBe(false);
  });

  it("opens a step's menu on right-click (selecting it first unless it is selected) and on Shift+F10", () => {
    const { container, onSelect, onStepMenu } = renderFlow([], null, { selectedKey: JOB, runKeys: new Set([JOB, REPLY]) });
    fireEvent.contextMenu(node(container, REPLY));
    expect(onSelect).not.toHaveBeenCalled();
    expect(onStepMenu).toHaveBeenCalledWith(REPLY, expect.anything());
    fireEvent.contextMenu(node(container, IF));
    expect(onSelect).toHaveBeenCalledWith(IF);
    expect(onStepMenu).toHaveBeenLastCalledWith(IF, expect.anything());
    const step = node(container, JOB);
    expect(step.tabIndex).toBe(-1);
    fireEvent.keyDown(step, { key: "F10", shiftKey: true });
    expect(onStepMenu).toHaveBeenLastCalledWith(JOB, step);
  });
});

describe("a step group's one collapse state", () => {
  const P = "p-orders0000";
  const REF = { project: P, id: ".unfold/boards/b.json" };
  const PAGE = { board: REF, page: "page-1" };
  const card: Card = { id: "create-order", ref: { block: FN }, at: [0, 0], view: "flow" };
  const placeGroup: Group = { id: "place-it", card: "create-order", title: PLACE, members: [...place.members], collapsed: false };
  let engine: FakeEngine;

  beforeEach(() => {
    store.resetBoards();
    queryClient.clear();
    engine = installFakeEngine({
      handlers: {
        card: () => createOrder,
        list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
        save_board: () => boardState(P, REF.id),
      },
    });
  });

  afterEach(() => {
    store.resetBoards();
    engine.uninstall();
  });

  const groupNow = () => store.getOpenBoard(REF)?.document.pages[0]?.groups[0];

  it("collapses from Flow as one undo step, selects the group from its step, and folds Code the same way", async () => {
    store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1", [card], { groups: [placeGroup], defaultView: "diagram" })])));
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
    const { container } = render(
      <QueryClientProvider client={queryClient}>
        <div style={{ width: 1200, height: 800 }}>
          <BoardView board={REF} />
        </div>
      </QueryClientProvider>,
    );
    fireEvent.click(await screen.findByRole("button", { name: `Collapse step group ${PLACE}` }));
    expect(groupNow()?.collapsed).toBe(true);
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
    const step = await waitFor(() => {
      const found = container.querySelector<HTMLElement>('[data-flow-node="group:place-it"]');
      expect(found).not.toBeNull();
      return found;
    });
    act(() => {
      step?.click();
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "group", groupId: "place-it" });

    act(() => {
      store.undo(REF);
    });
    expect(groupNow()?.collapsed).toBe(false);
    await screen.findByRole("button", { name: `Collapse step group ${PLACE}` });

    act(() => {
      store.redo(REF);
    });
    act(() => {
      store.setPageLens(PAGE, "code");
    });
    await waitFor(() => {
      expect(container.querySelector('.code-group.collapsed[data-group="place-it"]')).not.toBeNull();
    });
  });
});

describe("calls expanded in place, as the Flow view draws them", () => {
  const PLACE_CALL = flow.nodes[6]?.chips?.[0]?.source ?? "";
  const PLACE_KEY = flow.nodes[6]?.key ?? "";

  it("puts a chevron on resolved calls only, and draws each inset in the room kept under its node", () => {
    const layout = layoutFlow(flow, { measure, maxWidth: 878, insets: new Map([[PLACE_KEY, { width: 260, height: 90 }]]) });
    const renderInset = vi.fn(() => <p>inset here</p>);
    const onToggle = vi.fn();
    const expansion: FlowExpansion = {
      stateOf: (call) => expansionState(call, [], [FN], [[PLACE_CALL]], 3),
      onToggle,
      renderInset,
    };
    const { container } = render(<FlowView model={flow} layout={layout} width={600} expansion={expansion} />);
    const chevrons = screen.getAllByRole("button", { name: /^(Expand|Collapse) .* here$/ });
    expect(chevrons.map((button) => [button.getAttribute("aria-label"), button.getAttribute("aria-expanded")])).toEqual([
      ["Collapse OrderService.place here", "true"],
      ["Expand slack.post here", "false"],
    ]);
    // External and unresolved calls keep their explanation and get no chevron.
    expect(container.querySelector('[data-flow-node="3"] .flow-call-toggle, [data-flow-node="4"] .flow-call-toggle')).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand slack.post here" }));
    expect(onToggle).toHaveBeenCalledWith(expect.objectContaining({ key: flow.nodes[7]?.chips?.[0]?.source, name: "slack.post" }));
    const slot = container.querySelector<HTMLElement>(`.flow-inset-slot[data-inset-node="${PLACE_KEY}"]`);
    expect(slot?.textContent).toBe("inset here");
    expect(renderInset).toHaveBeenCalledWith(layout.insets[0]);
    const node = container.querySelector<HTMLElement>('[data-flow-node="6"]');
    expect(parseFloat(slot?.style.top ?? "0")).toBe(parseFloat(node?.style.top ?? "0") + parseFloat(node?.style.height ?? "0") + 8);
  });

  it("keeps a call's port on its own step when it is expanded, never at the bottom of its inset", () => {
    const layout = layoutFlow(flow, { measure, maxWidth: 878, insets: new Map([[PLACE_KEY, { width: 260, height: 90 }]]) });
    const { byNode, unplaced } = flowPortNodes(layout, new Map([[PLACE_CALL, FN]]));
    expect(byNode.get("6")).toEqual([PLACE_CALL]);
    expect(unplaced).toEqual([]);
    const expansion: FlowExpansion = { stateOf: () => null, onToggle: vi.fn(), renderInset: () => null };
    const { container } = render(
      <FlowView model={flow} layout={layout} width={600} expansion={expansion} ports={byNode} renderPorts={(ids) => ids.map((id) => <i key={id} data-port={id} />)} />,
    );
    const leader = container.querySelector<HTMLElement>(".flow-leader");
    const slot = container.querySelector<HTMLElement>(".flow-inset-slot");
    expect(leader?.querySelector(`[data-port="${PLACE_CALL}"]`)).not.toBeNull();
    expect(parseFloat(leader?.style.top ?? "0")).toBeLessThan(parseFloat(slot?.style.top ?? "0"));
  });
});

describe("calls expanded in place on a card", () => {
  const P = "p-orders0000";
  const REF = { project: P, id: ".unfold/boards/b.json" };
  const PAGE = { board: REF, page: "page-1" };
  const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
  const CHARGE = "src/services/payment.ts#function:charge";
  const AUDIT = "src/services/audit.ts#function:audit";
  const LAST = "src/services/audit.ts#function:last";
  const SLACK = "src/lib/slack.ts#variable:slack/method:post";
  const ONE = `${PLACE}/arrow_function[0]`;
  const PLACE_CALL = flow.nodes[6]?.chips?.[0]?.source ?? "";

  const chip = (source: Key, target: Key | undefined, name: string): FlowChip => ({
    source,
    seq: 0,
    kind: "calls",
    resolution: target !== undefined ? "resolved" : "external",
    ...(target !== undefined ? { target } : {}),
    name,
    line: 4,
  });
  const callNode = (owner: Key, index: number, target: Key | undefined, name: string, extra: Partial<FlowNode> = {}): FlowNode => ({
    id: index,
    kind: "call",
    key: `${owner}/n${index}`,
    label: name,
    lane: 0,
    startLine: 3 + index,
    chips: [chip(`${owner}/n${index}/call[0]`, target, name)],
    call: 0,
    ...extra,
  });
  /** A block whose Flow is `steps`, one after another. */
  function block(key: Key, path: string, steps: readonly FlowNode[]): CardView {
    const name = key.replace(/^.*[:/]/, "");
    const nodes: FlowNode[] = [{ id: 0, kind: "start", key, label: name, lane: 0 }, ...steps, { id: steps.length + 1, kind: "end", label: "End", lane: 0 }];
    const model: FlowModel = {
      key,
      path,
      language: "typescript",
      kind: "function",
      source: "current",
      nodes,
      routes: nodes.slice(1).map((node, index) => ({ from: index, to: node.id, kind: "sequence" as const })),
      tree: steps.map((step) => ({ type: "node" as const, node: step.id, ends: false, lanes: 1 })),
      start: 0,
      end: steps.length + 1,
      lanes: 1,
      warnings: [],
    };
    return {
      type: "block",
      header: { key, kind: "function", name, qualifiedName: name === "place" ? "OrderService.place" : name === "post" ? "slack.post" : name, path, startLine: 3, endLine: 30, language: "typescript", fileState: "current", source: "current", owner: `${path}#` },
      code: null,
      flow: model,
    };
  }
  const MODELS: Readonly<Record<Key, CardView>> = {
    [FN]: createOrder,
    [PLACE]: block(PLACE, "src/services/order-service.ts", [
      callNode(PLACE, 1, FN, "createOrder"),
      callNode(PLACE, 2, CHARGE, "charge", { callbacks: [{ key: ONE, label: "callback to withRetry", lines: 1, throws: false }] }),
    ]),
    [ONE]: block(ONE, "src/services/order-service.ts", [callNode(ONE, 1, SLACK, "slack.post")]),
    [SLACK]: block(SLACK, "src/lib/slack.ts", [callNode(SLACK, 1, undefined, "fetch")]),
    [CHARGE]: block(CHARGE, "src/services/payment.ts", [callNode(CHARGE, 1, AUDIT, "audit")]),
    [AUDIT]: block(AUDIT, "src/services/audit.ts", [callNode(AUDIT, 1, LAST, "last")]),
    [LAST]: block(LAST, "src/services/audit.ts", []),
  };
  const CHARGE_CALL = `${PLACE}/n2/call[0]`;
  const AUDIT_CALL = `${CHARGE}/n1/call[0]`;
  let engine: FakeEngine;

  beforeEach(() => {
    store.resetBoards();
    queryClient.clear();
    // The window's memory of each block's last verified Code (a test may load other models of a key).
    forgetVerifiedCode(P);
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => {
          const model = MODELS[key];
          if (!model) {
            // eslint-disable-next-line @typescript-eslint/only-throw-error
            throw { code: "not_found", message: "no such key" };
          }
          return model;
        },
        line_labels: () => [],
        list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
        save_board: () => boardState(P, REF.id, store.getOpenBoard(REF)?.document),
      },
    });
  });

  afterEach(() => {
    store.resetBoards();
    engine.uninstall();
  });

  function PeekProbe() {
    const peek = usePeekTarget();
    return <output data-testid="peek">{peek ? `${peek.path}:${String(peek.line)}` : ""}</output>;
  }

  function open(cards: Card[], sidebar = false) {
    store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1", cards, { defaultView: "flow" })])));
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
    queryClient.setQueryData(engineKey("app_info", {}), { username: "maya", engineVersion: "0" });
    return render(
      <QueryClientProvider client={queryClient}>
        <div style={{ width: 1200, height: 800 }}>
          {sidebar ? <BoardSidebar board={REF} /> : null}
          <BoardView board={REF} />
          <PeekProbe />
        </div>
      </QueryClientProvider>,
    );
  }

  const cardNow = () => store.getOpenBoard(REF)?.document.pages[0]?.cards[0];
  const inset = async (name: string) => screen.findByRole("region", { name: `${name}, expanded in place` });

  it("expands a resolved call as a board edit: an inset titled with the callee and its file:line, drawing its own Flow", async () => {
    open([{ id: "create-order", ref: { block: FN }, at: [0, 0] }]);
    const chevrons = await screen.findAllByRole("button", { name: /^Expand .* here$/ });
    expect(chevrons.map((button) => button.getAttribute("aria-label"))).toEqual(["Expand OrderService.place here", "Expand slack.post here"]);
    fireEvent.click(screen.getByRole("button", { name: "Expand OrderService.place here" }));
    expect(cardNow()?.expanded).toEqual([[PLACE_CALL]]);
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
    const place = await inset("OrderService.place");
    await waitFor(() => {
      expect(place.querySelector(".flow-view.inset [data-flow-node]")).not.toBeNull();
    });
    expect(place.querySelector(".flow-inset-location")?.textContent).toBe("services/order-service.ts:3");
    // No second header, description, or caption; no Start or End.
    expect(place.querySelector(".card-header, .card-description, .flow-caption, .flow-pill")).toBeNull();
    // The call back to createOrder (the card's own block) keeps its follow link but has no chevron.
    expect(await within(place).findByRole("button", { name: "Follow createOrder" })).not.toBeNull();
    expect(within(place).queryByRole("button", { name: "Expand createOrder here", hidden: true })).toBeNull();
    expect(await within(place).findByRole("button", { name: "Expand charge here" })).not.toBeNull();
    // Its title opens the definition; → opens it as its own card.
    fireEvent.click(await within(place).findByRole("button", { name: "OrderService.place" }));
    expect(screen.getByTestId("peek").textContent).toBe("src/services/order-service.ts:3");
    fireEvent.click(await within(place).findByRole("button", { name: "Open OrderService.place as its own card" }));
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards.map((card) => card.ref)).toEqual([{ block: FN }, { block: PLACE }]);
    expect(cardNow()?.expanded).toEqual([[PLACE_CALL]]);
  });

  it("collapses a call from its inset's title, with the ones inside it", async () => {
    open([{ id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL], [PLACE_CALL, CHARGE_CALL]] }]);
    const place = await inset("OrderService.place");
    await inset("charge");
    fireEvent.click(place.querySelector<HTMLElement>(".flow-inset-title .flow-call-toggle") ?? place);
    expect(cardNow()?.expanded).toBeUndefined();
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "OrderService.place, expanded in place" })).toBeNull();
    });
    act(() => {
      store.undo(REF);
    });
    expect(cardNow()?.expanded).toEqual([[PLACE_CALL], [PLACE_CALL, CHARGE_CALL]]);
  });

  it("shows a one-call callback's proven callee via the callback, and selects a step inside with its owner", async () => {
    open([{ id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL], [PLACE_CALL, ONE]] }], true);
    const slack = await inset("slack.post");
    expect(slack.querySelector(".flow-inset-via")?.textContent).toBe("via callback to withRetry");
    const place = await inset("OrderService.place");
    // The step and its callback chip stay as they are: the chip's chevron, and the inset's own.
    expect(await within(place).findAllByRole("button", { name: "Collapse callback to withRetry here" })).toHaveLength(2);
    expect(await within(place).findByRole("button", { name: "Follow charge" })).not.toBeNull();
    const charge = await waitFor(() => {
      const found = place.querySelector<HTMLElement>(`[data-flow-node="2"]`);
      expect(found?.textContent).toContain("charge");
      return found;
    });
    act(() => {
      charge?.click();
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create-order", key: `${PLACE}/n2`, owner: PLACE });
    expect(charge?.classList.contains("selected")).toBe(true);
    const selected = await screen.findByRole("region", { name: "Selected" });
    await waitFor(() => {
      expect(selected.querySelector(".selected-location")?.textContent).toBe("CALL · line 5 · in OrderService.place, expanded in createOrder");
    });
  });

  it("nests three levels at most", async () => {
    open([{ id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL], [PLACE_CALL, CHARGE_CALL], [PLACE_CALL, CHARGE_CALL, AUDIT_CALL]] }]);
    const audit = await inset("audit");
    expect(await within(audit).findByRole("button", { name: "Follow last" })).not.toBeNull();
    expect(within(audit).queryByRole("button", { name: "Expand last here", hidden: true })).toBeNull();
    // The call's chevron, and the inset's own.
    expect(await within(await inset("charge")).findAllByRole("button", { name: "Collapse audit here" })).toHaveLength(2);
  });

  it("draws a callee's step groups only from its own card on the page, and its chevron there edits that card's group", async () => {
    const outlined = structuredClone(MODELS[PLACE]) as Extract<CardView, { type: "block" }>;
    const item = (key: Key, index: number) => ({ key, kind: "expression_statement" as const, parent: 0, startLine: 3 + index, endLine: 3 + index, start: 0, end: 1, groupable: true });
    outlined.code = {
      key: PLACE,
      path: "src/services/order-service.ts",
      language: "typescript",
      kind: "method",
      startLine: 3,
      endLine: 30,
      source: {
        state: "current",
        text: "",
        dedent: 0,
        lines: [],
        spans: [],
        outline: [{ key: PLACE, kind: "method", startLine: 3, endLine: 30, start: 0, end: 1, groupable: false }, item(`${PLACE}/n1`, 1), item(`${PLACE}/n2`, 2)],
        links: [],
      },
    };
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => (key === PLACE ? outlined : (MODELS[key] ?? createOrder)),
        line_labels: () => [],
        list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
        save_board: () => boardState(P, REF.id, store.getOpenBoard(REF)?.document),
      },
    });
    const pay: Group = { id: "pay", card: "place", title: "Pay for the order", members: [`${PLACE}/n1`, `${PLACE}/n2`], collapsed: true };
    store.adoptBoardState(
      boardState(
        P,
        REF.id,
        boardDocument([
          page(
            "page-1",
            [
              { id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL]] },
              { id: "place", ref: { block: PLACE }, at: [1200, 0], view: "diagram" },
            ],
            { defaultView: "flow", groups: [pay] },
          ),
        ]),
      ),
    );
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
    render(
      <QueryClientProvider client={queryClient}>
        <div style={{ width: 1200, height: 800 }}>
          <BoardView board={REF} />
        </div>
      </QueryClientProvider>,
    );
    const place = await inset("OrderService.place");
    await waitFor(() => {
      expect(place.querySelector('[data-flow-node="group:pay"]')?.textContent).toContain("Pay for the order");
    });
    fireEvent.click(await within(place).findByRole("button", { name: "Expand step group Pay for the order" }));
    expect(store.getOpenBoard(REF)?.document.pages[0]?.groups[0]?.collapsed).toBe(false);
    // Without its own card, the callee shows no groups (none are made for the caller).
    act(() => {
      store.editPage(PAGE, (draft) => {
        draft.cards.splice(1, 1);
      });
    });
    await waitFor(() => {
      expect(place.querySelector('.flow-region.kind-group, [data-flow-node^="group:"]')).toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.groups.every((group) => group.card !== "create-order")).toBe(true);
  });

  it("keeps the call's arrow leaving from its own step; the inset has ports only for its calls to cards on the page", async () => {
    const { container } = open([
      { id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL]] },
      { id: "place", ref: { block: PLACE }, at: [1200, 0], view: "diagram" },
    ]);
    await inset("OrderService.place");
    const card = () => container.querySelector<HTMLElement>('.block-card[data-card-id="create-order"]');
    await waitFor(() => {
      expect(card()?.querySelector(`.flow-leader [data-handleid="${PLACE_CALL}"]`)).not.toBeNull();
    });
    const leader = card()?.querySelector<HTMLElement>(`.flow-leader`);
    const slot = card()?.querySelector<HTMLElement>(".flow-inset-slot");
    expect(parseFloat(leader?.style.top ?? "0")).toBeLessThan(parseFloat(slot?.style.top ?? "0"));
    expect(slot?.querySelector(".card-port, .flow-leader")).toBeNull();
  });

  it("connects a followed callee's card from the inset's own call, though the callee showing it has no card, once per expanded call", async () => {
    const { container } = open([
      { id: "create-order", ref: { block: FN }, at: [0, 0], expanded: [[PLACE_CALL]] },
      { id: "charge", ref: { block: CHARGE }, at: [1200, 0], view: "diagram" },
    ]);
    const place = await inset("OrderService.place");
    const port = insetPort([PLACE_CALL], CHARGE_CALL);
    const handle = await waitFor(() => {
      const found = place.querySelector<HTMLElement>(`.flow-leader [data-handleid='${port}']`);
      expect(found).not.toBeNull();
      return found;
    });
    // On the charge step inside the inset, and nowhere else on the card.
    expect(handle?.closest(".flow-view.inset")).not.toBeNull();
    const charge = place.querySelector<HTMLElement>('[data-flow-node="2"]');
    const leader = handle?.closest<HTMLElement>(".flow-leader");
    expect(parseFloat(leader?.style.top ?? "0")).toBeGreaterThanOrEqual(parseFloat(charge?.style.top ?? "0"));
    expect(parseFloat(leader?.style.top ?? "0")).toBeLessThanOrEqual(parseFloat(charge?.style.top ?? "0") + parseFloat(charge?.style.height ?? "0"));
    const card = container.querySelector<HTMLElement>('.block-card[data-card-id="create-order"]');
    expect(card?.querySelectorAll(".card-port")).toHaveLength(1);
    expect(card?.querySelector(".card-header-ports .card-port")).toBeNull();
  });

  it("puts a connection leaving a step of a callback, shown as itself, on that step in the callback's inset", async () => {
    // A callback of two steps shows its own Flow.
    const two = block(ONE, "src/services/order-service.ts", [callNode(ONE, 1, SLACK, "slack.post"), callNode(ONE, 2, undefined, "fetch")]);
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => (key === ONE ? two : (MODELS[key] ?? createOrder)),
        line_labels: () => [],
        list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
        save_board: () => boardState(P, REF.id, store.getOpenBoard(REF)?.document),
      },
    });
    const step = `${ONE}/n2`;
    store.adoptBoardState(
      boardState(
        P,
        REF.id,
        boardDocument([
          page("page-1", [{ id: "place", ref: { block: PLACE }, at: [0, 0], expanded: [[ONE]] }], {
            defaultView: "flow",
            resources: [{ id: "outbox", kind: "queue", name: "outbox", at: [900, 0], anchors: [], author: "person:maya" }],
            links: [{ id: "sends", from: "place", to: "outbox", kind: "queue", step }],
          }),
        ]),
      ),
    );
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
    const { container } = render(
      <QueryClientProvider client={queryClient}>
        <div style={{ width: 1200, height: 800 }}>
          <BoardView board={REF} />
        </div>
      </QueryClientProvider>,
    );
    const callback = await inset("arrow_function[0]");
    const handle = await waitFor(() => {
      const found = callback.querySelector<HTMLElement>(`[data-handleid='${stepPort(step)}']`);
      expect(found).not.toBeNull();
      return found;
    });
    expect(handle?.closest(".flow-leader")?.classList.contains("authored")).toBe(true);
    // Not also on the step that holds the callback in the card's own Flow.
    const card = container.querySelector<HTMLElement>('.block-card[data-card-id="place"]');
    expect(card?.querySelectorAll(`[data-handleid='${stepPort(step)}']`)).toHaveLength(1);
  });
});
