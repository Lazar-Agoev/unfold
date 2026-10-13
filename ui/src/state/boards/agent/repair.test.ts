// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepairBoardArgs } from "../../../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page } from "../../../test/workspace";
import { moveItems } from "../document";
import { useShownBoardStore } from "../../shownBoard";
import * as store from "../store";
import { repairForAgent } from "./repair";
import { resetWorking, useWorkingItems } from "./working";

const P = "p-orders0000";
const ID = ".unfold/boards/orders.json";
const REF = { project: P, id: ID };
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const CARD = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] as [number, number] };
const REPAIR = { page: "page-1", kind: "reconnect", from: "src/a.ts#function:a", to: "src/a.ts#function:b" };

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  useShownBoardStore.setState({ board: null });
  engine = installFakeEngine({
    handlers: {
      repair_board: (args: RepairBoardArgs) => boardState(args.project, args.id, boardDocument([page("page-1", [{ ...CARD, ref: { block: "src/a.ts#function:b" } }])]), { revision: "r2" }),
    },
  });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

describe("repairing a board for an agent", () => {
  it("applies the repair as one undo step, answers with the page as saved, and marks what it changed", async () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
    const result = await repairForAgent({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, repair: REPAIR, items: ["a"] });
    expect(engine.callsOf("repair_board")).toEqual([{ project: P, id: ID, repair: REPAIR }]);
    expect(result).toMatchObject({ ok: true, page: { id: "page-1", cards: [{ id: "a", ref: { block: "src/a.ts#function:b" } }] }, board: { revision: "r2", place: "draft" } });
    expect(store.canUndo(REF)).toBe(true);
    expect(renderHook(() => useWorkingItems({ board: REF, page: "page-1" })).result.current).toEqual(new Set(["a"]));
  });

  it("refuses a board this window doesn't hold, and passes on the engine's refusal", async () => {
    expect(await repairForAgent({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, repair: REPAIR, items: [] })).toMatchObject({ ok: false, code: "board_not_open" });
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        repair_board: () => {
          throw Object.assign(new Error("No such key."), { code: "board_problem" });
        },
      },
    });
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
    expect(await repairForAgent({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, repair: REPAIR, items: [] })).toMatchObject({ ok: false });
    expect(store.canUndo(REF)).toBe(false);
  });

  it("refuses while a conflict is open, and keeps a person's edit made while it runs", async () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
    store.useBoards.setState((draft) => {
      const open = Object.values(draft.boards)[0];
      if (open) {
        open.conflict = { diskRevision: "r9" };
      }
    });
    expect(await repairForAgent({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, repair: REPAIR, items: ["a"] })).toMatchObject({ ok: false, code: "board_conflict" });
    expect(engine.callsOf("repair_board")).toEqual([]);

    store.useBoards.setState((draft) => {
      const open = Object.values(draft.boards)[0];
      if (open) {
        open.conflict = null;
      }
    });
    let finish: (() => void) | undefined;
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        repair_board: (args: RepairBoardArgs) =>
          new Promise((resolve) => {
            finish = () => {
              resolve(boardState(args.project, args.id, boardDocument([page("page-1", [{ ...CARD, ref: { block: "src/a.ts#function:b" } }])]), { revision: "r2" }));
            };
          }),
      },
    });
    const target = { board: REF, page: "page-1" };
    const result = repairForAgent({ requestId: 2, board: REF, page: "page-1", agent: CLAUDE, repair: REPAIR, items: ["a"] });
    await vi.waitFor(() => {
      expect(finish).toBeDefined();
    });
    store.editPage(target, (draft) => {
      moveItems(draft, { a: [40, 0] });
    });
    finish?.();
    expect(await result).toMatchObject({ ok: false, code: "page_changed" });
    const board = store.getOpenBoard(REF);
    expect(board?.document.pages[0]?.cards[0]).toMatchObject({ ref: { block: "src/a.ts#function:a" }, at: [40, 0] });
    expect(board && store.isDirty(board)).toBe(true);
    expect(renderHook(() => useWorkingItems(target)).result.current).toEqual(new Set());
  });
});
