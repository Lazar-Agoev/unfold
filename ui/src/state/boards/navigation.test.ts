import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../../ipc/client";
import type { BoardDocument, Card, CardView } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("./engine", () => engine);
vi.mock("../../components/Toast", () => toast);

const store = await import("./store");
const { followCall, extendTrail } = await import("./navigation");
const { addBlock, addLink, entranceCardText, focusFrame, labelNode, openEntrance, removeCard, traceEntrance } = await import("./api");
const { arrangeBoard } = await import("../../canvas/arrangeBoard");
const { FOLLOW_GAP, HEADER_PORT, STACK_GAP, fallbackSize } = await import("../../canvas/layout");
const { estimateText } = await import("../../canvas/measure");
const { entranceCardWidth, entranceHandlerRows } = await import("../../canvas/sizing");
const { getShownBoard, useShownBoardStore } = await import("../shownBoard");

const P = "p-orders0000";
const Q = "p-payments00";
const SAME = "p-orders2000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };

function open(cards: Card[] = [], document?: BoardDocument): void {
  store.adoptBoardState(boardState(P, ID, document ?? boardDocument([page("page-1", cards)])));
}

function cards(): readonly Card[] {
  return store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
}

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

beforeEach(() => {
  engine.call.mockReset();
  engine.call.mockImplementation(() => Promise.resolve(null));
  toast.showToast.mockClear();
  store.resetBoards();
  queryClient.clear();
  // Two projects share the name "orders": the first in workspace order is the one refs name (shared project names).
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders"), projectRow(Q, "payments"), projectRow(SAME, "orders")]));
  useShownBoardStore.setState({ board: null });
});

describe("following a call", () => {
  it("opens the target 80 px right of the calling card, its header level with the calling line", () => {
    open([{ id: "create-order", ref: { block: "src/routes/orders.ts#function:createOrder" }, at: [100, 50] }]);
    store.updatePageView(PAGE, (view) => {
      view.sizes["create-order"] = { width: 520, height: 400 };
    });
    const lineOffset = 190;
    const id = followCall({
      target: PAGE,
      fromCardId: "create-order",
      fromLabel: "createOrder",
      targetKey: "src/services/order-service.ts#class:OrderService/method:place",
      targetLabel: "OrderService.place",
      lineOffset,
      line: 32,
    });
    const card = cards().find((item) => item.id === id);
    expect(card?.at).toEqual([100 + 520 + FOLLOW_GAP, 50 + lineOffset - HEADER_PORT]);
    expect(card?.ref).toEqual({ block: "src/services/order-service.ts#class:OrderService/method:place" });
    const view = store.getPageView(PAGE);
    expect(view.trail).toEqual([
      { cardId: "create-order", label: "createOrder", line: 32 },
      { cardId: id, label: "OrderService.place" },
    ]);
    expect(view.selection).toEqual({ kind: "card", cardId: id, key: null });
    expect(view.back).toHaveLength(1);
  });

  it("stays in the calling card's project", () => {
    open([{ id: "charge", ref: { block: "src/pay.ts#function:charge", project: "payments" }, at: [0, 0] }]);
    const id = followCall({ target: PAGE, fromCardId: "charge", fromLabel: "charge", targetKey: "src/stripe.ts#function:post", targetLabel: "post" });
    expect(cards().find((item) => item.id === id)?.ref).toEqual({ block: "src/stripe.ts#function:post", project: "payments" });
  });

  it("focuses and unfolds a target that is already on the page instead of adding it", () => {
    open([
      { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [700, 0], folded: true },
    ]);
    const id = followCall({ target: PAGE, fromCardId: "a", fromLabel: "a", targetKey: "src/b.ts#function:b", targetLabel: "b" });
    expect(id).toBe("b");
    expect(cards()).toHaveLength(2);
    expect(cards()[1]?.folded).toBeUndefined();
    expect(store.getPageView(PAGE).focus?.cardIds).toEqual(["b"]);
  });

  it("Back and Forward walk the focus moves", () => {
    open([{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }]);
    store.updatePageView(PAGE, (view) => {
      view.trail = [{ cardId: "a", label: "a" }];
    });
    followCall({ target: PAGE, fromCardId: "a", fromLabel: "a", targetKey: "src/b.ts#function:b", targetLabel: "b" });
    const b = store.getPageView(PAGE).trail[1]?.cardId ?? "";
    followCall({ target: PAGE, fromCardId: b, fromLabel: "b", targetKey: "src/c.ts#function:c", targetLabel: "c" });
    expect(store.getPageView(PAGE).trail.map((item) => item.label)).toEqual(["a", "b", "c"]);
    store.travel(PAGE, "back");
    store.travel(PAGE, "back");
    expect(store.getPageView(PAGE).trail.map((item) => item.label)).toEqual(["a"]);
    store.travel(PAGE, "forward");
    expect(store.getPageView(PAGE).trail.map((item) => item.label)).toEqual(["a", "b"]);
  });

  it("extends the trail after its source, adding a source that isn't on it", () => {
    expect(extendTrail([{ cardId: "e", label: "POST /orders" }], { cardId: "c", label: "createOrder" }, 32, { cardId: "p", label: "OrderService.place" }).map((item) => item.label)).toEqual([
      "POST /orders",
      "createOrder",
      "OrderService.place",
    ]);
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

describe("adding cards from any project", () => {
  it("adds a definition once, shows its board and page, and focuses it the second time", () => {
    open([], boardDocument([page("page-1"), page("page-2")]));
    const second = { board: REF, page: "page-2" };
    const id = addBlock(second, P, "src/a.ts#function:a");
    expect(id).toBe("a");
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(getShownBoard()).toEqual(REF);
    expect(addBlock(second, P, "src/a.ts#function:a")).toBe("a");
    expect(store.getOpenBoard(REF)?.document.pages[1]?.cards).toHaveLength(1);
    expect(store.getPageView(second).focus?.cardIds).toEqual(["a"]);
  });

  it("names another project by its shared name, and keeps the same key from two projects apart", () => {
    open();
    addBlock(PAGE, P, "src/a.ts#function:a");
    addBlock(PAGE, Q, "src/a.ts#function:a");
    expect(cards().map((card) => card.ref)).toEqual([{ block: "src/a.ts#function:a" }, { block: "src/a.ts#function:a", project: "payments" }]);
    expect(new Set(cards().map((card) => card.id)).size).toBe(2);
  });

  it("refuses a card from a project with the board project's own name", () => {
    open();
    expect(addBlock(PAGE, SAME, "src/a.ts#function:a")).toBeUndefined();
    expect(cards()).toHaveLength(0);
    expect(toast.showToast).toHaveBeenCalledTimes(1);
  });

  it("labels a node of another project under that project's name", () => {
    open();
    labelNode(PAGE, Q, "src/a.ts#function:a/if[0]", "Paid?", "person:a");
    labelNode(PAGE, P, "src/a.ts#function:a/if[0]", "Valid?", "person:a");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.labels).toEqual([
      { key: "src/a.ts#function:a/if[0]", text: "Valid?", author: "person:a", signature: "" },
      { project: "payments", key: "src/a.ts#function:a/if[0]", text: "Paid?", author: "person:a", signature: "" },
    ]);
  });

  it("draws links between two cards and removes them with either card", () => {
    open([
      { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
      { id: "b", ref: { block: "src/b.ts#function:b", project: "payments" }, at: [600, 0] },
    ]);
    expect(addLink(PAGE, "a", "a")).toBeUndefined();
    expect(addLink(PAGE, "a", "b")).toBe("a-to-b");
    removeCard(PAGE, "b");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.links).toEqual([]);
  });

  it("opens an entrance card and one full card per resolved handler, in its project", async () => {
    open();
    engine.call.mockImplementation((command: string) => Promise.resolve(command === "card" ? entrance : null));
    await openEntrance(PAGE, Q, "entrance:x");
    expect(engine.call).toHaveBeenCalledWith("card", { project: Q, key: "entrance:x" });
    expect(cards().map((card) => card.ref)).toEqual([
      { entrance: "entrance:x", project: "payments" },
      { block: "src/m.ts#function:requireUser", project: "payments" },
      { block: "src/o.ts#function:createOrder", project: "payments" },
    ]);
    expect(store.getPageView(PAGE).trail).toEqual([{ cardId: cards()[0]?.id, label: "POST /orders" }]);
  });

  it("places the handlers beside the entrance, level with their rows, in the step that opens them", async () => {
    open();
    engine.call.mockImplementation((command: string) => Promise.resolve(command === "card" ? entrance : null));
    await openEntrance(PAGE, P, "entrance:x");
    const text = entranceCardText(entrance);
    const width = entranceCardWidth(text, estimateText);
    const rows = entranceHandlerRows(text, false, width, estimateText);
    const [first, second] = [rows[0] ?? 0, rows[2] ?? 0];
    const top = first - HEADER_PORT;
    expect(cards().map((card) => card.at)).toEqual([
      [0, 0],
      [width + FOLLOW_GAP, top],
      // createOrder's row (after the external audit) is above requireUser's bottom, so it stacks below it.
      [width + FOLLOW_GAP, Math.max(second - HEADER_PORT, top + fallbackSize(false).height + STACK_GAP)],
    ]);
    expect(store.getPageView(PAGE).focus?.cardIds).toEqual(cards().map((card) => card.id));
    // One undo step; nothing is left to move the cards later.
    store.undo(REF);
    expect(cards()).toEqual([]);
  });

  it("traces an entrance into a new draft titled after it", async () => {
    const draft = { project: Q, id: ".unfold/local/drafts/trace.json" };
    engine.call.mockImplementation((command: string) =>
      Promise.resolve(command === "card" ? entrance : command === "new_board" ? boardState(Q, draft.id, boardDocument([page("page-1")], { title: "POST /orders" })) : null),
    );
    await expect(traceEntrance(Q, "entrance:x")).resolves.toEqual(draft);
    expect(engine.call).toHaveBeenCalledWith("new_board", { project: Q, title: "POST /orders" });
    expect(store.getOpenBoard(draft)?.document.pages[0]?.cards).toHaveLength(3);
    expect(getShownBoard()).toEqual(draft);
  });
});

describe("bringing a frame into view", () => {
  it("selects it and expands the collapsed frames around it in one undo step; it stays collapsed itself", () => {
    const a: Card = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] };
    open(
      [a],
      boardDocument([
        page("page-1", [a], {
          frames: [
            { id: "inner", title: "Inner", members: ["a"], look: "box", collapsed: true, author: "person:lazar" },
            { id: "outer", title: "Outer", members: ["inner"], look: "zone", collapsed: true, author: "person:lazar" },
          ],
        }),
      ]),
    );
    const collapsed = () => store.getOpenBoard(REF)?.document.pages[0]?.frames?.map((frame) => frame.collapsed === true);
    focusFrame(PAGE, "inner");
    expect(collapsed()).toEqual([true, false]);
    expect(store.getPageView(PAGE)).toMatchObject({ selection: { kind: "frame", frameId: "inner" }, focus: { cardIds: ["inner"], mode: "fit" } });
    store.undo(REF);
    expect(collapsed()).toEqual([true, true]);
  });
});

describe("Arrange", () => {
  it("lays a page out by call depth in one undoable step", () => {
    open([
      { id: "c", ref: { block: "src/c.ts#function:c" }, at: [0, 900] },
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [50, 500] },
      { id: "e", ref: { entrance: "entrance:e" }, at: [900, 0] },
    ]);
    const before = cards().map((card) => card.at);
    const arrows = [
      { id: "1", source: "e", sourceHandle: "target:0", target: "b", project: "p", occurrence: "target:0", seq: 0, kind: "handler" as const, dashed: false, order: 0, line: null },
      { id: "2", source: "b", sourceHandle: "src/b.ts#function:b/call[0]", target: "c", project: "p", occurrence: "src/b.ts#function:b/call[0]", seq: 0, kind: "calls" as const, dashed: false, order: 0, line: 3 },
    ];
    expect(arrangeBoard(PAGE, arrows, new Map(), () => undefined)).toBe(3);
    const at = Object.fromEntries(cards().map((card) => [card.id, card.at]));
    expect(at["e"]?.[0]).toBeLessThan(at["b"]?.[0] ?? 0);
    expect(at["b"]?.[0]).toBeLessThan(at["c"]?.[0] ?? 0);
    // Arranging again changes nothing.
    expect(arrangeBoard(PAGE, arrows, new Map(), () => undefined)).toBe(0);
    store.undo(REF);
    expect(cards().map((card) => card.at)).toEqual(before);
  });

  it("arranges a page of resource cards alone (a data model of tables)", () => {
    const table = (id: string, at: [number, number]) => ({ id, kind: "table" as const, name: id, anchors: [], at, author: "agent:claude" });
    open([], boardDocument([page("page-1", [], { resources: [table("orders", [0, 2000]), table("customers", [5000, 0])] })]));
    expect(arrangeBoard(PAGE, [], new Map(), () => undefined)).toBeGreaterThan(0);
  });

  it("sizes cards it hasn't measured by what their bodies show", () => {
    open([
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [0, 0], view: "diagram" },
      { id: "c", ref: { block: "src/c.ts#function:c" }, at: [900, 0], view: "diagram" },
    ]);
    const arrows = [
      { id: "1", source: "b", sourceHandle: "node", target: "c", project: "p", occurrence: "node", seq: 0, kind: "calls" as const, dashed: false, order: 0, line: null },
    ];
    arrangeBoard(PAGE, arrows, new Map([["b", "node" as const], ["c", "node" as const]]), () => undefined);
    const [b, c] = cards();
    // A Diagram node's column is a node's width, not a Code card's.
    expect((c?.at[0] ?? 0) - (b?.at[0] ?? 0)).toBeLessThan(fallbackSize(false).width);
  });
});
