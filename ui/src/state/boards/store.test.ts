import { produce, type Draft } from "immer";
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

const { addCard, moveItems, setCardsFolded, setTitle } = await import("./document");
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
    store.setPageLens(PAGE, "code");
    store.saveAllNow();
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["save_board", "save_board_view"]);
    // The autosaves it replaced don't run later.
    vi.advanceTimersByTime(store.VIEW_SAVE_MS * 2);
    expect(engine.call).toHaveBeenCalledTimes(2);
  });
});

describe("a step saved at once (an agent's edit)", () => {
  const A = { block: "src/a.ts#function:a" } as const;
  const addA = (document: BoardDocument) => ({
    document: produce(document, (draft) => {
      const target = draft.pages[0];
      if (target) {
        addCard(target, { ref: A, at: [0, 0] });
      }
    }),
    value: "added",
  });

  it("saves without waiting for the autosave, and becomes one named undo step once the engine accepts it", async () => {
    store.adoptBoardState(state());
    engine.call.mockImplementation(savesAs("r2"));
    const outcome = await store.saveStep(REF, "page-1", "Claude · Add cards", addA);
    expect(outcome).toMatchObject({ ok: true, value: "added", state: { revision: "r2" } });
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["save_board"]);
    const board = store.getOpenBoard(REF);
    expect(board?.document.pages[0]?.cards.map((card) => card.id)).toEqual(["a"]);
    expect(board?.history.past).toEqual([{ document: doc(), page: "page-1", name: "Claude · Add cards" }]);
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.revision).toBe("r2");
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toEqual([]);
    // Undo and redo keep the step's name.
    expect(store.getOpenBoard(REF)?.history.future.map((entry) => entry.name)).toEqual(["Claude · Add cards"]);
    store.redo(REF);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Add cards"]);
  });

  it("leaves the board as it was when the engine refuses, and says why", async () => {
    store.adoptBoardState(state());
    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "board_problem", message: "The key is gone." }));
    const outcome = await store.saveStep(REF, "page-1", "Claude · Add cards", addA);
    expect(outcome).toEqual({ ok: false, error: new UnfoldError({ code: "board_problem", message: "The key is gone." }) });
    const board = store.getOpenBoard(REF);
    expect(board?.document).toEqual(doc());
    expect(board?.history).toMatchObject({ past: [], future: [] });
    expect(board?.saveError).toBeNull();
    expect(toast.showToast).not.toHaveBeenCalled();
  });

  it("saves the person's pending edit first, and refuses while their edits can't be saved or a conflict is open", async () => {
    store.adoptBoardState(state());
    engine.call.mockImplementation(savesAs("r2"));
    edit((draft) => addCard(draft, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    await store.saveStep(REF, "page-1", "Claude · Add cards", addA);
    const saves = engine.call.mock.calls.map(([, args]) => (args as SaveBoardArgs).board.pages[0]?.cards.map((card) => card.id));
    expect(saves).toEqual([["b"], ["b", "a"]]);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual([undefined, "Claude · Add cards"]);

    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "io", message: "Disk full." }));
    edit((draft) => addCard(draft, { ref: { block: "src/c.ts#function:c" }, at: [900, 0] }));
    expect(await store.saveStep(REF, "page-1", "Claude · Add cards", addA)).toMatchObject({ ok: false, error: { code: "unsaved_edits", message: "Disk full." } });
  });

  it("applies the change again over edits the person made while it saved, saves that, and reports what it applied", async () => {
    store.adoptBoardState(state());
    const releases: (() => void)[] = [];
    engine.call.mockImplementation((_command: string, args: SaveBoardArgs) => {
      const revision = `r${releases.length + 2}`;
      return new Promise((resolve) => {
        releases.push(() => {
          resolve(state({ ...doc(), pages: args.board.pages as BoardDocument["pages"] }, revision));
        });
      });
    });
    const addOne = (document: BoardDocument) => {
      let id = "";
      const next = produce(document, (draft) => {
        const target = draft.pages[0];
        if (target) {
          id = addCard(target, { ref: A, at: [0, 0] });
        }
      });
      return { document: next, value: id };
    };
    const outcome = store.saveStep(REF, "page-1", "Claude · Add cards", addOne);
    await vi.advanceTimersByTimeAsync(0);
    // The person adds a card for the same function meanwhile: theirs takes the id `a`.
    edit((draft) => addCard(draft, { ref: A, at: [600, 0] }));
    releases[0]?.();
    await vi.advanceTimersByTimeAsync(0);
    releases[1]?.();
    const result = await outcome;
    expect(result).toMatchObject({ ok: true, value: "a-2", state: { revision: "r3" } });
    expect(result.ok && result.state.document.pages[0]?.cards.map((card) => [card.id, card.at[0]])).toEqual([
      ["a", 600],
      ["a-2", 0],
    ]);
    const saves = engine.call.mock.calls.map(([, args]) => (args as SaveBoardArgs).board.pages[0]?.cards.map((card) => card.id));
    expect(saves).toEqual([["a"], ["a", "a-2"]]);
    const board = store.getOpenBoard(REF);
    expect(board?.document).toEqual(result.ok ? result.state.document : null);
    expect(board?.history.past.map((entry) => entry.name)).toEqual([undefined, "Claude · Add cards"]);
    expect(board && store.isDirty(board)).toBe(false);
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect(engine.call).toHaveBeenCalledTimes(2);
  });

  it("refuses when the change no longer applies over the person's edits, leaving their edits to save", async () => {
    store.adoptBoardState(state(doc([{ id: "a", ref: A, at: [0, 0] }])));
    let release: (() => void) | undefined;
    engine.call.mockImplementationOnce(
      (_command: string, args: SaveBoardArgs) =>
        new Promise((resolve) => {
          release = () => {
            resolve(state({ ...doc(), pages: args.board.pages as BoardDocument["pages"] }, "r2"));
          };
        }),
    );
    const foldA = (document: BoardDocument) => {
      if (!document.pages[0]?.cards.some((card) => card.id === "a")) {
        throw new Error("the card is gone");
      }
      return {
        document: produce(document, (draft) => {
          const target = draft.pages[0];
          if (target) {
            setCardsFolded(target, ["a"], true);
          }
        }),
        value: "folded",
      };
    };
    const outcome = store.saveStep(REF, "page-1", "Claude · Fold", foldA);
    await vi.advanceTimersByTimeAsync(0);
    edit((draft) => {
      draft.cards = [];
    });
    release?.();
    expect(await outcome).toMatchObject({ ok: false, error: { code: "page_changed" } });
    const board = store.getOpenBoard(REF);
    expect(board?.document.pages[0]?.cards).toEqual([]);
    expect(board?.history.past.map((entry) => entry.name)).toEqual([undefined]);
    expect(board && store.isDirty(board)).toBe(true);
    engine.call.mockImplementation(savesAs("r3"));
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect((engine.call.mock.calls[1]?.[1] as SaveBoardArgs).board.pages[0]?.cards).toEqual([]);
  });

  it("never runs two saves at once: a second step waits for the first", async () => {
    store.adoptBoardState(state());
    engine.call.mockImplementation(savesAs("r2"));
    const addB = (document: BoardDocument) => ({
      document: produce(document, (draft) => {
        const target = draft.pages[0];
        if (target) {
          addCard(target, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] });
        }
      }),
      value: "b",
    });
    const [first, second] = await Promise.all([store.saveStep(REF, "page-1", "Claude · A", addA), store.saveStep(REF, "page-1", "Codex · B", addB)]);
    expect([first.ok, second.ok]).toEqual([true, true]);
    const saves = engine.call.mock.calls.map(([, args]) => (args as SaveBoardArgs).board.pages[0]?.cards.map((card) => card.id));
    expect(saves).toEqual([["a"], ["a", "b"]]);
  });
});

describe("undo and redo", () => {
  it("undoes one edit at a time, merges a coalesced burst, and keeps at most 200 steps", () => {
    store.adoptBoardState(state());
    const id = edit((draft) => addCard(draft, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] })) ?? "";
    store.editPage(PAGE, (draft) => { moveItems(draft, { [id]: [10, 0] }); }, { coalesce: "drag" });
    store.editPage(PAGE, (draft) => { moveItems(draft, { [id]: [20, 0] }); }, { coalesce: "drag" });
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
    expect(store.getOpenBoard(REF)?.history.future).toHaveLength(3);
    for (let step = 0; step < 250; step += 1) {
      store.editBoard(REF, (document) => { setTitle(document, `Title ${step}`); });
    }
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(store.HISTORY_LIMIT);
  });

  it("keeps one history across the board's pages, and undo or redo shows the page each step changed", () => {
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

  it("makes a repair one undo step, after saving the edits waiting for the autosave", async () => {
    store.adoptBoardState(state(doc([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])));
    engine.call.mockImplementation(savesAs("r2"));
    edit((draft) => { moveItems(draft, { a: [10, 0] }); });
    const beforeRepair = store.getOpenBoard(REF)?.document;
    const outcome = await store.engineStep(REF, "page-1", () => Promise.resolve(state(doc([]), "r3")));
    expect(outcome).toMatchObject({ ok: true, state: { revision: "r3" } });
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["save_board"]);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(0);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document).toEqual(beforeRepair);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.at).toEqual([0, 0]);
  });
});

describe("a step the engine writes (a repair)", () => {
  const CARD = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] as [number, number] };
  const repaired = () => Promise.resolve(state(doc([]), "r3"));

  it("refuses while the board's edits can't be saved or a conflict is open, without asking the engine", async () => {
    store.adoptBoardState(state(doc([CARD])));
    const write = vi.fn(repaired);
    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "io", message: "Disk full." }));
    edit((draft) => { moveItems(draft, { a: [10, 0] }); });
    expect(await store.engineStep(REF, "page-1", write)).toMatchObject({ ok: false, error: { code: "unsaved_edits", message: "Disk full." } });
    store.useBoards.setState((draft) => {
      const open = Object.values(draft.boards)[0];
      if (open) {
        open.conflict = { diskRevision: "r9" };
      }
    });
    expect(await store.engineStep(REF, "page-1", write)).toMatchObject({ ok: false, error: { code: "board_conflict" } });
    expect(write).not.toHaveBeenCalled();
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.at).toEqual([10, 0]);
  });

  it("keeps edits the person makes while it runs, unsaved over its state, and says the repair wasn't kept", async () => {
    store.adoptBoardState(state(doc([CARD])));
    let finish: (() => void) | undefined;
    const write = () =>
      new Promise<BoardState>((resolve) => {
        finish = () => {
          resolve(state(doc([]), "r3"));
        };
      });
    const outcome = store.engineStep(REF, "page-1", write);
    await vi.advanceTimersByTimeAsync(0);
    edit((draft) => { moveItems(draft, { a: [10, 0] }); });
    finish?.();
    expect(await outcome).toMatchObject({ ok: false, error: { code: "page_changed" } });
    const board = store.getOpenBoard(REF);
    expect(board?.document.pages[0]?.cards[0]?.at).toEqual([10, 0]);
    expect(board?.savedDocument.pages[0]?.cards).toEqual([]);
    expect(board?.revision).toBe("r3");
    expect(board?.history.past).toHaveLength(1);
    expect(board && store.isDirty(board)).toBe(true);
    engine.call.mockImplementation(savesAs("r4"));
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS);
    expect((engine.call.mock.calls[0]?.[1] as SaveBoardArgs).board.pages[0]?.cards[0]?.at).toEqual([10, 0]);
  });
});

describe("view state", () => {
  it("saves the active page and each page's camera and lens once they settle, never as undo steps", async () => {
    store.adoptBoardState(state(boardDocument([page("page-1"), page("page-2")])));
    engine.call.mockResolvedValue(null);
    store.setCamera(PAGE, { x: 10, y: 20, zoom: 0.8 });
    store.setActivePage(REF, "page-2");
    store.setPageLens({ board: REF, page: "page-2" }, "code");
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(store.VIEW_SAVE_MS);
    expect(engine.call).toHaveBeenCalledTimes(1);
    expect(engine.call).toHaveBeenCalledWith("save_board_view", {
      project: P,
      id: ID,
      view: { page: "page-2", pages: { "page-1": { x: 10, y: 20, zoom: 0.8, lens: null }, "page-2": { x: 40, y: 40, zoom: 1, lens: "code" } } },
    });
    expect(store.getOpenBoard(REF)?.viewDirty).toBe(false);
  });

  it("shows Code in the Code lens, and each card compact or expanded in Diagram", () => {
    store.adoptBoardState(state());
    // No lens picked: the page's default decides; only a Code default opens Code.
    expect(store.pageLens(null, "code")).toBe("code");
    expect(store.pageLens(null, "flow")).toBe("diagram");
    expect(store.pageLens(null, "diagram")).toBe("diagram");
    // A saved Flow lens is Diagram.
    expect(store.pageLens("flow", "code")).toBe("diagram");
    expect(store.shownView("code", { view: "diagram" }, "diagram")).toBe("code");
    // In Diagram a card's own view, else the page's default, says whether it is expanded; a legacy Code card is.
    expect(store.shownView("diagram", { view: "flow" }, "diagram")).toBe("flow");
    expect(store.shownView("diagram", { view: "code" }, "diagram")).toBe("flow");
    expect(store.shownView("diagram", {}, "flow")).toBe("flow");
    expect(store.shownView("diagram", {}, "code")).toBe("flow");
    expect(store.shownView("diagram", { view: "diagram" }, "flow")).toBe("diagram");
    expect(store.shownView(null, {}, "diagram")).toBe("diagram");
    store.setPageLens(PAGE, "code");
    expect(store.getPageView(PAGE).lens).toBe("code");
    expect(store.getOpenBoard(REF)?.edits).toBe(0);
  });

  it("restores a saved Flow lens as Diagram without writing it back, and keeps camera, selection, and expansions across Code", async () => {
    const card = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] as [number, number], view: "flow" as const, expanded: [["src/a.ts#function:a/call[0]"]] };
    store.adoptBoardState(state(doc([card]), "r1", { view: { page: "page-1", pages: { "page-1": { x: 1, y: 2, zoom: 0.5, lens: "flow" } } } }));
    const { defaultView } = store.getOpenBoard(REF)?.document.pages[0] ?? { defaultView: "code" as const };
    expect(store.pageLens(store.getPageView(PAGE).lens, defaultView)).toBe("diagram");
    expect(store.getOpenBoard(REF)?.viewDirty).toBe(false);
    await vi.advanceTimersByTimeAsync(store.VIEW_SAVE_MS * 2);
    expect(engine.call).not.toHaveBeenCalled();

    store.select(PAGE, { kind: "card", cardId: "a", key: "src/a.ts#function:a/call[0]" });
    const document = store.getOpenBoard(REF)?.document;
    const selection = store.getPageView(PAGE).selection;
    for (const lens of ["code", "diagram"] as const) {
      store.setPageLens(PAGE, lens);
      expect(store.pageLens(store.getPageView(PAGE).lens, defaultView)).toBe(lens);
      expect(store.getPageView(PAGE).camera).toEqual({ x: 1, y: 2, zoom: 0.5 });
      expect(store.getPageView(PAGE).selection).toBe(selection);
      expect(store.getOpenBoard(REF)?.document).toBe(document);
    }
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]).toMatchObject({ view: "flow", expanded: [["src/a.ts#function:a/call[0]"]] });
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
