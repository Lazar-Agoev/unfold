// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { BoardSummary, Card, HealthItem } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { useLayout } from "../state/layout";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardSidebar } from "./BoardSidebar";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#function:createOrder";

const summary: BoardSummary = {
  project: P,
  id: ID,
  name: "b",
  title: "Order placement",
  folder: "Checkout",
  pages: 2,
  cards: 2,
  updatedAtMs: 0,
  label: null,
  error: null,
};

let engine: FakeEngine;

const cards: Card[] = [
  { id: "create-order", ref: { block: KEY }, at: [0, 0] },
  { id: "pay", ref: { block: "src/pay.ts#function:pay" }, at: [600, 0] },
];

function open(items: HealthItem[] = []) {
  const document = boardDocument([
    page("page-1", cards, { title: "Placing an order", links: [{ id: "a-to-pay", from: "create-order", to: "pay" }] }),
    page("page-2", [], { title: "Refunds" }),
  ]);
  store.adoptBoardState(boardState(P, ID, document, { health: { ...HEALTHY, items } }));
}

function renderSidebar() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("list_boards", { project: P }), { project: P, boards: [summary], emptyFolders: [] });
  queryClient.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <BoardSidebar board={REF} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({ handlers: { save_board: () => boardState(P, ID) } });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the board sidebar", () => {
  it("shows the project and folder, and renames the board in place", () => {
    open();
    renderSidebar();
    expect(screen.getByText("orders / Checkout")).not.toBeNull();
    fireEvent.doubleClick(screen.getByRole("heading", { name: "Order placement" }));
    const field = screen.getByRole("textbox", { name: "Board title" });
    fireEvent.change(field, { target: { value: "Order lifecycle" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order lifecycle");
    expect(screen.getByRole("heading", { name: "Order lifecycle" })).not.toBeNull();
  });

  it("keeps the old title on Escape", () => {
    open();
    renderSidebar();
    fireEvent.doubleClick(screen.getByRole("heading", { name: "Order placement" }));
    const field = screen.getByRole("textbox", { name: "Board title" });
    fireEvent.change(field, { target: { value: "Something else" } });
    fireEvent.keyDown(field, { key: "Escape" });
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order placement");
  });

  it("lists the pages with a review mark, switches and adds pages", () => {
    open([{ page: "page-2", kind: "review", target: { card: "x" }, message: "Changed", repairs: [] }]);
    renderSidebar();
    const pages = screen.getByRole("listbox", { name: "Pages of the board" });
    const refunds = within(pages).getByRole("option", { name: /Refunds/ });
    expect(within(refunds).getByLabelText("Needs review")).not.toBeNull();
    act(() => {
      refunds.click();
    });
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    act(() => {
      screen.getByRole("button", { name: "New page" }).click();
    });
    expect(store.getOpenBoard(REF)?.document.pages.map((item) => item.title)).toEqual(["Placing an order", "Refunds", "Page 3"]);
  });

  it("has the drawn page menu: Rename, Copy link, Move up, Move down, then Delete page…", () => {
    open();
    renderSidebar();
    fireEvent.click(screen.getByRole("button", { name: "Refunds actions" }));
    const menu = screen.getByRole("menu");
    expect([...menu.querySelectorAll("[role=menuitem], [role=separator]")].map((item) => (item.getAttribute("role") === "separator" ? "—" : item.textContent))).toEqual([
      "Rename",
      "Copy link",
      "Move up",
      "Move down",
      "—",
      "Delete page…",
    ]);
    expect(within(menu).getByRole("menuitem", { name: "Move down" }).getAttribute("aria-disabled")).toBe("true");
  });

  it("lists the cards and drawn links", () => {
    open();
    renderSidebar();
    const list = screen.getByRole("listbox", { name: "Cards on this page" });
    const rows = within(list).getAllByRole("option");
    expect(rows.map((row) => row.textContent)).toEqual(["ƒcreateOrder", "link to pay", "ƒpay"]);
    act(() => {
      rows[1]?.click();
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "a-to-pay" });
  });

  it("shows the selected link in Selected, edits its label, and removes it", () => {
    open();
    renderSidebar();
    act(() => {
      store.select(PAGE, { kind: "link", linkId: "a-to-pay" });
    });
    const selected = screen.getByRole("region", { name: "Selected" });
    const label = within(selected).getByLabelText("Label");
    fireEvent.change(label, { target: { value: "calls over HTTP" } });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.links[0]?.label).toEqual({ text: "calls over HTTP", author: "person:lazar", signature: "" });
    act(() => {
      within(selected).getByRole("button", { name: "Remove link" }).click();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.links).toEqual([]);
  });

  it("collapses Selected, remembered in the window layout", () => {
    open();
    renderSidebar();
    expect(screen.getByText("Select a card, a group, a frame, a resource card, a note, or a link to see it here.")).not.toBeNull();
    act(() => {
      screen.getByRole("button", { name: "Hide Selected" }).click();
    });
    expect(useLayout.getState().selectedCollapsed).toBe(true);
    expect(screen.queryByText("Select a card, a group, a frame, a resource card, a note, or a link to see it here.")).toBeNull();
    act(() => {
      screen.getByRole("button", { name: "Show Selected" }).click();
    });
  });
});
