// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BoardView } from "../board/BoardView";
import { queryClient } from "../ipc/client";
import type { Card, CardView, FlowModel, Group } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { stepPort } from "../code/outline";
import { FlowView, flowPortNodes } from "./FlowView";
import { layoutFlow, type Measure, type StepGroup } from "./layout";

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

function renderFlow(groups: readonly StepGroup[], selectedGroup: string | null = null) {
  const handlers = { onSelectGroup: vi.fn(), onToggleGroup: vi.fn(), onGroupMenu: vi.fn(), onSelect: vi.fn() };
  const layout = layoutFlow(flow, { measure, maxWidth: 878, groups });
  const view = render(<FlowView model={flow} layout={layout} width={600} selectedGroup={selectedGroup} {...handlers} />);
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
    expect(step?.textContent).toContain("STEP GROUP");
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

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

describe("a step group's one collapse state", () => {
  const P = "p-orders0000";
  const REF = { project: P, id: ".unfold/local/drafts/b.json" };
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
        save_board_view: () => null,
      },
    });
  });

  afterEach(() => {
    store.resetBoards();
    engine.uninstall();
  });

  const groupNow = () => store.getOpenBoard(REF)?.document.pages[0]?.groups[0];

  it("collapses from Flow as one undo step, selects the group from its step, and folds Code the same way", async () => {
    store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1", [card], { groups: [placeGroup] })])));
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
      container.querySelector<HTMLElement>('.block-card .segmented-option[data-value="code"]')?.click();
    });
    await waitFor(() => {
      expect(container.querySelector('.code-group.collapsed[data-group="place-it"]')).not.toBeNull();
    });
  });
});
