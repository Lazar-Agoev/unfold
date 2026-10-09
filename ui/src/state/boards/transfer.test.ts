// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument, BoardState } from "../../ipc/generated";
import { useTabs } from "../tabs";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import * as store from "./store";
import { installTransferReceiver, moveBoardToNewWindow, parseSession, resetTransfers, setTransfersReady } from "./transfer";
import type { BoardSession } from "./types";

const ID = ".unfold/local/drafts/orders.json";

function boardState(title = "Orders"): BoardState {
  const document: BoardDocument = { format: "unfold-board", version: 1, title, defaultView: "code", cards: [], groups: [], labels: {}, guards: {} };
  return {
    id: ID,
    place: "draft",
    name: "orders",
    document,
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  resetTransfers();
  useTabs.setState({ tabs: [], activeId: null });
});

afterEach(() => {
  store.resetBoards();
  resetTransfers();
  engine.uninstall();
});

describe("Move to New Window", () => {
  it("opens a window, hands it the claim, then the session with its undo history, and forgets the board", async () => {
    engine = installFakeEngine({
      handlers: { save_board: ({ board }) => ({ ...boardState(), document: board, revision: "r2" }), move_board: () => null },
      windowHandlers: { move_tab_to_new_window: () => ({ windowId: 2 }) },
    });
    store.adoptBoardState(boardState());
    store.editBoard(ID, (document) => {
      document.title = "Order placement";
    });
    await moveBoardToNewWindow(ID);

    expect(engine.windowCalls.map((entry) => entry.command)).toEqual(["move_tab_to_new_window", "transfer_board_state"]);
    // Saved first, so nothing is on its way when the claim moves.
    expect(engine.calls.map((entry) => entry.command)).toEqual(["save_board", "move_board"]);
    expect(engine.callsOf("move_board")).toEqual([{ id: ID, toWindow: 2 }]);
    const transfer = engine.windowCalls[1]?.args as { toWindow: number; payload: BoardSession };
    expect(transfer.toWindow).toBe(2);
    expect(transfer.payload.document.title).toBe("Order placement");
    expect(transfer.payload.history.past.map((document) => document.title)).toEqual(["Orders"]);
    expect(store.getBoard(ID)).toBeUndefined();
  });

  it("adopts a moved board once the window is restored, with its history, and opens its tab", () => {
    engine = installFakeEngine({ handlers: { save_board: ({ board }) => ({ ...boardState(), document: board }) } });
    installTransferReceiver();
    setTransfersReady(false);
    const session: BoardSession = {
      state: boardState("Order placement"),
      document: boardState("Order placement").document,
      history: { past: [boardState("Orders").document], future: [] },
    };
    // Structured cloning, as the main process delivers it.
    engine.emit({ type: "board_state_transferred", fromWindow: 1, payload: structuredClone(session) });
    expect(store.getBoard(ID)).toBeUndefined();

    setTransfersReady(true);
    expect(store.getBoard(ID)?.document.title).toBe("Order placement");
    expect(useTabs.getState().activeId).toBe(`board:${ID}`);
    expect(store.canUndo(ID)).toBe(true);
    store.undo(ID);
    expect(store.getBoard(ID)?.document.title).toBe("Orders");

    // Later sessions are adopted at once; malformed ones are ignored.
    engine.emit({ type: "board_state_transferred", fromWindow: 1, payload: { state: { id: 3 } } });
    expect(store.useBoards.getState().order).toEqual([ID]);
  });

  it("recognizes only well-formed sessions", () => {
    const valid = { state: boardState(), document: boardState().document, history: { past: [], future: [] } };
    expect(parseSession(valid)).not.toBeNull();
    expect(parseSession({ ...valid, history: { past: [{}], future: [] } })).toBeNull();
    expect(parseSession(null)).toBeNull();
    expect(parseSession({ ...valid, document: { format: "codegraph-board" } })).toBeNull();
  });
});
