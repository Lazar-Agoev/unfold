// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardDocument, Page } from "../../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page } from "../../test/workspace";
import { describeCard, editFrame, editNote, nameBlock, removeFrame, removeItem } from "./api";
import { moveFrame } from "./frames";
import { cancelLink, clickItem, startLink } from "./linking";
import { placeNote } from "./notes";
import * as store from "./store";

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const LIBRARY_CALL = "shop/service.py#place_order/try[0]/except[0]/call:logger.warning";
const AUTHOR = "person:maya";

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  engine = installFakeEngine({ handlers: { save_board: ({ board }) => boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"])) } });
  store.adoptBoardState(
    boardState(
      P,
      ID,
      boardDocument([
        page("page-1", [{ id: "place", ref: { block: "shop/service.py#place_order" }, at: [0, 0] }, { id: "save", ref: { block: "shop/store.py#save_order" }, at: [600, 0] }], {
          frames: [{ id: "api", title: "Shop API", members: ["place"], look: "box", author: "agent:claude" }],
          notes: [{ id: "note", text: "Refunds skip the queue.", at: [40, 40], attach: "api", author: "agent:claude" }],
        }),
      ]),
    ),
  );
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

function shown(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("no page");
  }
  return found;
}

describe("names and descriptions by hand", () => {
  it("name any block page-wide, a library call too; typing is one undo step, and clearing the name takes it off", () => {
    nameBlock(PAGE, LIBRARY_CALL, "Warn", AUTHOR);
    nameBlock(PAGE, LIBRARY_CALL, "Warn support", AUTHOR);
    expect(shown().names).toEqual([{ key: LIBRARY_CALL, text: "Warn support", author: AUTHOR, signature: "" }]);
    store.undo(REF);
    expect(shown().names).toEqual([]);
    store.redo(REF);
    nameBlock(PAGE, LIBRARY_CALL, "", AUTHOR);
    expect(shown().names).toEqual([]);
  });

  it("describe the card, unsigned until the next save, as one undo step", () => {
    describeCard(PAGE, "place", "Charges the card, then saves the order.", AUTHOR);
    expect(shown().cards[0]?.description).toEqual({ text: "Charges the card, then saves the order.", author: AUTHOR, signature: "" });
    store.undo(REF);
    expect(shown().cards[0]?.description).toBeUndefined();
  });
});

describe("frames and notes by hand", () => {
  it("rename, describe, collapse, and remove a frame, each one undo step", () => {
    editFrame(PAGE, "api", { title: "Orders API", author: AUTHOR });
    expect(shown().frames?.[0]).toMatchObject({ title: "Orders API", author: AUTHOR });
    editFrame(PAGE, "api", { description: { text: "The public API.", author: AUTHOR } });
    expect(shown().frames?.[0]?.description?.text).toBe("The public API.");
    editFrame(PAGE, "api", { collapsed: true });
    expect(shown().frames?.[0]?.collapsed).toBe(true);
    removeFrame(PAGE, "api");
    expect(shown().frames).toBeUndefined();
    expect(shown().cards.map((card) => card.id)).toEqual(["place", "save"]);
    store.undo(REF);
    expect(shown().frames?.[0]?.collapsed).toBe(true);
    store.undo(REF);
    expect(shown().frames?.[0]?.collapsed).toBeUndefined();
    store.undo(REF);
    expect(shown().frames?.[0]?.description).toBeUndefined();
    store.undo(REF);
    expect(shown().frames?.[0]?.title).toBe("Shop API");
  });

  it("move a frame with what it holds, as one undo step", () => {
    store.editPage(PAGE, (draft) => {
      moveFrame(draft, "api", 100, 50);
    });
    expect(shown().cards.map((card) => card.at)).toEqual([
      [100, 50],
      [600, 0],
    ]);
    store.undo(REF);
    expect(shown().cards[0]?.at).toEqual([0, 0]);
  });

  it("edit, move, and remove a note, each one undo step", () => {
    editNote(PAGE, "note", "Refunds call the gateway directly.", AUTHOR);
    expect(shown().notes?.[0]).toMatchObject({ text: "Refunds call the gateway directly.", author: AUTHOR });
    store.editPage(PAGE, (draft) => {
      placeNote(draft, "note", [80, 10], "api");
    });
    expect(shown().notes?.[0]?.at).toEqual([80, 10]);
    removeItem(PAGE, "note");
    expect(shown().notes).toBeUndefined();
    store.undo(REF);
    store.undo(REF);
    expect(shown().notes?.[0]?.at).toEqual([40, 40]);
    store.undo(REF);
    expect(shown().notes?.[0]?.text).toBe("Refunds skip the queue.");
  });
});

describe("drawing a link", () => {
  it("starts at an item, ends at the next one clicked (a click where it starts waits), and selects nothing meanwhile", () => {
    startLink(PAGE, "place");
    clickItem(PAGE, "place", { kind: "card", cardId: "place", key: null });
    expect(store.getPageView(PAGE).linking).toEqual({ from: "place", to: null });
    clickItem(PAGE, "save", { kind: "card", cardId: "save", key: null });
    expect(store.getPageView(PAGE).linking).toEqual({ from: "place", to: "save" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    cancelLink(PAGE);
    clickItem(PAGE, "save", { kind: "card", cardId: "save", key: null });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "save", key: null });
    expect(store.canUndo(REF)).toBe(false);
  });
});
