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
    expect(engine.callsOf("repair_board")).toEqual([{ project: P, id: ID, base: "r1", repair: REPAIR }]);
    expect(result).toMatchObject({ ok: true, page: { id: "page-1", cards: [{ id: "a", ref: { block: "src/a.ts#function:b" } }] }, board: { revision: "r2" } });
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

  it("removes a person's words only when the person asked, as remove_items does", async () => {
    const lazar = "person:lazar";
    const described = { ...CARD, description: { text: "Places it.", author: lazar, signature: "s" } };
    const other = { id: "b", ref: { block: "src/b.ts#function:b" }, at: [400, 0] as [number, number], description: { text: "Bills.", author: "agent:claude", signature: "s" } };
    const document = boardDocument([
      page("page-1", [described, other], {
        names: [{ key: "src/b.ts#function:b/if[0]", text: "Paid?", author: lazar, signature: "s" }],
        combined: [{ id: "pay", title: "Pay", members: ["src/b.ts#function:b/return[0]"], collapsed: true, author: "agent:claude", signature: "s" }],
        links: [{ id: "bills", from: "b", to: "a", kind: "calls", anchors: [], author: lazar }],
      }),
    ]);
    store.adoptBoardState(boardState(P, ID, document));
    const repair = (spec: Record<string, unknown>, replacePeopleText?: boolean) =>
      repairForAgent({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, repair: { page: "page-1", ...spec } as typeof REPAIR, items: [], ...(replacePeopleText !== undefined ? { replacePeopleText } : {}) });

    expect(await repair({ kind: "remove", target: { card: "a" } })).toEqual({
      ok: false,
      code: "people_text",
      message: "Can't change the description of src/a.ts#function:a: Lazar wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    // What goes with an item counts: the connection a person drew to the agent's card.
    expect(await repair({ kind: "remove", target: { card: "b" } })).toMatchObject({ ok: false, code: "people_text", message: expect.stringMatching(/^Can't change the calls connection: Lazar wrote it\./) as string });
    expect(await repair({ kind: "remove", target: { name: "src/b.ts#function:b/if[0]" } })).toMatchObject({ ok: false, code: "people_text", message: expect.stringMatching(/^Can't change the name “Paid\?”/) as string });
    expect(await repair({ kind: "remove_description", target: { card: "a" } })).toMatchObject({ ok: false, code: "people_text" });
    expect(engine.callsOf("repair_board")).toEqual([]);

    // An agent's words go without asking; a person's when the person asked.
    expect(await repair({ kind: "remove", target: { combined: "pay" } })).toMatchObject({ ok: true });
    expect(await repair({ kind: "remove_description", target: { card: "a" } }, true)).toMatchObject({ ok: true });
    expect(engine.callsOf("repair_board").map((call) => call["repair"])).toEqual([
      { page: "page-1", kind: "remove", target: { combined: "pay" } },
      { page: "page-1", kind: "remove_description", target: { card: "a" } },
    ]);
  });

  it("keeps a person's edit made while it runs", async () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
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
