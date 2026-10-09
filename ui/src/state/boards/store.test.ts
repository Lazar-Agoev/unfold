import type { Draft } from "immer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardDocument, BoardState, Page, SaveBoardArgs } from "../../ipc/generated";
import { UnfoldError } from "../../ipc/unfold";
import { boardDocument, boardState, page } from "../../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("./engine", () => engine);
vi.mock("../../components/Toast", () => toast);

const { addCard, moveCards, setCardsFolded, setTitle } = await import("./document");
const store = await import("./store");
const { closeBoard, closeBoardsOfProject, openBoard, reclaimBoards } = await import("./open");
const { addPage } = await import("./pages");
const tabs = await import("../tabStore");

const P = "p-orders0000";
const Q = "p-payments00";
const ID = ".unfold/local/drafts/order-placement.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };

function doc(cards: BoardDocument["pages"][number]["cards"] = []): BoardDocument {
  return boardDocument([page("page-1", cards)]);
}

function state(document = doc(), revision = "r1", overrides: Partial<BoardState> = {}): BoardState {
  return boardState(P, ID, document, { revision, ...overrides });
}

/** Answers `save_board` with the document it was given, signed as revision `revision`. */
function savesAs(revision: string) {
  return (command: string, args: SaveBoardArgs) =>
    Promise.resolve(command === "save_board" ? state({ ...doc(), title: args.board.title, pages: args.board.pages as BoardDocument["pages"] }, revision) : null);
}

function edit<T>(recipe: (page: Draft<Page>) => T, target = PAGE): T | undefined {
  return store.editPage(target, recipe);
}

beforeEach(() => {
  vi.useFakeTimers();
  engine.call.mockReset();
  engine.windowCommand.mockClear();
  toast.showToast.mockClear();
  store.resetBoards();
  tabs.useTabs.setState({ boards: [], active: "home" });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("opening and closing", () => {
  it("claims a board, opens its tab, and shows the page its saved view names with that camera and lens", async () => {
    const two = boardDocument([page("page-1"), page("page-2")]);
    engine.call.mockResolvedValueOnce({
      status: "opened",
      board: state(two, "r1", { view: { page: "page-2", pages: { "page-2": { x: 5, y: 6, zoom: 0.5, lens: "flow" } } } }),
    });
    await expect(openBoard(REF)).resolves.toEqual({ status: "opened", ref: REF });
    expect(engine.call).toHaveBeenCalledWith("open_board", { project: P, id: ID });
    const board = store.getOpenBoard(REF);
    expect(board?.activePage).toBe("page-2");
    expect(board?.pages["page-2"]).toMatchObject({ camera: { x: 5, y: 6, zoom: 0.5 }, lens: "flow" });
    expect(tabs.useTabs.getState()).toEqual({ boards: [REF], active: `${P}\u0000${ID}` });
  });

  it("opens on the page the window's tab remembered, and claims once for two quick opens", async () => {
    engine.call.mockResolvedValue({ status: "opened", board: state(boardDocument([page("page-1"), page("page-2")])) });
    const [first, second] = await Promise.all([openBoard(REF, { page: "page-2", activate: false }), openBoard(REF)]);
    expect(first).toEqual(second);
    expect(engine.call).toHaveBeenCalledTimes(1);
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(tabs.useTabs.getState().active).toBe("home");
  });

  it("focuses the window that holds a board instead of opening it", async () => {
    engine.call.mockResolvedValueOnce({ status: "held", heldBy: 7 });
    await expect(openBoard(REF)).resolves.toEqual({ status: "held", heldBy: 7 });
    expect(engine.windowCommand).toHaveBeenCalledWith("focus_window", { windowId: 7, board: { project: P, board: ID } });
    expect(store.getOpenBoard(REF)).toBeUndefined();
    expect(tabs.useTabs.getState().boards).toEqual([]);
  });

  it("refuses a 101st board tab before claiming it", async () => {
    tabs.useTabs.setState({ boards: Array.from({ length: tabs.MAX_BOARD_TABS }, (_, index) => ({ project: P, id: `b${index}` })), active: "home" });
    await expect(openBoard(REF)).rejects.toMatchObject({ code: "too_many_tabs" });
    expect(engine.call).not.toHaveBeenCalled();
  });

  it("saves edits and view, releases the claim, and closes the tab; closes only one project's boards", async () => {
    const other = { project: Q, id: ID };
    store.adoptBoardState(state());
    store.adoptBoardState(boardState(Q, ID));
    tabs.openBoardTab(REF);
    tabs.openBoardTab(other);
    engine.call.mockImplementation(savesAs("r2"));
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    store.setCamera(PAGE, { x: 1, y: 2, zoom: 1 });
    await closeBoardsOfProject(P);
    expect(engine.call.mock.calls.map(([command]) => command as string).sort()).toEqual(["close_board", "save_board", "save_board_view"]);
    expect(store.getOpenBoard(REF)).toBeUndefined();
    expect(store.getOpenBoard(other)).toBeDefined();
    expect(tabs.useTabs.getState()).toEqual({ boards: [other], active: `${Q}\u0000${ID}` });
    await closeBoard(other);
    expect(tabs.useTabs.getState()).toEqual({ boards: [], active: "home" });
  });
});

describe("editing and autosave", () => {
  it("autosaves once, after the last of several edits, as a BoardEdit of the title and pages only", async () => {
    store.adoptBoardState(state(boardDocument([page("page-1")], { folder: "Checkout", projects: ["payments"] })));
    engine.call.mockImplementation(savesAs("r2"));
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    vi.advanceTimersByTime(store.AUTOSAVE_MS - 100);
    edit((draft) => addCard(draft, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    vi.advanceTimersByTime(store.AUTOSAVE_MS - 100);
    expect(engine.call).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(engine.call).toHaveBeenCalledTimes(1);
    const [command, args] = engine.call.mock.calls[0] as [string, SaveBoardArgs];
    expect(command).toBe("save_board");
    expect(args.project).toBe(P);
    expect(args.id).toBe(ID);
    expect(Object.keys(args.board).sort()).toEqual(["pages", "title"]);
    expect(args.board.pages[0]?.cards.map((card) => card.id)).toEqual(["a", "b"]);
    const board = store.getOpenBoard(REF);
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.revision).toBe("r2");
  });

  it("never overlaps saves: an edit made during a save is written right after it", async () => {
    store.adoptBoardState(state());
    let release: (() => void) | undefined;
    engine.call.mockImplementation(
      (_command: string, args: SaveBoardArgs) =>
        new Promise((resolve) => {
          release = () => {
            resolve(state({ ...doc(), pages: args.board.pages as BoardDocument["pages"] }, "r2"));
          };
        }),
    );
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    const first = store.saveBoard(REF);
    await vi.advanceTimersByTimeAsync(0);
    edit((draft) => addCard(draft, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    const waiting = [store.saveBoard(REF), store.flushBoard(REF), store.saveBoard(REF)];
    expect(engine.call).toHaveBeenCalledTimes(1);
    release?.();
    await first;
    await vi.advanceTimersByTimeAsync(0);
    expect(engine.call).toHaveBeenCalledTimes(2);
    release?.();
    await Promise.all(waiting);
    expect(engine.call).toHaveBeenCalledTimes(2);
    const saved = engine.call.mock.calls[1]?.[1] as SaveBoardArgs;
    expect(saved.board.pages[0]?.cards).toHaveLength(2);
    expect(store.isDirty(store.getOpenBoard(REF) as never)).toBe(false);
  });

  it("keeps the identity of everything a save didn't change, so unchanged cards don't redraw", async () => {
    store.adoptBoardState(state(doc([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])));
    edit((draft) => addCard(draft, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    const before = store.getOpenBoard(REF);
    // The engine answers with an equal copy, one card signed.
    engine.call.mockImplementation((_command: string, args: SaveBoardArgs) => {
      const pages = structuredClone(args.board.pages) as BoardDocument["pages"];
      const second = pages[0]?.cards[1];
      if (second) {
        second.description = { text: "Signed", author: "person:a", signature: "sha256:1" };
      }
      return Promise.resolve(state({ ...doc(), pages }, "r2"));
    });
    await store.saveBoard(REF);
    const after = store.getOpenBoard(REF);
    expect(after?.document).not.toBe(before?.document);
    expect(after?.document.pages[0]?.cards[0]).toBe(before?.document.pages[0]?.cards[0]);
    expect(after?.document.pages[0]?.cards[1]?.description?.text).toBe("Signed");
    expect(after?.health).toBe(before?.health);
    expect(after?.outlines).toBe(before?.outlines);
  });

  it("surfaces a failed save and keeps the edit unsaved; a disconnected one stays quiet", async () => {
    store.adoptBoardState(state());
    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "board_problem", message: "missing reference" }));
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    await store.saveBoard(REF);
    expect(store.getOpenBoard(REF)?.saveError).toBe("missing reference");
    expect(toast.showToast).toHaveBeenCalledTimes(1);
    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "disconnected", message: "gone" }));
    await store.saveBoard(REF);
    expect(toast.showToast).toHaveBeenCalledTimes(1);
    expect(store.isDirty(store.getOpenBoard(REF) as never)).toBe(true);
  });

  it("sends every unsaved board and view at once when the page goes away", () => {
    store.adoptBoardState(state());
    engine.call.mockReturnValue(new Promise(() => undefined));
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    store.setPageLens(PAGE, "flow");
    store.saveAllNow();
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["save_board", "save_board_view"]);
    // The autosaves it replaced don't run later.
    vi.advanceTimersByTime(store.VIEW_SAVE_MS * 2);
    expect(engine.call).toHaveBeenCalledTimes(2);
  });
});

describe("undo and redo", () => {
  it("undoes one edit at a time, merges a coalesced burst, and keeps at most 200 steps", () => {
    store.adoptBoardState(state());
    const id = edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] })) ?? "";
    store.editPage(PAGE, (draft) => { moveCards(draft, { [id]: [10, 0] }); }, { coalesce: "drag" });
    store.editPage(PAGE, (draft) => { moveCards(draft, { [id]: [20, 0] }); }, { coalesce: "drag" });
    edit((draft) => { setCardsFolded(draft, [id], true); });
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(3);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.folded).toBeUndefined();
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.at).toEqual([0, 0]);
    store.redo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.at).toEqual([20, 0]);
    store.undo(REF);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(0);
    expect(store.canUndo(REF)).toBe(false);
    expect(store.canRedo(REF)).toBe(true);
    for (let step = 0; step < 250; step += 1) {
      store.editBoard(REF, (document) => { setTitle(document, `Title ${step}`); });
    }
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(store.HISTORY_LIMIT);
  });

  it("keeps one history across the board's pages, and undo or redo shows the page each step changed (V-54)", () => {
    store.adoptBoardState(state(boardDocument([page("page-1"), page("page-2")])));
    const second = { board: REF, page: "page-2" };
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    store.setActivePage(REF, "page-2");
    edit((draft) => addCard(draft, { ref: { block: "src/b.ts#function:b" }, at: [0, 0] }), second);
    store.setActivePage(REF, "page-1");

    store.undo(REF);
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(store.getOpenBoard(REF)?.document.pages[1]?.cards).toHaveLength(0);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-1");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(0);
    store.redo(REF);
    store.redo(REF);
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(store.getOpenBoard(REF)?.document.pages.map((item) => item.cards.length)).toEqual([1, 1]);
  });

  it("undoing a new page shows the page it was added from", () => {
    store.adoptBoardState(state());
    const added = addPage(REF);
    expect(added).toBe("page-2");
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(store.getOpenBoard(REF)?.document.pages[1]?.title).toBe("Page 2");
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages).toHaveLength(1);
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-1");
  });

  it("makes a repair one undo step", () => {
    store.adoptBoardState(state(doc([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])));
    edit((draft) => { moveCards(draft, { a: [10, 0] }); });
    const beforeRepair = store.getOpenBoard(REF)?.document;
    store.useBoards.setState((draft) => {
      const open = Object.values(draft.boards)[0];
      if (open) {
        open.savedEdits = open.edits;
      }
    });
    store.applyServerState(REF, state(doc([]), "r3"), "page-1");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(0);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document).toEqual(beforeRepair);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.at).toEqual([0, 0]);
  });
});

describe("view state", () => {
  it("saves the active page and each page's camera and lens once they settle, never as undo steps", async () => {
    store.adoptBoardState(state(boardDocument([page("page-1"), page("page-2")])));
    engine.call.mockResolvedValue(null);
    store.setCamera(PAGE, { x: 10, y: 20, zoom: 0.8 });
    store.setActivePage(REF, "page-2");
    store.setPageLens({ board: REF, page: "page-2" }, "flow");
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(store.VIEW_SAVE_MS);
    expect(engine.call).toHaveBeenCalledTimes(1);
    expect(engine.call).toHaveBeenCalledWith("save_board_view", {
      project: P,
      id: ID,
      view: { page: "page-2", pages: { "page-1": { x: 10, y: 20, zoom: 0.8, lens: null }, "page-2": { x: 40, y: 40, zoom: 1, lens: "flow" } } },
    });
    expect(store.getOpenBoard(REF)?.viewDirty).toBe(false);
  });

  it("applies a page's lens over every card until a card's own switch", () => {
    store.adoptBoardState(state());
    store.updatePageView(PAGE, (view) => {
      view.lensExceptions = ["x"];
    });
    store.setPageLens(PAGE, "flow");
    const view = store.getPageView(PAGE);
    expect(view.lensExceptions).toEqual([]);
    expect(store.effectiveView(view.lens, "code", "code", false)).toBe("flow");
    expect(store.effectiveView(view.lens, "code", "code", true)).toBe("code");
    expect(store.effectiveView(null, undefined, "flow", false)).toBe("flow");
    expect(store.getOpenBoard(REF)?.edits).toBe(0);
  });
});

describe("outside edits", () => {
  it("reloads a board without unsaved edits", async () => {
    store.adoptBoardState(state());
    const theirs = doc([{ id: "x", ref: { block: "src/x.ts#function:x" }, at: [0, 0] }]);
    engine.call.mockResolvedValueOnce(state(theirs, "r9"));
    await store.handleOutsideChange(REF, "r9");
    expect(engine.call).toHaveBeenCalledWith("reload_board", { project: P, id: ID });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards.map((card) => card.id)).toEqual(["x"]);
    expect(store.getOpenBoard(REF)?.conflict).toBeNull();
  });

  it("asks about a board with unsaved edits, then keeps this window's version on request", async () => {
    store.adoptBoardState(state());
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    await store.handleOutsideChange(REF, "r9");
    expect(store.getOpenBoard(REF)?.conflict).toEqual({ diskRevision: "r9" });
    // Autosave holds off while the person decides.
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS * 2);
    expect(engine.call).not.toHaveBeenCalled();
    engine.call.mockImplementation(savesAs("r10"));
    await store.resolveConflict(REF, "mine");
    expect(engine.call).toHaveBeenCalledWith("save_board", expect.objectContaining({ project: P, id: ID, force: true }));
    expect(store.getOpenBoard(REF)?.conflict).toBeNull();
  });

  it("discards changes as one undo step back to the published version", async () => {
    store.adoptBoardState(state(doc(), "r1", { place: "published" }));
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    engine.call.mockResolvedValueOnce(state(doc(), "r0", { place: "published" }));
    await store.discardChanges(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(0);
    expect(store.isDirty(store.getOpenBoard(REF) as never)).toBe(false);
    expect(store.canUndo(REF)).toBe(true);
  });
});

describe("engine-host restarts", () => {
  it("claims each board tab again before writing its unsaved edits", async () => {
    store.adoptBoardState(state());
    tabs.openBoardTab(REF);
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    engine.call.mockImplementation((command: string, args: SaveBoardArgs) =>
      command === "open_board" ? Promise.resolve({ status: "opened", board: state() }) : savesAs("r2")(command, args),
    );
    await reclaimBoards();
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["open_board", "save_board"]);
    expect(store.isDirty(store.getOpenBoard(REF) as never)).toBe(false);
  });

  it("asks about unsaved edits when the file changed meanwhile, and drops a board another window took", async () => {
    const other = { project: Q, id: ID };
    store.adoptBoardState(state());
    store.adoptBoardState(boardState(Q, ID));
    tabs.openBoardTab(REF);
    tabs.openBoardTab(other);
    edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    engine.call.mockImplementation((_command: string, args: { project: string }) =>
      Promise.resolve(args.project === P ? { status: "opened", board: state(doc(), "r7") } : { status: "held", heldBy: 3 }),
    );
    await reclaimBoards();
    expect(store.getOpenBoard(REF)?.conflict).toEqual({ diskRevision: "r7" });
    expect(store.getOpenBoard(other)).toBeUndefined();
    expect(tabs.useTabs.getState().boards).toEqual([REF]);
    expect(toast.showToast).toHaveBeenCalledTimes(1);
  });
});
