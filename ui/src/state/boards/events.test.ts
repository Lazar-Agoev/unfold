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

beforeEach(() => {
  vi.useFakeTimers();
  store.resetBoards();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders"), projectRow(Q, "payments"), projectRow(R, "billing")]));
  engine = installFakeEngine({
    handlers: {
      board_health: () => ({ health: { ...HEALTHY, missing: 1 }, outlines: [], label: "1 missing" }),
      reload_board: () => boardState(P, REF.id, boardDocument(), { revision: "r9" }),
      card: () => fixture("card", "card_create_order"),
    },
  });
  uninstall = installBoardEvents();
  // A board of P with a card from payments (Q).
  store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1")], { projects: ["payments"] })));
});

afterEach(() => {
  uninstall();
  engine.uninstall();
  vi.useRealTimers();
});

describe("board events", () => {
  it("refreshes a board's health once, after a burst of graph updates of a project its cards resolve to", async () => {
    const updated = { type: "graph_updated", changedFiles: [], removedFiles: [], changedKeys: [], changedEntrances: [], inputsChanged: false, everything: false, complete: true } as const;
    engine.emit({ ...updated, project: R });
    await vi.advanceTimersByTimeAsync(HEALTH_DELAY_MS * 2);
    expect(engine.callsOf("board_health")).toEqual([]);
    engine.emit({ ...updated, project: Q });
    engine.emit({ ...updated, project: P });
    engine.emit({ ...updated, project: Q });
    await vi.advanceTimersByTimeAsync(HEALTH_DELAY_MS);
    expect(engine.callsOf("board_health")).toEqual([{ project: P, id: REF.id }]);
    expect(store.getOpenBoard(REF)?.label).toBe("1 missing");
  });

  it("reloads a clean board that changed on disk", async () => {
    engine.emit({ type: "board_conflict", project: P, board: REF.id, file: "published", savedRevision: "r1", diskRevision: "r9" });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getOpenBoard(REF)?.revision).toBe("r9");
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
