// @vitest-environment jsdom
import type { ShownBoard } from "@unfold/desktop/unfold-api";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "../components";
import type { BoardDocument } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { testQueryClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { getOpenBoard, openBoard } from "./boards";
import { editBoard, resetBoards } from "./boards/store";
import { installSession, resetSession } from "./session";
import { getShownBoard, showHome, useShownBoardStore } from "./shownBoard";
import { getShowAllCalls, setShowAllCalls } from "./showAllCalls";
import { useWindowStore } from "./window";

const P = "p-orders0000";
const Q = "p-payments00";
const A = { project: P, id: ".unfold/boards/a.json" };
const B = { project: Q, id: ".unfold/boards/b.json" };

function document(): BoardDocument {
  return boardDocument([page("page-1"), page("page-2")]);
}

let engine: FakeEngine;
let uninstall: (() => void) | null = null;

/** The engine commands run so far, by name (the workspace query left out). */
function commands(): string[] {
  return engine.calls.map((entry) => entry.command).filter((command) => command !== "workspace");
}

/** What the window reported to the main process, in order. */
function reports(): unknown[] {
  return engine.windowCalls.filter((entry) => entry.command === "report_shown_board").map((entry) => entry.args);
}

/** What the window reported about Show all calls, in order. */
function switchReports(): unknown[] {
  return engine.windowCalls.filter((entry) => entry.command === "report_show_all_calls").map((entry) => entry.args);
}

function setup(shown: ShownBoard | null, options: { gone?: string; showAllCalls?: boolean } = {}): void {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(P, "orders"), projectRow(Q, "payments")]),
      select_project: () => null,
      open_board: ({ project, id }) => {
        if (id === options.gone) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such board." };
        }
        return boardState(project, id, document());
      },
      save_board: ({ project, id, board }) => boardState(project, id, { ...document(), title: board.title }, { revision: "r2" }),
    },
  });
  useWindowStore.setState({ info: { windowId: 1, board: shown, showAllCalls: options.showAllCalls ?? false }, selectedProject: Q });
}

async function install(client = testQueryClient()): Promise<void> {
  await act(async () => {
    uninstall = installSession(client);
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
}

beforeEach(() => {
  resetSession();
  resetBoards();
  setShowAllCalls(false);
  useShownBoardStore.setState({ board: null });
});

afterEach(() => {
  uninstall?.();
  uninstall = null;
  engine.uninstall();
  vi.restoreAllMocks();
});

describe("restoring the window", () => {
  it("opens and shows the board it showed last, then reports it", async () => {
    setup({ project: A.project, board: A.id });
    await install();
    expect(commands()).toEqual(["open_board"]);
    expect(getShownBoard()).toEqual(A);
    expect(getOpenBoard(A)?.activePage).toBe("page-1");
    expect(reports()).toEqual([{ board: { project: A.project, board: A.id } }]);
  });

  it("shows Home when that board is gone, and reports Home", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    setup({ project: A.project, board: A.id }, { gone: A.id });
    await install();
    expect(getShownBoard()).toBeNull();
    expect(getOpenBoard(A)).toBeUndefined();
    expect(reports()).toEqual([{ board: null }]);
  });

  it("opens nothing when it showed Home", async () => {
    setup(null);
    await install();
    expect(commands()).toEqual([]);
    expect(getShownBoard()).toBeNull();
    expect(reports()).toEqual([{ board: null }]);
  });

  it("then reports each change of what it shows, and stops when uninstalled", async () => {
    setup(null);
    await install();
    await act(async () => {
      await openBoard(B);
    });
    act(() => {
      showHome();
      showHome();
    });
    expect(reports()).toEqual([{ board: null }, { board: { project: Q, board: B.id } }, { board: null }]);
    uninstall?.();
    uninstall = null;
    await act(async () => {
      await openBoard(B);
    });
    expect(reports()).toHaveLength(3);
  });

  it("puts Show all calls back as the window had it, without reporting what it was told", async () => {
    setup(null, { showAllCalls: true });
    await install();
    expect(getShowAllCalls()).toBe(true);
    expect(switchReports()).toEqual([]);
  });

  it("then reports each change of Show all calls, and stops when uninstalled", async () => {
    setup(null);
    await install();
    expect(getShowAllCalls()).toBe(false);
    act(() => {
      setShowAllCalls(true);
      setShowAllCalls(true);
      setShowAllCalls(false);
    });
    expect(switchReports()).toEqual([{ on: true }, { on: false }]);
    uninstall?.();
    uninstall = null;
    act(() => {
      setShowAllCalls(true);
    });
    expect(switchReports()).toHaveLength(2);
  });
});

describe("an engine restart", () => {
  it("shows the toast, then selects, opens the boards again, saves their edits, resets the cache, and hides the toast", async () => {
    setup({ project: A.project, board: A.id });
    const client = testQueryClient();
    const reset = vi.spyOn(client, "resetQueries");
    render(<Toaster />);
    await install(client);
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
    expect(engine.callsOf("select_project")).toEqual([{ project: Q }]);
    expect(engine.callsOf("save_board")[0]).toMatchObject({ project: P, id: A.id, board: { title: "Order flow" } });
    await vi.waitFor(() => {
      expect(screen.queryByText("Reconnecting to the engine…")).toBeNull();
    });
  });
});
