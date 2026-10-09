// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { engineKey } from "../ipc";
import { adoptBoardState, resetBoards } from "../state/boards/store";
import { openBoardTab, useTabs } from "../state/tabs";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient, testQueryClient } from "../test/render";
import { boardDocument, boardState, projectRow, workspaceView } from "../test/workspace";
import { TabStrip } from "./TabStrip";

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";
const a = { project: ORDERS, id: ".unfold/boards/a.json" };
const b = { project: PAYMENTS, id: ".unfold/boards/b.json" };
const key = (ref: { project: string; id: string }) => `${ref.project}\u0000${ref.id}`;

let engine: FakeEngine;

beforeEach(() => {
  engine = installFakeEngine({ handlers: { close_board: () => null } });
  resetBoards();
  useTabs.setState({ boards: [], active: "home" });
  adoptBoardState(boardState(a.project, a.id, boardDocument(undefined, { title: "Order lifecycle" })));
  adoptBoardState(boardState(b.project, b.id, boardDocument(undefined, { title: "Refunds" })));
  openBoardTab(a, { activate: false });
  openBoardTab(b);
});

afterEach(() => {
  engine.uninstall();
});

function renderStrip() {
  const client = testQueryClient();
  client.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(ORDERS, "storefront-api"), projectRow(PAYMENTS, "payments")]));
  return renderWithClient(<TabStrip />, client);
}

const tab = (name: RegExp | string) => screen.getByRole("tab", { name });

describe("the tab strip", () => {
  it("pins Home first, then a tab per open board with its project's monogram", () => {
    renderStrip();
    const tablist = screen.getByRole("tablist", { name: "Window tabs" });
    expect(within(tablist).getAllByRole("tab").map((element) => element.textContent)).toEqual(["Home", "SOrder lifecycle", "PRefunds"]);
    expect(tab(/Refunds/).getAttribute("aria-selected")).toBe("true");
    expect(tab("Home").getAttribute("aria-selected")).toBe("false");
  });

  it("activates a tab on click", () => {
    renderStrip();
    fireEvent.click(tab(/Order lifecycle/));
    expect(useTabs.getState().active).toBe(key(a));
    fireEvent.click(tab("Home"));
    expect(useTabs.getState().active).toBe("home");
  });

  it("closes a board tab with its ×, and on a middle click", async () => {
    renderStrip();
    fireEvent.click(screen.getByRole("button", { name: "Close Refunds" }));
    await waitFor(() => {
      expect(useTabs.getState().boards).toEqual([a]);
    });
    expect(engine.callsOf("close_board")).toEqual([{ project: PAYMENTS, id: b.id }]);
    const orders = tab(/Order lifecycle/).parentElement;
    if (!orders) {
      throw new Error("the tab has no frame");
    }
    fireEvent(orders, new MouseEvent("auxclick", { bubbles: true, button: 1 }));
    await waitFor(() => {
      expect(useTabs.getState().boards).toEqual([]);
    });
    expect(screen.getAllByRole("tab")).toHaveLength(1);
  });

  it("offers Close, Close other tabs, and Open in new window on a tab's menu", async () => {
    renderStrip();
    fireEvent.contextMenu(tab(/Refunds/));
    const menu = await screen.findByRole("menu", { name: "Tab" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      expect.stringContaining("Close"),
      "Close other tabs",
      "Open in new window",
    ]);
    act(() => {
      fireEvent.click(within(menu).getByRole("menuitem", { name: "Close other tabs" }));
    });
    await waitFor(() => {
      expect(useTabs.getState().boards).toEqual([b]);
    });
  });

  it("moves focus between tabs with the arrow keys, wrapping around", () => {
    renderStrip();
    tab(/Refunds/).focus();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "ArrowRight" });
    expect(document.activeElement).toBe(tab("Home"));
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "End" });
    expect(document.activeElement).toBe(tab(/Refunds/));
  });
});
