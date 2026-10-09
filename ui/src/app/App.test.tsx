// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Preferences } from "../ipc";
import { ActionIds, actions } from "../keyboard";
import { resetBoards } from "../state/boards/store";
import { initialLayout, useLayout } from "../state/layout";
import { useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { projectRow, workspaceView } from "../test/workspace";
import { App } from "./App";
import { GLOBAL_ACTIONS } from "./actions";

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";
const SHOP = "p-shop000000";

const PREFERENCES: Preferences = {
  theme: "system",
  editorCommand: null,
  backgroundUpdates: true,
  threadLimit: null,
  keyBindings: {},
  updates: { enabled: true },
};

let engine: FakeEngine;

beforeEach(() => {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "storefront-api"), projectRow(PAYMENTS, "payments")], { selected: PAYMENTS }),
      select_project: () => null,
      get_preferences: () => PREFERENCES,
      list_boards: ({ project }) => ({ project, boards: [], emptyFolders: [] }),
    },
  });
  resetBoards();
  useTabs.setState({ boards: [], active: "home" });
  useLayout.setState(initialLayout);
  useWindowStore.setState({ info: null, selectedProject: null });
});

afterEach(() => {
  engine.uninstall();
});

describe("the window frame", () => {
  it("shows the tab strip, the sidebar, and the content area, on the workspace's selected project", async () => {
    renderWithClient(<App />);
    expect(screen.getByRole("tablist", { name: "Window tabs" })).toBeTruthy();
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    expect(sidebar.style.width).toBe("264px");
    expect(document.getElementById("content-area")).toBeTruthy();
    await waitFor(() => {
      expect(engine.callsOf("select_project")).toEqual([{ project: PAYMENTS }]);
    });
    expect(await screen.findByRole("heading", { name: "payments" })).toBeTruthy();
    expect(document.title).toBe("payments — Unfold");
  });

  it("registers the frame's actions while mounted", () => {
    const { unmount } = renderWithClient(<App />);
    for (const { id } of GLOBAL_ACTIONS) {
      expect(actions.get(id)).toBeDefined();
    }
    unmount();
    expect(actions.get(ActionIds.toggleSidebar)).toBeUndefined();
  });

  it("hides and shows the sidebar with Toggle Sidebar", () => {
    renderWithClient(<App />);
    act(() => {
      actions.run(ActionIds.toggleSidebar);
    });
    expect(screen.queryByRole("complementary", { name: "Sidebar" })).toBeNull();
    act(() => {
      actions.run(ActionIds.toggleSidebar);
    });
    expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeTruthy();
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
    expect(useTabs.getState().active).toBe("home");
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

  it("adds a folder dropped anywhere on the window, and says when a drop isn't a folder", async () => {
    const dropped: string[] = [];
    const bridge = window.unfold;
    if (!bridge) {
      throw new Error("no fake bridge");
    }
    Object.assign(bridge, {
      addDroppedFolder: (file: { name: string }) => {
        dropped.push(file.name);
        return file.name === "shop"
          ? Promise.resolve({ status: "added", project: SHOP })
          : // Rejections are plain {code, message} data, as from the real preload.
            // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
            Promise.reject({ code: "not_a_folder", message: "Not a folder." });
      },
    });
    renderWithClient(<App />);
    const drop = (names: string[]) => {
      fireEvent.drop(screen.getByRole("tablist", { name: "Window tabs" }), {
        dataTransfer: { types: ["Files"], files: names.map((name) => ({ name })) },
      });
    };
    drop(["notes.txt", "shop"]);
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(SHOP);
    });
    expect(dropped).toEqual(["notes.txt", "shop"]);
    expect(await screen.findByText("notes.txt isn't a folder")).toBeTruthy();
  });
});
