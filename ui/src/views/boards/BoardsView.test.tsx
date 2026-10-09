// @vitest-environment jsdom
import "../../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BoardDialogs } from "../../boards/BoardDialogs";
import { closeBoardDialog } from "../../boards/dialogs";
import type { BoardList, BoardState, BoardSummary } from "../../ipc/generated";
import * as store from "../../state/boards/store";
import { useLayout } from "../../state/layout";
import { useWindow } from "../../state/window";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { BoardsView } from "./BoardsView";

const ID = ".unfold/local/drafts/post-orders.json";
const ENTRANCE = 'entrance:["express.route","http","POST","/orders"]';
const CREATE = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";

function summary(fields: Partial<BoardSummary> & Pick<BoardSummary, "id" | "title">): BoardSummary {
  return { place: "published", name: "board", cards: 0, label: null, heldBy: null, error: null, ...fields };
}

const LIST: BoardList = {
  published: [
    summary({ id: ".unfold/boards/order-placement.json", title: "Order placement", cards: 5 }),
    summary({ id: ".unfold/boards/worker-retries.json", title: "Worker retries", cards: 2, label: "2 outdated", heldBy: 7 }),
  ],
  drafts: [summary({ id: ID, title: "POST /orders", place: "draft", cards: 3 })],
};

function openState(): BoardState {
  return {
    id: ID,
    place: "draft",
    name: "post-orders",
    document: {
      format: "unfold-board",
      version: 1,
      title: "POST /orders",
      defaultView: "flow",
      cards: [
        { id: "post-orders", ref: { entrance: ENTRANCE }, at: [0, 0] },
        { id: "create-order", ref: { block: CREATE }, at: [440, 0], description: { text: "Places it", author: "person:alice", signature: "" } },
        { id: "place", ref: { block: PLACE }, at: [900, 0] },
      ],
      groups: [{ id: "queue", card: "create-order", title: "Queue or place now", members: [`${CREATE}/if[0]`], collapsed: false }],
      labels: {},
      guards: {},
    },
    revision: "r1",
    health: {
      items: [{ kind: "outdated", target: { card: "create-order" }, message: "the code changed since this was written", repairs: ["resign", "removeDescription"] }],
      missing: 0,
      outdated: 1,
      brokenGroups: 0,
      review: 0,
    },
    outline: {
      rows: [
        { card: "post-orders", depth: 0 },
        { card: "create-order", depth: 1 },
        { card: "place", depth: 2 },
      ],
      unlabeled: [
        { card: "create-order", key: `${CREATE}/variable:draft` },
        { card: "create-order", key: `${CREATE}/if[0]` },
      ],
    },
    label: "1 outdated",
  };
}

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  closeBoardDialog();
  useLayout.setState({ inspectorOpen: false });
  useWindow.getState().setInfo({ windowId: 1, key: "w1", folder: "/tmp/orders", platform: "darwin" });
  engine = installFakeEngine({
    handlers: {
      list_boards: () => LIST,
      new_board: () => ({ ...openState(), id: ".unfold/local/drafts/untitled.json", document: { ...openState().document, title: "Untitled board", cards: [], groups: [] } }),
      open_board: ({ id }) => ({ status: "opened", board: { ...openState(), id } }),
      repair_board: ({ repair }) => {
        expect(repair).toEqual({ kind: "remove_description", target: { card: "create-order" } });
        const state = openState();
        return {
          ...state,
          revision: "r2",
          document: { ...state.document, cards: state.document.cards.map(({ description: _description, ...card }) => card) },
          health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
          label: null,
        };
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the Boards view", () => {
  it("lists published boards and drafts, with a label only when something is wrong", async () => {
    renderWithClient(<BoardsView />);
    const placement = await screen.findByRole("option", { name: "Order placement" });
    expect(placement.querySelector(".board-square-published")).not.toBeNull();
    expect(screen.getByRole("option", { name: "Worker retries, 2 outdated" }).textContent).toContain("2 outdated");
    const draft = screen.getByRole("option", { name: "POST /orders" });
    expect(draft.querySelector(".board-square-draft")).not.toBeNull();
    expect(placement.querySelector(".board-label")).toBeNull();
    expect(screen.getByText("Drafts · on this machine")).toBeTruthy();
  });

  it("creates a new board with +, and focuses the window that holds a board instead of opening it", async () => {
    renderWithClient(<BoardsView />);
    fireEvent.click(screen.getByRole("button", { name: "New Board" }));
    await waitFor(() => {
      expect(engine.callsOf("new_board")).toHaveLength(1);
    });
    fireEvent.click(await screen.findByRole("option", { name: "Worker retries, 2 outdated" }));
    expect(engine.windowCalls).toContainEqual({ command: "focus_window", args: { windowId: 7 } });
    expect(engine.callsOf("open_board")).toEqual([]);
    fireEvent.click(screen.getByRole("option", { name: "Order placement" }));
    await waitFor(() => {
      expect(engine.callsOf("open_board")).toEqual([{ id: ".unfold/boards/order-placement.json" }]);
    });
  });

  it("shows the open board's outline in follow order, its groups, and steps through unlabeled Flow nodes", () => {
    store.adoptBoardState(openState());
    renderWithClient(<BoardsView />);
    const outline = screen.getByRole("heading", { name: "On this board" }).closest("section");
    if (!outline) {
      throw new Error("the outline renders");
    }
    const rows = within(outline).getAllByRole("button").filter((button) => button.classList.contains("boards-outline-row"));
    expect(rows.map((row) => row.textContent)).toEqual(["POST /orders", "createOrder", "OrderService.place"]);
    expect(rows.map((row) => row.style.paddingLeft)).toEqual(["14px", "28px", "42px"]);

    const [, second] = rows;
    if (!second) {
      throw new Error("the outline has a second row");
    }
    fireEvent.click(second);
    expect(store.getBoard(ID)?.view.selectedCardId).toBe("create-order");
    expect(store.getBoard(ID)?.view.focus?.cardIds).toEqual(["create-order"]);

    const stepper = screen.getByRole("button", { name: /2 Flow nodes still show code names/ });
    fireEvent.click(stepper);
    expect(store.getBoard(ID)?.view.selectedKey).toBe(`${CREATE}/variable:draft`);
    expect(useLayout.getState().inspectorOpen).toBe(true);
    fireEvent.click(stepper);
    expect(store.getBoard(ID)?.view.selectedKey).toBe(`${CREATE}/if[0]`);

    // A label written since the last save no longer counts.
    act(() => {
      store.editBoard(ID, (document) => {
        document.labels[`${CREATE}/if[0]`] = { text: "Async?", author: "person:alice", signature: "" };
      });
    });
    expect(screen.getByRole("button", { name: /1 Flow node still shows a code name/ })).toBeTruthy();

    const groups = screen.getByRole("heading", { name: "Groups" }).closest("section");
    expect(groups?.textContent).toContain("Queue or place now");
    expect(groups?.textContent).toContain("createOrder");
  });

  it("lists health problems and repairs one after showing what it does", async () => {
    store.adoptBoardState(openState());
    renderWithClient(
      <>
        <BoardsView />
        <BoardDialogs />
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: /Outdated.*createOrder/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Outdated: createOrder")).toBeTruthy();
    expect(within(dialog).getByText(/Keeps the text of createOrder as written/)).toBeTruthy();
    expect(within(dialog).getByText(/Removes the description of createOrder. The card stays./)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("radio", { name: /Remove the description/ }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove the description" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(engine.callsOf("repair_board")).toEqual([{ id: ID, repair: { kind: "remove_description", target: { card: "create-order" } } }]);
    expect(store.getBoard(ID)?.document.cards[1]?.description).toBeUndefined();
    expect(store.getBoard(ID)?.revision).toBe("r2");
    expect(screen.queryByRole("heading", { name: "Needs attention" })).toBeNull();
  });
});
