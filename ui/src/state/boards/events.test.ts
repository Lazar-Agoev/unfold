// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { lastVerifiedCode } from "../../canvas/cardData";
import { queryClient } from "../../ipc/client";
import { engineKey } from "../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { HEALTH_DELAY_MS, installBoardEvents } from "./events";
import * as store from "./store";

const P = "p-orders0000";
const Q = "p-payments00";
const R = "p-billing000";
const REF = { project: P, id: ".unfold/boards/a.json" };
const KEY = "src/routes/orders.ts#function:createOrder";

let engine: FakeEngine;
let uninstall: () => void;
/** Why `save_board` fails, while it does. */
let saveFailure: string | null;

beforeEach(() => {
  vi.useFakeTimers();
  store.resetBoards();
  queryClient.clear();
  saveFailure = null;
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders"), projectRow(Q, "payments"), projectRow(R, "billing")]));
  engine = installFakeEngine({
    handlers: {
      board_health: () => ({ health: { ...HEALTHY, missing: 1 }, outlines: [], label: "1 missing" }),
      card: () => fixture("card", "card_create_order"),
      save_board: (args) => {
        if (saveFailure !== null) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "io", message: saveFailure };
        }
        return boardState(P, REF.id, boardDocument([page("page-1")], { title: args.board.title }), { revision: "r2" });
      },
    },
  });
  uninstall = installBoardEvents();
  store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1")])));
});

afterEach(() => {
  uninstall();
  engine.uninstall();
  vi.useRealTimers();
});

describe("board events", () => {
  it("refreshes a board's health once, after a burst of graph updates of its project", async () => {
    const updated = { type: "graph_updated", changedFiles: [], removedFiles: [], changedKeys: [], changedEntrances: [], inputsChanged: false, everything: false, complete: true } as const;
    engine.emit({ ...updated, project: Q });
    await vi.advanceTimersByTimeAsync(HEALTH_DELAY_MS * 2);
    expect(engine.callsOf("board_health")).toEqual([]);
    engine.emit({ ...updated, project: P });
    engine.emit({ ...updated, project: P });
    await vi.advanceTimersByTimeAsync(HEALTH_DELAY_MS);
    expect(engine.callsOf("board_health")).toEqual([{ project: P, id: REF.id }]);
    expect(store.getOpenBoard(REF)?.label).toBe("1 missing");
  });

  it("reloads a board whose file changed outside, and starts its undo history over", async () => {
    store.editBoard(REF, (document) => {
      document.title = "Renamed";
    });
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect(store.canUndo(REF)).toBe(true);
    engine.emit({ type: "board_reloaded", project: P, board: REF.id, state: boardState(P, REF.id, boardDocument([page("page-1")], { title: "From git" }), { revision: "r9" }) });
    await vi.advanceTimersByTimeAsync(0);
    const board = store.getOpenBoard(REF);
    expect(board?.document.title).toBe("From git");
    expect(board?.revision).toBe("r9");
    expect(store.canUndo(REF)).toBe(false);
    expect(board && store.isDirty(board)).toBe(false);
  });

  it("keeps the person's version when the file was deleted outside, and the next save writes it back", async () => {
    engine.emit({ type: "board_reloaded", project: P, board: REF.id, state: null });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order placement");
    store.editBoard(REF, (document) => {
      document.title = "Renamed";
    });
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect(engine.callsOf("save_board")).toMatchObject([{ project: P, id: REF.id, board: { title: "Renamed" } }]);
  });

  it("doesn't reload while the board's save is failing, and Retry writes the person's version", async () => {
    saveFailure = "The disk is full.";
    store.editBoard(REF, (document) => {
      document.title = "Mine";
    });
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect(store.getOpenBoard(REF)?.saveError).toBe("The disk is full.");
    engine.emit({ type: "board_reloaded", project: P, board: REF.id, state: boardState(P, REF.id, boardDocument([page("page-1")], { title: "Theirs" }), { revision: "r9" }) });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getOpenBoard(REF)?.document.title).toBe("Mine");
    expect(store.canUndo(REF)).toBe(true);
    saveFailure = null;
    await store.saveBoard(REF);
    expect(engine.callsOf("save_board").at(-1)).toMatchObject({ project: P, id: REF.id, board: { title: "Mine" } });
    expect(store.getOpenBoard(REF)?.saveError).toBeNull();
  });

  it("forgets a reset project's remembered renders", async () => {
    await queryClient.query((await import("../../canvas/cardData")).cardQuery(Q, KEY));
    expect(lastVerifiedCode(Q, KEY)).toBeDefined();
    engine.emit({ type: "project_reset", project: Q });
    expect(lastVerifiedCode(Q, KEY)).toBeUndefined();
  });

  it("sends unsaved edits when the page goes away", () => {
    store.editBoard(REF, (document) => {
      document.title = "Renamed";
    });
    window.dispatchEvent(new Event("pagehide"));
    expect(engine.callsOf("save_board")).toHaveLength(1);
  });
});
