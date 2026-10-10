// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "../components";
import type { BoardDocument, SavedWindow } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { testQueryClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { boardKey, editBoard, getOpenBoard } from "./boards";
import { resetBoards } from "./boards/store";
import { resetTransfers } from "./boards/transfer";
import { initialLayout, toggleSidebar, useLayout } from "./layout";
import { installSession, resetSession, SAVE_DELAY_MS } from "./session";
import { useTabs } from "./tabs";
import { useWindowStore } from "./window";

const P = "p-orders0000";
const Q = "p-payments00";
const GONE = "p-gone000000";
const A = { project: P, id: ".unfold/boards/a.json" };
const B = { project: Q, id: ".unfold/boards/b.json" };
const C = { project: P, id: ".unfold/boards/c.json" };
const KEY = "window-key";

function saved(overrides: Partial<SavedWindow> = {}): SavedWindow {
  return {
    key: KEY,
    bounds: null,
    maximized: false,
    selectedProject: Q,
    tabs: [
      { project: A.project, board: A.id, page: "page-2" },
      { project: B.project, board: B.id, page: null },
      { project: C.project, board: C.id, page: null },
    ],
    activeTab: 1,
    layout: { v: 1, sidebar: { width: 300, visible: false }, home: { view: "health" } },
    ...overrides,
  };
}

function document(): BoardDocument {
  return boardDocument([page("page-1"), page("page-2")]);
}

let engine: FakeEngine;
let uninstall: (() => void) | null = null;

/** The engine commands run so far, by name (the workspace query left out). */
function commands(): string[] {
  return engine.calls.map((entry) => entry.command).filter((command) => command !== "workspace");
}

function setup(window: SavedWindow | null, options: { held?: readonly string[]; slow?: string; failing?: string } = {}): void {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(P, "orders"), projectRow(Q, "payments")]),
      window_state: () => window,
      save_window_state: () => null,
      select_project: () => null,
      // The second tab answers first: tabs still come back in their saved order.
      open_board: async ({ project, id }) => {
        if (id === options.failing) {
          throw new Error("The board's file can't be read.");
        }
        if (id === options.slow) {
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        return options.held?.includes(id) ? { status: "held", heldBy: 9 } : { status: "opened", board: boardState(project, id, document()) };
      },
      save_board: ({ project, id, board }) => boardState(project, id, { ...document(), title: board.title }, { revision: "r2" }),
    },
  });
  useWindowStore.setState({ info: { windowId: 1, key: KEY, platform: "darwin" }, selectedProject: null });
}

async function install(): Promise<void> {
  await act(async () => {
    uninstall = installSession(testQueryClient());
    await vi.waitFor(() => {
      expect(engine.callsOf("window_state")).toHaveLength(1);
    });
  });
  // Let the restore finish.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 40));
  });
}

beforeEach(() => {
  resetSession();
  resetBoards();
  resetTransfers();
  useTabs.setState({ boards: [], active: "home" });
  useLayout.setState(initialLayout);
});

afterEach(() => {
  uninstall?.();
  uninstall = null;
  engine.uninstall();
  vi.restoreAllMocks();
});

describe("restoring a window", () => {
  it("brings back the layout, the selection, then the tabs on their pages in order with the saved one active", async () => {
    setup(saved(), { slow: A.id });
    await install();
    expect(commands()).toEqual(["window_state", "select_project", "open_board", "open_board", "open_board"]);
    expect(engine.callsOf("window_state")).toEqual([{ key: KEY }]);
    expect(engine.callsOf("select_project")).toEqual([{ project: Q }]);
    expect(useLayout.getState().sidebar).toEqual({ width: 300, visible: false });
    expect(useLayout.getState().home.view).toBe("health");
    expect(useTabs.getState()).toEqual({ boards: [A, B, C], active: boardKey(B) });
    expect(getOpenBoard(A)?.activePage).toBe("page-2");
    expect(getOpenBoard(B)?.activePage).toBe("page-1");
  });

  it("drops a tab whose board is held elsewhere or can't open, and a selected project that left the workspace", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    setup(saved({ selectedProject: GONE, activeTab: 0 }), { held: [B.id], failing: C.id });
    await install();
    expect(engine.callsOf("select_project")).toEqual([]);
    expect(useWindowStore.getState().selectedProject).toBeNull();
    expect(useTabs.getState()).toEqual({ boards: [A], active: boardKey(A) });
  });

  it("starts empty when nothing was saved", async () => {
    setup(null);
    await install();
    expect(commands()).toEqual(["window_state"]);
    expect(useTabs.getState()).toEqual({ boards: [], active: "home" });
  });
});

describe("saving the window's state", () => {
  it("saves the selection, tabs, active pages, and layout 400 ms after a change, and at once when the page goes away", async () => {
    setup(saved());
    await install();
    expect(engine.callsOf("save_window_state")).toEqual([]);

    act(() => {
      toggleSidebar();
    });
    expect(engine.callsOf("save_window_state")).toEqual([]);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, SAVE_DELAY_MS + 50));
    });
    expect(engine.callsOf("save_window_state")).toEqual([
      {
        state: {
          key: KEY,
          selectedProject: Q,
          tabs: [
            { project: P, board: A.id, page: "page-2" },
            { project: Q, board: B.id, page: "page-1" },
            { project: P, board: C.id, page: "page-1" },
          ],
          activeTab: 1,
          layout: { ...initialLayout, sidebar: { width: 300, visible: true }, home: { ...initialLayout.home, view: "health" } },
        },
      },
    ]);

    act(() => {
      useWindowStore.setState({ selectedProject: P });
      window.dispatchEvent(new Event("pagehide"));
    });
    expect(engine.callsOf("save_window_state")).toHaveLength(2);
    expect(engine.callsOf("save_window_state")[1]).toMatchObject({ state: { selectedProject: P } });
  });
});

describe("an engine restart", () => {
  it("shows the toast, then selects, reclaims the tabs, saves edits and the window, resets the cache, and hides the toast", async () => {
    setup(saved({ tabs: [{ project: A.project, board: A.id, page: null }], activeTab: 0 }));
    const client = testQueryClient();
    const reset = vi.spyOn(client, "resetQueries");
    render(<Toaster />);
    await act(async () => {
      uninstall = installSession(client);
      await new Promise((resolve) => setTimeout(resolve, 40));
    });
    engine.calls.length = 0;

    act(() => {
      engine.setConnection("reconnecting");
    });
    expect(screen.getByText("Reconnecting to the engine…")).toBeTruthy();
    expect(screen.getByText("Your boards stay open. Edits are saved once it's back.")).toBeTruthy();
    // An edit while the engine is away stays unsaved until it's back.
    act(() => {
      editBoard(A, (draft) => {
        draft.title = "Order flow";
      });
    });

    await act(async () => {
      engine.setConnection("connected");
      await vi.waitFor(() => {
        expect(reset).toHaveBeenCalled();
      });
    });
    const order = commands();
    expect(order.slice(0, 2)).toEqual(["select_project", "open_board"]);
    expect(order.indexOf("save_board")).toBeGreaterThan(order.indexOf("open_board"));
    expect(order.lastIndexOf("save_window_state")).toBeGreaterThan(order.indexOf("save_board"));
    expect(engine.callsOf("select_project")).toEqual([{ project: Q }]);
    expect(engine.callsOf("save_board")[0]).toMatchObject({ project: P, id: A.id, board: { title: "Order flow" } });
    await vi.waitFor(() => {
      expect(screen.queryByText("Reconnecting to the engine…")).toBeNull();
    });
  });
});

describe("focus_board", () => {
  it("shows the tab of the board another window asked for", async () => {
    setup(saved({ activeTab: null }));
    await install();
    expect(useTabs.getState().active).toBe("home");
    act(() => {
      engine.emit({ type: "focus_board", project: C.project, board: C.id });
    });
    expect(useTabs.getState().active).toBe(boardKey(C));
  });
});
