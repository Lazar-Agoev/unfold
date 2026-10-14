// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardDocument, Connection } from "../../ipc/generated";
import * as store from "../../state/boards/store";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page } from "../../test/workspace";
import { kindEntries } from "./linkMenu";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../../components/Toast", () => toast);

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const AUTHOR = "person:maya";
const CALL = "shop/service.py#place_order/call:client.post";

let engine: FakeEngine;
let refuse: string | null;

function open(links: Connection[]): void {
  store.adoptBoardState(
    boardState(P, ID, boardDocument([page("page-1", [{ id: "place", ref: { block: "shop/service.py#place_order" }, at: [0, 0] }, { id: "save", ref: { block: "shop/store.py#save_order" }, at: [600, 0] }], { links })])),
  );
}

function link(): Connection | undefined {
  return store.getOpenBoard(REF)?.document.pages[0]?.links[0];
}

/** Picks entry `id` of connection `linkId`'s Kind ▸. */
function choose(linkId: string, id: string): void {
  const entry = kindEntries(PAGE, linkId, AUTHOR).find((item) => item.id === id);
  if (entry?.kind === "separator" || entry?.kind === "heading" || !entry?.onSelect) {
    throw new Error(`no ${id} entry`);
  }
  entry.onSelect();
}

beforeEach(() => {
  store.resetBoards();
  refuse = null;
  toast.showToast.mockClear();
  engine = installFakeEngine({
    handlers: {
      save_board: ({ board }) => {
        if (refuse !== null) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "board_problem", message: refuse };
        }
        return boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"]));
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("Kind ▸", () => {
  it("offers calls only on a connection that leaves from a call, and other only once it has a label", () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: AUTHOR }]);
    const drawn = kindEntries(PAGE, "drawn", AUTHOR);
    expect(drawn.map((entry) => entry.id)).toEqual(["kind-http", "kind-queue", "kind-reads", "kind-writes", "kind-relates", "kind-other"]);
    expect(drawn.find((entry) => entry.id === "kind-other")).toMatchObject({ disabled: true });

    open([{ id: "traced", kind: "http", from: "place", to: "save", step: CALL, label: { text: "Posts", author: AUTHOR, signature: "" }, author: "agent:claude" }]);
    const traced = kindEntries(PAGE, "traced", AUTHOR);
    expect(traced.map((entry) => entry.id)).toContain("kind-calls");
    expect(traced.find((entry) => entry.id === "kind-other")).toMatchObject({ disabled: false });
  });

  it("saves a new kind at once as one undo step, signed by the person", async () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: "agent:claude" }]);
    choose("drawn", "kind-queue");
    await vi.waitFor(() => {
      expect(engine.callsOf("save_board")).toHaveLength(1);
    });
    await vi.waitFor(() => {
      expect(link()).toMatchObject({ kind: "queue", author: AUTHOR });
    });
    const board = store.getOpenBoard(REF);
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.history.past.map((step) => step.name)).toEqual(["Change kind"]);
    store.undo(REF);
    expect(link()?.kind).toBe("http");
  });

  it("leaves the connection as it was, with nothing unsaved, when the board refuses, and says why", async () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: AUTHOR }]);
    refuse = "Can't connect place to save (queue): no.";
    choose("drawn", "kind-queue");
    await vi.waitFor(() => {
      expect(toast.showToast).toHaveBeenCalledWith({ title: "Couldn't change the kind", body: "Can't connect place to save (queue): no.", tone: "danger" });
    });
    const board = store.getOpenBoard(REF);
    expect(link()?.kind).toBe("http");
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.saveError).toBeNull();
    expect(board?.history.past).toEqual([]);
  });
});
