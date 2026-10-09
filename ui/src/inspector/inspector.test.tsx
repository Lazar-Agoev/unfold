// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument, BoardState, Relations } from "../ipc/generated";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { Inspector } from "./Inspector";

const ID = ".unfold/local/drafts/orders.json";
const KEY = "src/routes/orders.ts#function:createOrder";
const IF = `${KEY}/if[0]`;
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const VALIDATE = "src/services/validate.ts#function:validateOrder";

function boardState(document: Partial<BoardDocument> = {}): BoardState {
  return {
    id: ID,
    place: "draft",
    name: "orders",
    document: {
      format: "unfold-board",
      version: 1,
      title: "Orders",
      defaultView: "code",
      cards: [
        { id: "create-order", ref: { block: KEY }, at: [0, 0] },
        { id: "place", ref: { block: PLACE }, at: [600, 0] },
      ],
      groups: [],
      labels: {},
      guards: {},
      ...document,
    },
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

function relations(): Relations {
  const base = fixture("relations", "relations_create_order");
  return {
    ...base,
    outgoing: {
      onBoard: [{ source: `${KEY}/variable:order/call[0]`, seq: 0, kind: "calls", resolution: "resolved", target: PLACE, text: "orderService.place", line: 32 }],
      openable: [{ source: `${KEY}/call[9]`, seq: 0, kind: "calls", resolution: "resolved", target: VALIDATE, text: "validateOrder", line: 26 }],
      externalOrUnresolved: [
        { source: `${KEY}/expression_statement[0]/call[0]`, seq: 0, kind: "calls", resolution: "external", text: "slack.post", module: "@slack/web-api", line: 33 },
        { source: `${KEY}/if[0]/call[0]`, seq: 0, kind: "calls", resolution: "unresolved", text: "req.header", reason: "unsupported", note: "unsupported form", line: 27 },
      ],
    },
    targetNames: { [PLACE]: "OrderService.place", [VALIDATE]: "validateOrder" },
    callers: [{ owner: "src/server.ts#", ownerName: "src/server.ts", source: "src/server.ts#call[0]", path: "src/server.ts", line: 9, kind: "references", position: 1 }],
    callersTotal: 1,
    reachedFrom: [{ key: 'entrance:["express.route","http","POST","/orders"]', title: "POST /orders", kind: "http", method: "POST", trigger: "/orders", path: "src/routes/orders.ts" }],
  };
}

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  engine = installFakeEngine({
    handlers: {
      app_info: () => ({ username: "alice", engineVersion: "0.1.0" }),
      card: () => fixture("card", "card_create_order"),
      relations: () => relations(),
      save_board: ({ board }) => ({ ...boardState(), document: board }),
    },
  });
  store.adoptBoardState(boardState());
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the inspector", () => {
  it("asks for a selection until a card is selected", () => {
    renderWithClient(<Inspector />);
    expect(screen.getByText("Select a card or a Flow node to see its details.")).toBeTruthy();
  });

  it("edits a card's description signed by the person, one undo step per burst, and lists its relations", async () => {
    act(() => {
      store.selectCard(ID, "create-order");
    });
    renderWithClient(<Inspector />);
    const field = await screen.findByRole("textbox", { name: "Description" });
    await waitFor(() => {
      expect((field as HTMLInputElement).disabled).toBe(false);
    });
    fireEvent.change(field, { target: { value: "Places" } });
    fireEvent.change(field, { target: { value: "Places the order" } });
    expect(store.getBoard(ID)?.document.cards[0]?.description).toEqual({ text: "Places the order", author: "person:alice", signature: "" });
    expect(store.getBoard(ID)?.history.past).toHaveLength(1);
    expect(screen.getByText("by alice")).toBeTruthy();

    act(() => {
      store.undo(ID);
    });
    expect(store.getBoard(ID)?.document.cards[0]?.description).toBeUndefined();
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Description" }).value).toBe("");
    act(() => {
      store.redo(ID);
    });
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Description" }).value).toBe("Places the order");

    // Calls: on the board, + Open, external with its module, unresolved with its reason.
    const calls = (await screen.findByRole("heading", { name: /^Calls/ })).closest("section");
    if (!calls) {
      throw new Error("the Calls section renders");
    }
    expect(within(calls).getByText("OrderService.place")).toBeTruthy();
    expect(within(calls).getByRole("button", { name: "Open validateOrder on the board" })).toBeTruthy();
    expect(within(calls).getByText("@slack/web-api")).toBeTruthy();
    expect(within(calls).getByText("unsupported form")).toBeTruthy();
    expect(screen.getByText("/orders")).toBeTruthy();
    expect(screen.getByText("src/server.ts")).toBeTruthy();
    expect(engine.callsOf("relations")[0]).toEqual({ key: KEY, onBoard: [PLACE, KEY].sort() });

    fireEvent.click(within(calls).getByRole("button", { name: "Open validateOrder on the board" }));
    expect(store.getBoard(ID)?.document.cards.map((card) => card.ref)).toContainEqual({ block: VALIDATE });
  });

  it("opens to several lines with more", async () => {
    act(() => {
      store.selectCard(ID, "create-order");
    });
    renderWithClient(<Inspector />);
    const field = await screen.findByRole("textbox", { name: "Description" });
    expect(field.tagName).toBe("INPUT");
    fireEvent.click(screen.getByRole("button", { name: "more" }));
    expect(screen.getByRole("textbox", { name: "Description" }).tagName).toBe("TEXTAREA");
  });

  it("flags an outdated description and offers its repairs", async () => {
    store.resetBoards();
    store.adoptBoardState({
      ...boardState({ cards: [{ id: "create-order", ref: { block: KEY }, at: [0, 0], description: { text: "Old words", author: "agent:claude", signature: `sha256:${"0".repeat(64)}` } }] }),
      health: {
        items: [{ kind: "outdated", target: { card: "create-order" }, message: "the code changed since this was written", repairs: ["resign", "removeDescription"] }],
        missing: 0,
        outdated: 1,
        brokenGroups: 0,
        review: 0,
      },
    });
    act(() => {
      store.selectCard(ID, "create-order");
    });
    renderWithClient(<Inspector />);
    expect(await screen.findByText("Outdated: the code changed since this was written")).toBeTruthy();
    expect(screen.getByText("by agent claude")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Review…" })).toBeTruthy();
  });

  it("labels a Flow node, showing the name from the code it replaces and its yes and no paths", async () => {
    act(() => {
      store.selectBlock(ID, "create-order", IF);
    });
    renderWithClient(<Inspector />);
    const field = await screen.findByRole("textbox", { name: "Flow label" });
    expect((field as HTMLInputElement).placeholder).toBe("req.header?");
    expect(screen.getByText("Replaces the name from the code:")).toBeTruthy();
    expect(screen.getByText("condition", { exact: false })).toBeTruthy();
    const paths = screen.getByRole("heading", { name: "Paths" }).closest("section");
    expect(paths?.textContent).toContain("Yes");
    expect(paths?.textContent).toContain("ordersQueue.add");
    expect(paths?.textContent).toContain("No");
    // The No path ends at the place() call (its name is the target's or the written callee's).
    expect(paths?.textContent.toLowerCase()).toContain("orderservice.place");
    await waitFor(() => {
      expect((field as HTMLInputElement).disabled).toBe(false);
    });

    fireEvent.change(field, { target: { value: "Did the client ask for an async reply?" } });
    expect(store.getBoard(ID)?.document.labels[IF]).toEqual({ text: "Did the client ask for an async reply?", author: "person:alice", signature: "" });
    act(() => {
      store.undo(ID);
    });
    expect(store.getBoard(ID)?.document.labels[IF]).toBeUndefined();
    act(() => {
      store.redo(ID);
    });
    expect(store.getBoard(ID)?.document.labels[IF]?.text).toBe("Did the client ask for an async reply?");

    // A path selects the node it leads to.
    fireEvent.click(screen.getByRole("button", { name: /^No/ }));
    expect(store.getBoard(ID)?.view.selectedKey).toBe(`${KEY}/variable:order`);
  });

  it("edits a group's title and description, collapses it, and ungroups it", async () => {
    store.resetBoards();
    store.adoptBoardState(
      boardState({
        groups: [{ id: "sync", card: "create-order", title: "Place now", members: [`${KEY}/variable:order`], collapsed: false }],
      }),
    );
    act(() => {
      store.selectCard(ID, "create-order");
    });
    renderWithClient(<Inspector />);
    const title = await screen.findByRole("textbox", { name: "Title of the group Place now" });
    fireEvent.change(title, { target: { value: "Place and reply" } });
    expect(store.getBoard(ID)?.document.groups[0]?.title).toBe("Place and reply");
    await waitFor(() => {
      expect((screen.getAllByRole("textbox", { name: "Description" })[1] as HTMLInputElement).disabled).toBe(false);
    });
    const [, groupDescription] = screen.getAllByRole("textbox", { name: "Description" });
    if (!groupDescription) {
      throw new Error("the group's description field renders");
    }
    fireEvent.change(groupDescription, { target: { value: "The synchronous path" } });
    expect(store.getBoard(ID)?.document.groups[0]?.description?.text).toBe("The synchronous path");
    fireEvent.click(screen.getByRole("button", { name: "Collapse" }));
    expect(store.getBoard(ID)?.document.groups[0]?.collapsed).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Ungroup" }));
    expect(store.getBoard(ID)?.document.groups).toEqual([]);
    act(() => {
      store.undo(ID);
    });
    expect(store.getBoard(ID)?.document.groups).toHaveLength(1);
  });
});
