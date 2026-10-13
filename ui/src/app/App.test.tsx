// @vitest-environment jsdom
import "../test/dom";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { resetBoards } from "../state/boards/store";
import { initialLayout, useLayout } from "../state/layout";
import { getShownBoard, useShownBoardStore } from "../state/shownBoard";
import { useWindowStore } from "../state/window";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { projectRow, workspaceView } from "../test/workspace";
import { App } from "./App";

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";

let engine: FakeEngine;

beforeEach(() => {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "storefront-api"), projectRow(PAYMENTS, "payments")], { selected: PAYMENTS }),
      select_project: () => null,
      get_settings: () => ({}),
      list_boards: ({ project }) => ({ project, boards: [], emptyFolders: [] }),
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
  useLayout.setState(initialLayout);
  useWindowStore.setState({ info: null, selectedProject: null });
});

afterEach(() => {
  engine.uninstall();
});

describe("the window frame", () => {
  it("shows the sidebar, under the window's buttons, beside the content area, on the workspace's selected project", async () => {
    renderWithClient(<App />);
    expect(screen.queryByRole("tablist")).toBeNull();
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    expect(sidebar.firstElementChild?.className).toBe("app-window-bar");
    expect(document.getElementById("content-area")).toBeTruthy();
    await waitFor(() => {
      expect(engine.callsOf("select_project")).toEqual([{ project: PAYMENTS }]);
    });
    expect(await screen.findByRole("heading", { name: "payments" })).toBeTruthy();
    expect(document.title).toBe("payments — Unfold");
  });

  it("opens Settings from the sidebar's gear", async () => {
    renderWithClient(<App />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByRole("textbox", { name: "Editor command" })).toBeTruthy();
  });

  it("shows a project added elsewhere on Home", async () => {
    renderWithClient(<App />);
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(PAYMENTS);
    });
    act(() => {
      engine.emit({ type: "project_added", project: ORDERS });
    });
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(ORDERS);
    });
    expect(getShownBoard()).toBeNull();
  });

  it("says why a folder from File › Add Project… couldn't be added", async () => {
    renderWithClient(<App />);
    act(() => {
      engine.emit({ type: "project_add_failed", path: "/Users/ada/code/notes.txt", message: "Not a folder." });
    });
    expect(await screen.findByText("Couldn't add ~/code/notes.txt")).toBeTruthy();
    expect(screen.getByText("Not a folder.")).toBeTruthy();
  });

  it("shows another project when the shown one leaves the workspace", async () => {
    renderWithClient(<App />);
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(PAYMENTS);
    });
    act(() => {
      engine.emit({ type: "workspace_changed", workspace: workspaceView([projectRow(ORDERS, "storefront-api")], { selected: PAYMENTS }) });
    });
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(ORDERS);
    });
  });
});
