// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument, BoardState, SaveBoardArgs } from "../../../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page } from "../../../test/workspace";
import { getShownBoard, showHome, useShownBoardStore } from "../../shownBoard";
import * as store from "../store";
import { createBoardForAgent, editBoardForAgent, showForAgent } from "./boards";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const CARD = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] as [number, number] };

let engine: FakeEngine;
const saved: SaveBoardArgs[] = [];

/** What `save_board` answers: the document as sent. */
function savedState(args: SaveBoardArgs): BoardState {
  saved.push(args);
  const document = { format: "unfold-board", version: 1, ...args.board } as BoardDocument;
  return boardState(args.project, args.id, document, { revision: `r${saved.length + 1}` });
}

beforeEach(() => {
  store.resetBoards();
  useShownBoardStore.setState({ board: null });
  saved.length = 0;
  engine = installFakeEngine({ handlers: { save_board: savedState } });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

function openWith(document: BoardDocument): void {
  store.adoptBoardState(boardState(P, ID, document));
}

describe("creating a board for an agent", () => {
  it("creates a board file with its pages, saved before its session opens, without showing it", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        new_board: (args) => boardState(args.project, ID, boardDocument([{ ...page("page-1"), title: args.pageTitle ?? "Page 1" }], { title: args.title ?? "Untitled board", ...(args.folder !== undefined ? { folder: args.folder } : {}) })),
        // The engine keeps the folder, which a save doesn't send.
        save_board: (args) => {
          const state = savedState(args);
          return { ...state, document: { ...state.document, folder: "flows" } };
        },
      },
    });
    const result = await createBoardForAgent({ requestId: 1, project: P, title: "Orders", folder: "flows", pages: ["Placing an order", "Async path"] });
    expect(engine.callsOf("new_board")).toEqual([{ project: P, title: "Orders", folder: "flows", pageTitle: "Placing an order" }]);
    expect(saved.map((args) => args.board.pages.map((item) => item.title))).toEqual([["Placing an order", "Async path"]]);
    expect(result).toEqual({
      ok: true,
      board: REF,
      info: { title: "Orders", revision: "r2" },
      folder: "flows",
      pages: [
        { id: "page-1", title: "Placing an order" },
        { id: "page-2", title: "Async path" },
      ],
    });
    expect(store.getOpenBoard(REF)).toBeDefined();
    expect(getShownBoard()).toBeNull();
    expect(store.canUndo(REF)).toBe(false);
  });
});

describe("editing a board's pages for an agent", () => {
  it("applies the page operations as one undo step, saved at once", async () => {
    openWith(boardDocument([page("page-1", [CARD]), { ...page("page-2"), title: "Scratch" }, { ...page("page-3"), title: "Third" }]));
    const result = await editBoardForAgent({
      requestId: 1,
      board: REF,
      agent: CLAUDE,
      title: "Edit pages",
      ops: [
        { op: "add_page", title: "Async path", after: "page-1" },
        { op: "rename_page", page: "page-3", title: "Third" },
        { op: "move_page", page: "page-3", to: 0 },
        { op: "delete_page", page: "page-2" },
        { op: "set_title", title: "Orders" },
      ],
    });
    expect(result).toEqual({
      ok: true,
      board: { title: "Orders", revision: "r2" },
      pages: [
        { id: "page-3", title: "Third" },
        { id: "page-1", title: "Page page-1" },
        { id: "page-4", title: "Async path" },
      ],
      results: [
        { op: "add_page", status: "created", page: "page-4", title: "Async path" },
        { op: "rename_page", status: "kept", page: "page-3", title: "Third" },
        { op: "move_page", status: "updated", page: "page-3", title: "Third" },
        { op: "delete_page", status: "removed", page: "page-2", title: "Scratch" },
        { op: "set_title", status: "updated", title: "Orders" },
      ],
    });
    expect(saved).toHaveLength(1);
    const board = store.getOpenBoard(REF);
    expect(board?.history.past.map((entry) => entry.name)).toEqual(["Claude · Edit pages"]);
    expect(store.undo(REF)).toBe(true);
    expect(store.getOpenBoard(REF)?.document.pages.map((item) => item.id)).toEqual(["page-1", "page-2", "page-3"]);
  });

  it("keeps people's words on a page unless the person asked, and keeps one page", async () => {
    const labeled = { ...page("page-2"), title: "Scratch", labels: [{ key: "src/a.ts#function:a/if[0]", text: "Queue it", author: "person:lazar", signature: "" }] };
    openWith(boardDocument([page("page-1", [CARD]), labeled]));
    const refused = await editBoardForAgent({ requestId: 1, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "delete_page", page: "page-2" }] });
    expect(refused).toMatchObject({ ok: false, code: "people_text" });
    expect(!refused.ok && refused.message).toContain("Can't change the label “Queue it” on the page “Scratch”: Lazar wrote it.");
    expect(saved).toEqual([]);

    store.setActivePage(REF, "page-2");
    const allowed = await editBoardForAgent({ requestId: 2, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "delete_page", page: "page-2" }], replacePeopleText: true });
    expect(allowed).toMatchObject({ ok: true, pages: [{ id: "page-1" }] });
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-1");

    expect(await editBoardForAgent({ requestId: 3, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "delete_page", page: "page-1" }] })).toMatchObject({ ok: false, code: "invalid_op" });
    expect(await editBoardForAgent({ requestId: 4, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "rename_page", page: "page-9", title: "X" }] })).toMatchObject({
      ok: false,
      code: "page_not_found",
      message: "The board has no page with id “page-9”. Its pages: Page page-1 (id page-1).",
    });
    expect(await editBoardForAgent({ requestId: 5, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "move_page", page: "page-1", to: 3 }] })).toMatchObject({ ok: false, code: "invalid_op" });
    expect(await editBoardForAgent({ requestId: 6, board: { project: P, id: "other.json" }, agent: CLAUDE, title: "Edit pages", ops: [] })).toMatchObject({ ok: false, code: "board_not_open" });
  });

  it("counts a person's connection and a drawn link on a page as their words, as remove_items does", async () => {
    const B = { ...CARD, id: "b", at: [600, 0] as [number, number] };
    const connected = { ...page("page-2", [CARD, B], { links: [{ id: "posts", kind: "http", from: "a", to: "b", author: "person:lazar" }] }), title: "Scratch" };
    const drawn = { ...page("page-3", [CARD, B], { links: [{ id: "a-to-b", from: "a", to: "b" }] }), title: "Sketch" };
    openWith(boardDocument([page("page-1", [CARD]), connected, drawn]));
    const deleting = (id: string) => editBoardForAgent({ requestId: 1, board: REF, agent: CLAUDE, title: "Edit pages", ops: [{ op: "delete_page", page: id }] });
    const person = await deleting("page-2");
    expect(!person.ok && person.message).toContain("Can't change the http connection on the page “Scratch”: Lazar wrote it.");
    const line = await deleting("page-3");
    expect(!line.ok && line.message).toContain("Can't change the link a person drew on the page “Sketch”: a person wrote it.");
    expect(saved).toEqual([]);
  });
});

describe("showing a board for an agent", () => {
  it("shows the board and page, selects the item, and centers it", () => {
    const group = { id: "g", card: "a", title: "Check", members: [], collapsed: false };
    openWith(boardDocument([page("page-1"), page("page-2", [CARD], { groups: [group] })]));
    showHome();
    expect(showForAgent({ requestId: 1, board: REF, page: "page-2", item: "a" })).toEqual({ ok: true, page: "page-2" });
    expect(getShownBoard()).toEqual(REF);
    const target = { board: REF, page: "page-2" };
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(store.getPageView(target).selection).toEqual({ kind: "card", cardId: "a", key: null });
    expect(store.getPageView(target).focus).toMatchObject({ cardIds: ["a"], mode: "center" });

    showForAgent({ requestId: 2, board: REF, page: "page-2", item: "g" });
    expect(store.getPageView(target).selection).toEqual({ kind: "group", groupId: "g" });
  });

  it("refuses an item or page that isn't there", () => {
    openWith(boardDocument([page("page-1", [CARD])]));
    expect(showForAgent({ requestId: 1, board: REF, item: "nope" })).toEqual({ ok: false, code: "invalid_op", message: "There is no item with id “nope” on the page “Page page-1”. read_page lists the page's ids." });
    expect(showForAgent({ requestId: 2, board: REF, page: "page-9" })).toMatchObject({ ok: false, code: "page_not_found" });
  });
});
