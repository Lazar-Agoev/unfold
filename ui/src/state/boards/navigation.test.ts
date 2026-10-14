import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../../ipc/client";
import type { BoardDocument, Card } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("./engine", () => engine);

const store = await import("./store");
const { followCall } = await import("./navigation");
const { addBlock, addRef, focusFrame, nameBlock, removeCard } = await import("./api");
const { addConnection } = await import("./connections");
const { FOLLOW_GAP, HEADER_PORT } = await import("../../canvas/layout");
const { getShownBoard, useShownBoardStore } = await import("../shownBoard");

const P = "p-orders0000";
const Q = "p-payments00";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };

function open(cards: Card[] = [], document?: BoardDocument): void {
  store.adoptBoardState(boardState(P, ID, document ?? boardDocument([page("page-1", cards)])));
}

function cards(): readonly Card[] {
  return store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
}

beforeEach(() => {
  engine.call.mockReset();
  engine.call.mockImplementation(() => Promise.resolve(null));
  store.resetBoards();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders"), projectRow(Q, "payments")]));
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
      targetKey: "src/services/order-service.ts#class:OrderService/method:place",
      name: "OrderService.place",
      lineOffset,
    });
    expect(id).toBe("order-service-place");
    const card = cards().find((item) => item.id === id);
    expect(card?.at).toEqual([100 + 520 + FOLLOW_GAP, 50 + lineOffset - HEADER_PORT]);
    expect(card?.ref).toEqual({ block: "src/services/order-service.ts#class:OrderService/method:place" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: id, key: null });
  });

  it("focuses a target that is already on the page instead of adding it", () => {
    open([
      { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [700, 0] },
    ]);
    const id = followCall({ target: PAGE, fromCardId: "a", targetKey: "src/b.ts#function:b", name: "b" });
    expect(id).toBe("b");
    expect(cards()).toHaveLength(2);
    expect(store.getPageView(PAGE).focus?.cardIds).toEqual(["b"]);
  });
});

describe("adding cards of the board's project", () => {
  it("adds a definition once, named by what it shows, shows its board and page, and focuses it the second time", () => {
    open([], boardDocument([page("page-1"), page("page-2")]));
    const second = { board: REF, page: "page-2" };
    const id = addBlock(second, "key-a", "placeOrder");
    expect(id).toBe("place-order");
    expect(store.getOpenBoard(REF)?.activePage).toBe("page-2");
    expect(getShownBoard()).toEqual(REF);
    expect(addBlock(second, "key-a", "placeOrder")).toBe("place-order");
    expect(store.getOpenBoard(REF)?.document.pages[1]?.cards).toHaveLength(1);
    expect(store.getPageView(second).focus?.cardIds).toEqual(["place-order"]);
  });

  it("adds an app or entrance as a start card", () => {
    open();
    const id = addRef(PAGE, { entrance: "key-place" }, "Place an order");
    expect(cards()).toEqual([{ id: "place-an-order", ref: { entrance: "key-place" }, at: [0, 0] }]);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: id, key: null });
  });

  it("names a block page-wide, by its key, in key order", () => {
    open();
    nameBlock(PAGE, "key-b", "Is it paid?", "person:a");
    nameBlock(PAGE, "key-a", "Load it", "person:a");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.names).toEqual([
      { key: "key-a", text: "Load it", author: "person:a", signature: "" },
      { key: "key-b", text: "Is it paid?", author: "person:a", signature: "" },
    ]);
    nameBlock(PAGE, "key-b", " ", "person:a");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.names.map((name) => name.key)).toEqual(["key-a"]);
  });

  it("removes a card with the connections to it", () => {
    open([
      { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [600, 0] },
    ]);
    store.editPage(PAGE, (draft) => addConnection(draft, { kind: "http", from: "a", to: "b", anchors: [], author: "person:a" }));
    removeCard(PAGE, "b");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.links).toEqual([]);
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
