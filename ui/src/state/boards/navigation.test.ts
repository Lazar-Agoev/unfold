import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardDocument, BoardState, CardView } from "../../ipc/generated";

const engine = vi.hoisted(() => ({ call: vi.fn(), onEngineEvent: vi.fn(() => () => undefined) }));
vi.mock("./engine", () => engine);
vi.mock("../../components/Toast", () => ({ showToast: vi.fn() }));

const store = await import("./store");
const { followCall, extendTrail } = await import("./navigation");
const { openEntrance, openBlockCard } = await import("./api");
const { arrangeBoard } = await import("../../canvas/arrangeBoard");
const { FOLLOW_GAP, HEADER_PORT } = await import("../../canvas/layout");

const ID = ".unfold/local/drafts/b.json";

function state(cards: BoardDocument["cards"]): BoardState {
  return {
    id: ID,
    place: "draft",
    name: "b",
    document: { format: "unfold-board", version: 1, title: "B", defaultView: "code", cards, groups: [], labels: {}, guards: {} },
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

beforeEach(() => {
  engine.call.mockReset();
  engine.call.mockImplementation((_command: string, args: { board?: BoardDocument }) => Promise.resolve(args.board ? { ...state([]), document: args.board } : null));
  store.resetBoards();
});

describe("following a call", () => {
  it("opens the target 80 px right of the calling card, its header level with the calling line", () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: "src/routes/orders.ts#function:createOrder" }, at: [100, 50] }]));
    store.updateView(ID, (view) => {
      view.sizes["create-order"] = { width: 520, height: 400 };
    });
    const lineOffset = 190;
    const id = followCall({
      boardId: ID,
      fromCardId: "create-order",
      fromLabel: "createOrder",
      targetKey: "src/services/order-service.ts#class:OrderService/method:place",
      targetLabel: "OrderService.place",
      lineOffset,
      line: 32,
    });
    const card = store.getBoard(ID)?.document.cards.find((item) => item.id === id);
    expect(card?.at).toEqual([100 + 520 + FOLLOW_GAP, 50 + lineOffset - HEADER_PORT]);
    expect(card?.ref).toEqual({ block: "src/services/order-service.ts#class:OrderService/method:place" });
    expect(card?.view).toBeUndefined();
    const view = store.getBoard(ID)?.view;
    expect(view?.trail).toEqual([
      { cardId: "create-order", label: "createOrder", line: 32 },
      { cardId: id, label: "OrderService.place" },
    ]);
    expect(view?.selectedCardId).toBe(id);
    expect(view?.back).toHaveLength(1);
  });

  it("focuses and unfolds a target that is already on the board instead of adding it", () => {
    store.adoptBoardState(
      state([
        { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
        { id: "b", ref: { block: "src/b.ts#function:b" }, at: [700, 0], folded: true },
      ]),
    );
    const id = followCall({ boardId: ID, fromCardId: "a", fromLabel: "a", targetKey: "src/b.ts#function:b", targetLabel: "b" });
    expect(id).toBe("b");
    const board = store.getBoard(ID);
    expect(board?.document.cards).toHaveLength(2);
    expect(board?.document.cards[1]?.folded).toBeUndefined();
    expect(board?.view.focus?.cardIds).toEqual(["b"]);
  });

  it("Back and Forward walk the focus moves", () => {
    store.adoptBoardState(state([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }]));
    store.updateView(ID, (view) => {
      view.trail = [{ cardId: "a", label: "a" }];
    });
    followCall({ boardId: ID, fromCardId: "a", fromLabel: "a", targetKey: "src/b.ts#function:b", targetLabel: "b" });
    expect(store.getBoard(ID)?.view.trail.map((item) => item.label)).toEqual(["a", "b"]);
    const b = store.getBoard(ID)?.view.trail[1]?.cardId ?? "";
    followCall({ boardId: ID, fromCardId: b, fromLabel: "b", targetKey: "src/c.ts#function:c", targetLabel: "c" });
    expect(store.getBoard(ID)?.view.trail.map((item) => item.label)).toEqual(["a", "b", "c"]);
    store.travel(ID, "back");
    store.travel(ID, "back");
    expect(store.getBoard(ID)?.view.trail.map((item) => item.label)).toEqual(["a"]);
    store.travel(ID, "forward");
    expect(store.getBoard(ID)?.view.trail.map((item) => item.label)).toEqual(["a", "b"]);
  });

  it("adds a source that isn't on the trail after it", () => {
    expect(extendTrail([{ cardId: "e", label: "POST /orders" }], { cardId: "c", label: "createOrder" }, 32, { cardId: "p", label: "OrderService.place" }).map((item) => item.label)).toEqual([
      "POST /orders",
      "createOrder",
      "OrderService.place",
    ]);
  });

  it("truncates the trail after the card the follow started from", () => {
    const trail = [
      { cardId: "e", label: "POST /orders" },
      { cardId: "c", label: "createOrder", line: 32 },
      { cardId: "p", label: "OrderService.place" },
    ];
    expect(extendTrail(trail, { cardId: "c", label: "createOrder" }, 33, { cardId: "s", label: "slack.post" }).map((item) => item.label)).toEqual([
      "POST /orders",
      "createOrder",
      "slack.post",
    ]);
  });
});

describe("opening from the sidebar", () => {
  it("opens an entrance card and one full card per resolved handler, creating a draft when none is open", async () => {
    const entrance: CardView = {
      type: "entrance",
      header: { key: "entrance:x", kind: "http", title: "POST /orders", label: null, method: "POST", trigger: "/orders", triggerStatus: "exact", detector: "express.route", language: "typescript" },
      evidence: { path: "src/server.ts", startLine: 3, endLine: 3, pointer: null, source: "current", code: null, text: null },
      targets: [
        { position: 0, role: "handler", text: "requireUser", resolution: "resolved", target: "src/m.ts#function:requireUser", name: "requireUser", path: "src/m.ts", line: 1, note: null, reason: null, module: null },
        { position: 1, role: "handler", text: "audit", resolution: "external", target: null, name: null, path: null, line: null, note: "audit-lib", reason: null, module: "audit-lib" },
        { position: 2, role: "handler", text: "createOrder", resolution: "resolved", target: "src/o.ts#function:createOrder", name: "createOrder", path: "src/o.ts", line: 5, note: null, reason: null, module: null },
      ],
      applications: [],
    };
    engine.call.mockImplementation((command: string) => Promise.resolve(command === "new_board" ? state([]) : command === "card" ? entrance : null));
    openEntrance("entrance:x");
    await vi.waitFor(() => {
      expect(store.getBoard(ID)?.document.cards).toHaveLength(3);
    });
    expect(engine.call).toHaveBeenCalledWith("new_board", { title: "POST /orders" });
    const cards = store.getBoard(ID)?.document.cards ?? [];
    expect(cards.map((card) => card.ref)).toEqual([
      { entrance: "entrance:x" },
      { block: "src/m.ts#function:requireUser" },
      { block: "src/o.ts#function:createOrder" },
    ]);
    expect(store.getBoard(ID)?.view.trail).toEqual([{ cardId: cards[0]?.id, label: "POST /orders" }]);
  });

  it("opens a definition once; opening it again focuses it", async () => {
    store.adoptBoardState(state([]));
    openBlockCard("src/a.ts#function:a");
    await vi.waitFor(() => {
      expect(store.getBoard(ID)?.document.cards).toHaveLength(1);
    });
    openBlockCard("src/a.ts#function:a");
    await vi.waitFor(() => {
      expect(store.getBoard(ID)?.view.focus?.cardIds).toEqual(["a"]);
    });
    expect(store.getBoard(ID)?.document.cards).toHaveLength(1);
  });

  it("brings the board's tab forward when a source tab is showing", async () => {
    const tabs = await import("../tabs");
    store.adoptBoardState(state([]));
    tabs.openBoardTab(ID);
    tabs.openSource("src/a.ts");
    expect(tabs.activeTab()?.kind).toBe("source");
    openBlockCard("src/a.ts#function:a");
    await vi.waitFor(() => {
      expect(store.getBoard(ID)?.document.cards).toHaveLength(1);
    });
    expect(tabs.activeTab()).toMatchObject({ kind: "board", boardId: ID });
  });
});

describe("Arrange", () => {
  it("lays cards out by call depth in one undoable step", () => {
    store.adoptBoardState(
      state([
        { id: "c", ref: { block: "src/c.ts#function:c" }, at: [0, 900] },
        { id: "b", ref: { block: "src/b.ts#function:b" }, at: [50, 500] },
        { id: "e", ref: { entrance: "entrance:e" }, at: [900, 0] },
      ]),
    );
    const before = store.getBoard(ID)?.document.cards.map((card) => card.at);
    const arrows = [
      { id: "1", source: "e", sourceHandle: "target:0", target: "b", kind: "handler" as const, dashed: false, order: 0, line: null },
      { id: "2", source: "b", sourceHandle: "src/b.ts#function:b/call[0]", target: "c", kind: "calls" as const, dashed: false, order: 0, line: 3 },
    ];
    const moved = arrangeBoard(ID, arrows, () => undefined);
    expect(moved).toBe(3);
    const at = Object.fromEntries((store.getBoard(ID)?.document.cards ?? []).map((card) => [card.id, card.at]));
    expect(at["e"]?.[0]).toBeLessThan(at["b"]?.[0] ?? 0);
    expect(at["b"]?.[0]).toBeLessThan(at["c"]?.[0] ?? 0);
    // Arranging again changes nothing.
    expect(arrangeBoard(ID, arrows, () => undefined)).toBe(0);
    store.undo(ID);
    expect(store.getBoard(ID)?.document.cards.map((card) => card.at)).toEqual(before);
  });
});
