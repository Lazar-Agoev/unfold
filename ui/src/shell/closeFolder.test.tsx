// @vitest-environment jsdom
import "../test/dom";
import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument, BoardState, GraphStatus } from "../ipc/generated";
import { editBoard, openBoard, resetBoards } from "../state/boards/store";
import { useWindow } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { engineKey } from "../ipc/queries";
import { renderWithClient, testQueryClient } from "../test/render";
import { Shell } from "./Shell";

const FOLDER = "/Users/someone/code/orders-service";
const BOARD = ".unfold/local/drafts/order-placement.json";

const status: GraphStatus = {
  state: { kind: "not_compiled" },
  fallback: null,
  watching: "events",
  backgroundUpdates: true,
  complete: false,
};

function boardState(): BoardState {
  const document: BoardDocument = {
    format: "unfold-board",
    version: 1,
    title: "Order placement",
    defaultView: "code",
    cards: [],
    groups: [],
    labels: {},
    guards: {},
  };
  return {
    id: BOARD,
    place: "draft",
    name: "order-placement",
    document,
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

let engine: FakeEngine;
/** Board saves and window commands, in the order they reached the main process or engine. */
let order: string[];

beforeEach(() => {
  order = [];
  resetBoards();
  useWindow.setState({ info: { windowId: 1, key: "w1", folder: FOLDER, platform: "darwin" } });
  engine = installFakeEngine({
    handlers: {
      graph_status: () => status,
      folder_summary: () => fixture("folder_summary", "folder_summary"),
      recent_folders: () => ({
        folders: useWindow.getState().info?.folder === null ? [{ path: FOLDER, name: "orders-service", openedAtMs: Date.now() }] : [],
        welcomeBounds: null,
      }),
      open_board: () => ({ status: "opened", board: boardState() }),
      save_board: ({ board }) => {
        order.push("save_board");
        return { ...boardState(), document: board, revision: "r2" };
      },
    },
    windowHandlers: {
      close_folder: () => {
        order.push("close_folder");
        return { windowKey: "w2" };
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
  resetBoards();
});

describe("Close Folder", () => {
  it("writes unsaved board edits, then has the main process close the folder, and shows the welcome screen", async () => {
    // The recent list as read on the welcome screen before this folder opened.
    const client = testQueryClient();
    client.setQueryData(engineKey("recent_folders", {}), { folders: [], welcomeBounds: null });
    renderWithClient(<Shell />, client);
    await screen.findByRole("button", { name: /^Compile/ });
    expect(screen.queryByText("See how a codebase starts, and follow where it goes.")).toBeNull();
    await act(async () => {
      await openBoard(BOARD);
      editBoard(BOARD, (document) => {
        document.title = "Order placement (edited)";
      });
    });

    act(() => {
      engine.emit({ type: "menu_command", command: "close_folder" });
    });

    await screen.findByText("See how a codebase starts, and follow where it goes.");
    expect(order).toEqual(["save_board", "close_folder"]);
    // The recent list is read again, so it now lists the folder just closed.
    await screen.findByTitle(FOLDER);
    expect(useWindow.getState().info).toMatchObject({ folder: null, key: "w2" });
    await waitFor(() => {
      expect(document.title).toBe("Unfold");
    });
  });
});
