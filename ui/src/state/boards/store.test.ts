import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardDocument, BoardState } from "../../ipc/generated";
import { UnfoldError } from "../../ipc/unfold";

const engine = vi.hoisted(() => ({ call: vi.fn(), onEngineEvent: vi.fn(() => () => undefined) }));
const windowCommand = vi.hoisted(() => vi.fn(() => Promise.resolve(null)));
vi.mock("./engine", () => engine);
vi.mock("../../ipc/unfold", async (original) => ({ ...(await original<Record<string, unknown>>()), windowCommand }));
vi.mock("../../components/Toast", () => ({ showToast: vi.fn() }));

const { addCard, moveCards, setCardsFolded } = await import("./document");
const store = await import("./store");

function document(cards: BoardDocument["cards"] = []): BoardDocument {
  return { format: "unfold-board", version: 1, title: "Order placement", defaultView: "code", cards, groups: [], labels: {}, guards: {} };
}

function state(id: string, doc = document(), revision = "r1"): BoardState {
  return {
    id,
    place: "draft",
    name: "order-placement",
    document: doc,
    revision,
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

const ID = ".unfold/local/drafts/order-placement.json";

beforeEach(() => {
  vi.useFakeTimers();
  engine.call.mockReset();
  windowCommand.mockClear();
  store.resetBoards();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("opening boards", () => {
  it("claims a board for this window and makes it active", async () => {
    engine.call.mockResolvedValueOnce({ status: "opened", board: state(ID) });
    const opened = vi.fn();
    store.subscribeBoardOpened(opened);
    await expect(store.openBoard(ID)).resolves.toEqual({ status: "opened", id: ID });
    expect(engine.call).toHaveBeenCalledWith("open_board", { id: ID });
    expect(store.getBoard(ID)?.document.title).toBe("Order placement");
    expect(store.useBoards.getState().activeBoardId).toBe(ID);
    expect(opened).toHaveBeenCalledWith(ID);
  });

  it("focuses the window that holds a board instead of opening it", async () => {
    engine.call.mockResolvedValueOnce({ status: "held", heldBy: 7 });
    await expect(store.openBoard(ID)).resolves.toEqual({ status: "held", heldBy: 7 });
    expect(windowCommand).toHaveBeenCalledWith("focus_window", { windowId: 7 });
    expect(store.getBoard(ID)).toBeUndefined();
  });
});

describe("editing", () => {
  it("autosaves once, a moment after the last of several edits, with the latest document", async () => {
    store.adoptBoardState(state(ID));
    engine.call.mockImplementation((_command: string, args: { board: BoardDocument }) => Promise.resolve(state(ID, args.board, "r2")));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    vi.advanceTimersByTime(store.AUTOSAVE_MS - 100);
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    vi.advanceTimersByTime(store.AUTOSAVE_MS - 100);
    expect(engine.call).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(engine.call).toHaveBeenCalledTimes(1);
    const [command, args] = engine.call.mock.calls[0] as [string, { id: string; board: BoardDocument }];
    expect(command).toBe("save_board");
    expect(args.board.cards.map((card) => card.id)).toEqual(["a", "b"]);
    const board = store.getBoard(ID);
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.revision).toBe("r2");
  });

  it("never overlaps saves: an edit made during a save is written right after it", async () => {
    store.adoptBoardState(state(ID));
    let release: (() => void) | undefined;
    engine.call.mockImplementation(
      (_command: string, args: { board: BoardDocument }) =>
        new Promise((resolve) => {
          release = () => { resolve(state(ID, args.board, "r2")); };
        }),
    );
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    const first = store.saveBoard(ID);
    await vi.advanceTimersByTimeAsync(0);
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    const second = store.saveBoard(ID);
    expect(engine.call).toHaveBeenCalledTimes(1);
    release?.();
    await first;
    await vi.advanceTimersByTimeAsync(0);
    expect(engine.call).toHaveBeenCalledTimes(2);
    release?.();
    await second;
    const saved = engine.call.mock.calls[1]?.[1] as { board: BoardDocument };
    expect(saved.board.cards).toHaveLength(2);
    expect(store.isDirty(store.getBoard(ID) as never)).toBe(false);
  });

  it("never overlaps saves even when several callers wait on the same one", async () => {
    store.adoptBoardState(state(ID));
    const releases: (() => void)[] = [];
    let running = 0;
    let most = 0;
    engine.call.mockImplementation(
      (_command: string, args: { board: BoardDocument }) =>
        new Promise((resolve) => {
          running += 1;
          most = Math.max(most, running);
          releases.push(() => {
            running -= 1;
            resolve(state(ID, args.board, "r2"));
          });
        }),
    );
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    const first = store.saveBoard(ID);
    await vi.advanceTimersByTimeAsync(0);
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    const waiting = [store.saveBoard(ID), store.flushBoard(ID), store.saveBoard(ID)];
    while (releases.length > 0) {
      releases.shift()?.();
      await vi.advanceTimersByTimeAsync(0);
    }
    await Promise.all([first, ...waiting]);
    expect(most).toBe(1);
    expect(engine.call).toHaveBeenCalledTimes(2);
    expect(store.isDirty(store.getBoard(ID) as never)).toBe(false);
  });

  it("keeps the identity of everything a save didn't change, so unchanged cards don't redraw", async () => {
    store.adoptBoardState(state(ID, document([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/b.ts#function:b" }, at: [600, 0] }));
    const before = store.getBoard(ID);
    // The engine answers with an equal copy, one card signed.
    engine.call.mockImplementation((_command: string, args: { board: BoardDocument }) => {
      const saved = structuredClone(args.board);
      const second = saved.cards[1];
      if (second) {
        second.description = { text: "Signed", author: "person:a", signature: "sha256:1" };
      }
      return Promise.resolve(state(ID, saved, "r2"));
    });
    await store.saveBoard(ID);
    const after = store.getBoard(ID);
    expect(after?.document).not.toBe(before?.document);
    expect(after?.document.cards[0]).toBe(before?.document.cards[0]);
    expect(after?.document.cards[1]?.description?.text).toBe("Signed");
    expect(after?.health).toBe(before?.health);
    expect(after?.outline).toBe(before?.outline);
  });

  it("sends every unsaved board at once when the page goes away", () => {
    store.adoptBoardState(state(ID));
    engine.call.mockReturnValue(new Promise(() => undefined));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    store.saveAllNow();
    expect(engine.call).toHaveBeenCalledWith("save_board", expect.objectContaining({ id: ID }));
    // The autosave it replaced doesn't run later.
    vi.advanceTimersByTime(store.AUTOSAVE_MS * 2);
    expect(engine.call).toHaveBeenCalledTimes(1);
  });

  it("makes a repair one undo step", () => {
    store.adoptBoardState(state(ID, document([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])));
    store.editBoard(ID, (doc) => { moveCards(doc, { a: [10, 0] }); });
    const beforeRepair = store.getBoard(ID)?.document;
    // Pretend everything was saved, then the engine removes the card as a repair.
    const board = store.getBoard(ID);
    if (board) {
      store.useBoards.setState((draft) => {
        const open = draft.boards[ID];
        if (open) {
          open.savedEdits = open.edits;
        }
      });
    }
    store.applyServerState(ID, state(ID, document([]), "r3"));
    expect(store.getBoard(ID)?.document.cards).toHaveLength(0);
    store.undo(ID);
    expect(store.getBoard(ID)?.document).toEqual(beforeRepair);
    store.undo(ID);
    expect(store.getBoard(ID)?.document.cards[0]?.at).toEqual([0, 0]);
  });

  it("surfaces a failed save and keeps the edit unsaved", async () => {
    store.adoptBoardState(state(ID));
    engine.call.mockRejectedValueOnce(new UnfoldError({ code: "board_problem", message: "missing reference" }));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    await store.saveBoard(ID);
    const board = store.getBoard(ID);
    expect(board?.saveError).toBe("missing reference");
    expect(board && store.isDirty(board)).toBe(true);
  });

  it("undoes and redoes one edit at a time, and merges a coalesced burst", () => {
    store.adoptBoardState(state(ID));
    const id = store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] })) ?? "";
    store.editBoard(ID, (doc) => { moveCards(doc, { [id]: [10, 0] }); }, { coalesce: "drag" });
    store.editBoard(ID, (doc) => { moveCards(doc, { [id]: [20, 0] }); }, { coalesce: "drag" });
    store.editBoard(ID, (doc) => { setCardsFolded(doc, [id], true); });
    expect(store.getBoard(ID)?.history.past).toHaveLength(3);
    store.undo(ID);
    expect(store.getBoard(ID)?.document.cards[0]?.folded).toBeUndefined();
    store.undo(ID);
    expect(store.getBoard(ID)?.document.cards[0]?.at).toEqual([0, 0]);
    store.redo(ID);
    expect(store.getBoard(ID)?.document.cards[0]?.at).toEqual([20, 0]);
    store.undo(ID);
    store.undo(ID);
    expect(store.getBoard(ID)?.document.cards).toHaveLength(0);
    expect(store.canUndo(ID)).toBe(false);
    expect(store.canRedo(ID)).toBe(true);
  });

  it("exports a session whose history is plain JSON and adopts it elsewhere", () => {
    store.adoptBoardState(state(ID));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    const session = store.exportSession(ID);
    const copy = JSON.parse(JSON.stringify(session)) as NonNullable<typeof session>;
    store.resetBoards();
    store.adoptSession(copy);
    expect(store.getBoard(ID)?.document.cards).toHaveLength(1);
    expect(store.canUndo(ID)).toBe(true);
  });
});

describe("outside edits", () => {
  it("reloads a board without unsaved edits", async () => {
    store.adoptBoardState(state(ID));
    const theirs = document([{ id: "x", ref: { block: "src/x.ts#function:x" }, at: [0, 0] }]);
    engine.call.mockResolvedValueOnce(state(ID, theirs, "r9"));
    await store.handleOutsideChange(ID, "r9");
    expect(engine.call).toHaveBeenCalledWith("reload_board", { id: ID });
    expect(store.getBoard(ID)?.document.cards.map((card) => card.id)).toEqual(["x"]);
    expect(store.getBoard(ID)?.conflict).toBeNull();
  });

  it("asks about a board with unsaved edits, then keeps this window's version on request", async () => {
    store.adoptBoardState(state(ID));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    await store.handleOutsideChange(ID, "r9");
    expect(store.getBoard(ID)?.conflict).toEqual({ diskRevision: "r9" });
    // Autosave holds off while the person decides.
    await vi.advanceTimersByTimeAsync(store.AUTOSAVE_MS * 2);
    expect(engine.call).not.toHaveBeenCalled();
    engine.call.mockImplementation((_command: string, args: { board: BoardDocument }) => Promise.resolve(state(ID, args.board, "r10")));
    await store.resolveConflict(ID, "mine");
    expect(engine.call).toHaveBeenCalledWith("save_board", expect.objectContaining({ id: ID, force: true }));
    expect(store.getBoard(ID)?.conflict).toBeNull();
  });
});

describe("engine-host restarts", () => {
  it("claims each board again before writing its unsaved edits", async () => {
    store.adoptBoardState(state(ID));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    engine.call.mockImplementation((command: string, args: { board?: BoardDocument }) =>
      Promise.resolve(command === "open_board" ? { status: "opened", board: state(ID) } : state(ID, args.board, "r2")),
    );
    await store.reclaimBoards();
    expect(engine.call.mock.calls.map(([command]) => command as string)).toEqual(["open_board", "save_board"]);
    expect(store.isDirty(store.getBoard(ID) as never)).toBe(false);
  });

  it("asks about unsaved edits when the file changed while the engine was down", async () => {
    store.adoptBoardState(state(ID));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    engine.call.mockResolvedValueOnce({ status: "opened", board: state(ID, document(), "r7") });
    await store.reclaimBoards();
    expect(store.getBoard(ID)?.conflict).toEqual({ diskRevision: "r7" });
    expect(engine.call).toHaveBeenCalledTimes(1);
  });

  it("forgets every board when the window switches folders", () => {
    store.adoptBoardState(state(ID));
    store.editBoard(ID, (doc) => addCard(doc, { ref: { block: "src/a.ts#function:a" }, at: [0, 0] }));
    store.forgetAllBoards();
    expect(store.useBoards.getState()).toMatchObject({ boards: {}, order: [], activeBoardId: null });
    vi.advanceTimersByTime(store.AUTOSAVE_MS * 2);
    expect(engine.call).not.toHaveBeenCalled();
  });
});

describe("the lens", () => {
  it("overrides every card's view without editing the board, until a card's own switch", () => {
    store.adoptBoardState(state(ID, document([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0], view: "code" }])));
    const before = store.getBoard(ID)?.document;
    store.applyLensToBoards("flow");
    const board = store.getBoard(ID);
    expect(board?.document).toBe(before);
    expect(board?.edits).toBe(0);
    expect(store.effectiveView(board?.view.lens ?? null, "code", "code", false)).toBe("flow");
    expect(store.effectiveView(board?.view.lens ?? null, "code", "code", true)).toBe("code");
    expect(store.effectiveView(null, undefined, "flow", false)).toBe("flow");
  });
});
