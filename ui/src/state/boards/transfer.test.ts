// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument } from "../../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page } from "../../test/workspace";
import { useTabs } from "../tabStore";
import * as store from "./store";
import { installTransferReceiver, moveBoardToNewWindow, parseSession, resetTransfers, setTransfersReady } from "./transfer";
import type { BoardSession } from "./types";

const P = "p-orders0000";
const ID = ".unfold/local/drafts/orders.json";
const REF = { project: P, id: ID };

function documentTitled(title: string): BoardDocument {
  return boardDocument([page("page-1"), page("page-2")], { title });
}

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  resetTransfers();
  useTabs.setState({ boards: [], active: "home" });
});

afterEach(() => {
  store.resetBoards();
  resetTransfers();
  engine.uninstall();
});

describe("Move to New Window", () => {
  it("opens a window, hands it the claim, then the session with its history and views, and forgets the board", async () => {
    engine = installFakeEngine({
      handlers: {
        save_board: ({ board }) => boardState(P, ID, { ...documentTitled(board.title), pages: board.pages as BoardDocument["pages"] }, { revision: "r2" }),
        save_board_view: () => null,
        move_board: () => null,
      },
      windowHandlers: { move_tab_to_new_window: () => ({ windowId: 2 }) },
    });
    store.adoptBoardState(boardState(P, ID, documentTitled("Orders")));
    useTabs.setState({ boards: [REF], active: `${P}\u0000${ID}` });
    store.setActivePage(REF, "page-2");
    store.setCamera({ board: REF, page: "page-2" }, { x: 1, y: 2, zoom: 0.5 });
    store.editBoard(REF, (document) => {
      document.title = "Order placement";
    });
    await moveBoardToNewWindow(REF);

    expect(engine.windowCalls.map((entry) => entry.command)).toEqual(["move_tab_to_new_window", "transfer_board_state"]);
    // Saved first, so nothing is on its way when the claim moves.
    expect(engine.calls.map((entry) => entry.command).sort()).toEqual(["move_board", "save_board", "save_board_view"]);
    expect(engine.callsOf("move_board")).toEqual([{ project: P, id: ID, toWindow: 2 }]);
    const transfer = engine.windowCalls[1]?.args as { toWindow: number; payload: BoardSession };
    expect(transfer.toWindow).toBe(2);
    expect(transfer.payload.document.title).toBe("Order placement");
    expect(transfer.payload.history.past.map((entry) => [entry.document.title, entry.page])).toEqual([["Orders", "page-2"]]);
    expect(transfer.payload.activePage).toBe("page-2");
    expect(transfer.payload.pages["page-2"]).toEqual({ camera: { x: 1, y: 2, zoom: 0.5 }, lens: null });
    expect(store.getOpenBoard(REF)).toBeUndefined();
    expect(useTabs.getState()).toEqual({ boards: [], active: "home" });
  });

  it("adopts a moved board once the window is restored, with its history and views, and saves its unsaved edits", async () => {
    engine = installFakeEngine({ handlers: { save_board: ({ board }) => boardState(P, ID, { ...documentTitled(board.title), pages: board.pages as BoardDocument["pages"] }) } });
    installTransferReceiver();
    setTransfersReady(false);
    const session: BoardSession = {
      state: boardState(P, ID, documentTitled("Orders")),
      document: documentTitled("Order placement"),
      history: { past: [{ document: documentTitled("Orders"), page: "page-2" }], future: [] },
      activePage: "page-2",
      pages: { "page-2": { camera: { x: 1, y: 2, zoom: 0.5 }, lens: "flow" } },
    };
    // Structured cloning, as the main process delivers it.
    engine.emit({ type: "board_state_transferred", fromWindow: 1, payload: structuredClone(session) });
    expect(store.getOpenBoard(REF)).toBeUndefined();

    setTransfersReady(true);
    const board = store.getOpenBoard(REF);
    expect(board?.document.title).toBe("Order placement");
    expect(board?.activePage).toBe("page-2");
    expect(board?.pages["page-2"]).toMatchObject({ camera: { x: 1, y: 2, zoom: 0.5 }, lens: "flow" });
    expect(board && store.isDirty(board)).toBe(true);
    expect(useTabs.getState().active).toBe(`${P}\u0000${ID}`);
    await store.flushBoard(REF);
    expect(engine.callsOf("save_board")).toHaveLength(1);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.title).toBe("Orders");

    // Later sessions are adopted at once; malformed ones are ignored.
    engine.emit({ type: "board_state_transferred", fromWindow: 1, payload: { state: { id: 3 } } });
    expect(useTabs.getState().boards).toEqual([REF]);
  });

  it("recognizes only well-formed sessions", () => {
    const valid = { state: boardState(P, ID), document: boardDocument(), history: { past: [], future: [] }, activePage: "page-1", pages: {} };
    expect(parseSession(valid)).not.toBeNull();
    expect(parseSession({ ...valid, history: { past: [{ document: boardDocument() }], future: [] } })).toBeNull();
    expect(parseSession({ ...valid, history: { past: [boardDocument()], future: [] } })).toBeNull();
    expect(parseSession(null)).toBeNull();
    expect(parseSession({ ...valid, document: { format: "unfold-board", version: 1, cards: [] } })).toBeNull();
    expect(parseSession({ ...valid, state: { ...valid.state, project: 1 } })).toBeNull();
  });
});
