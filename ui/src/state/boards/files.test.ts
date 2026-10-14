import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HealthItem, Page } from "../../ipc/generated";
import { boardDocument, boardState, page } from "../../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("./engine", () => engine);

const { deleteBoard, exportPagePng } = await import("./files");
const { itemsFor, repairEffect, repairSpec } = await import("./health");
const store = await import("./store");
const { getShownBoard, showBoard, useShownBoardStore } = await import("../shownBoard");

const P = "p-orders0000";
const KEY = "src/routes/orders.ts#function:createOrder";
const PAGE: Page = page("page-2", [{ id: "create-order", ref: { block: KEY }, at: [0, 0] }], {
  combined: [{ id: "queue", title: "Queue it", members: [`${KEY}/if[0]`], collapsed: false, author: "agent:claude", signature: "" }],
  names: [{ key: `${KEY}/if[0]`, text: "Is it paid?", author: "person:lazar", signature: "" }],
  links: [{ id: "a-to-b", from: "create-order", to: "create-order-2", kind: "calls", author: "person:lazar" }],
});

const HEALTH = { missing: 0, outdated: 0, review: 0, brokenCombined: 0, brokenLinks: 0 };

describe("repairs", () => {
  const missing: HealthItem = { page: "page-2", kind: "missing", target: { card: "create-order" }, message: "not in the graph", repairs: ["reconnect", "remove"] };
  const member: HealthItem = {
    page: "page-2",
    kind: "review",
    target: { combined: "queue" },
    key: `${KEY}/if[0]`,
    message: "the code changed",
    repairs: ["accept", "reconnect", "remove"],
  };
  const link: HealthItem = { page: "page-2", kind: "brokenLink", target: { connection: "a-to-b" }, message: "an end is missing", repairs: ["remove"] };

  it("finds the items about a target on its own page", () => {
    const health = { ...HEALTH, items: [missing, { ...missing, page: "page-1" }], missing: 2 };
    expect(itemsFor(health, "page-2", { card: "create-order" })).toEqual([missing]);
    expect(itemsFor(health, "page-2", { combined: "create-order" })).toEqual([]);
  });

  it("builds the request for each repair, on the item's page", () => {
    expect(repairSpec("remove", missing, PAGE)).toEqual({ page: "page-2", kind: "remove", target: { card: "create-order" } });
    expect(repairSpec("reconnect", missing, PAGE)).toBeNull();
    expect(repairSpec("reconnect", missing, PAGE, "src/routes/orders.ts#function:placeOrder")).toEqual({
      page: "page-2",
      kind: "reconnect",
      from: KEY,
      to: "src/routes/orders.ts#function:placeOrder",
    });
    expect(repairSpec("accept", member, PAGE)).toEqual({ page: "page-2", kind: "accept", key: `${KEY}/if[0]` });
    expect(repairSpec("remove", member, PAGE)).toEqual({ page: "page-2", kind: "remove", target: { combined: "queue" } });
    expect(repairSpec("remove", link, PAGE)).toEqual({ page: "page-2", kind: "remove", target: { connection: "a-to-b" } });
    expect(repairSpec("resign", { ...missing, kind: "outdated" }, PAGE)).toEqual({ page: "page-2", kind: "resign", target: { card: "create-order" } });
    const name: HealthItem = { page: "page-2", kind: "missing", target: { name: `${KEY}/if[0]` }, message: "", repairs: [] };
    expect(repairSpec("accept", name, PAGE)).toEqual({ page: "page-2", kind: "accept", key: `${KEY}/if[0]` });
  });

  it("says what each repair does, naming what it is about in the page's words", () => {
    expect(repairEffect("remove", missing, PAGE)).toBe("Removes the card from the page, with its connections.");
    expect(repairEffect("remove", member, PAGE)).toBe('Removes the combined step "Queue it". Its blocks are drawn one by one again.');
    expect(repairEffect("remove", link, PAGE)).toBe("Removes the calls connection. What it joins stays on the page.");
    const name: HealthItem = { page: "page-2", kind: "outdated", target: { name: `${KEY}/if[0]` }, message: "", repairs: ["resign"] };
    expect(repairEffect("resign", name, PAGE)).toBe('Keeps the text of the name "Is it paid?" as written and signs it against the current code, so it is no longer outdated.');
  });

  it("repairs a resource card's anchor by key, and removes or re-signs the card", () => {
    const withTable: Page = {
      ...PAGE,
      resources: [{ id: "orders", kind: "table", name: "orders", anchors: [{ key: `${KEY}/if[0]` }], at: [0, 0], author: "agent:claude" }],
    };
    const anchor: HealthItem = {
      page: "page-2",
      kind: "missing",
      target: { resource: "orders" },
      key: `${KEY}/if[0]`,
      anchor: 0,
      message: "Anchor is not in the graph",
      repairs: ["reconnect", "remove"],
    };
    expect(itemsFor({ ...HEALTH, items: [anchor], missing: 1 }, "page-2", { resource: "orders" })).toEqual([anchor]);
    expect(repairSpec("reconnect", anchor, withTable, `${KEY}/if[1]`)).toEqual({ page: "page-2", kind: "reconnect", from: `${KEY}/if[0]`, to: `${KEY}/if[1]` });
    expect(repairSpec("remove", anchor, withTable)).toEqual({ page: "page-2", kind: "remove", target: { resource: "orders" } });
    expect(repairSpec("resign", { ...anchor, kind: "outdated" }, withTable)).toEqual({ page: "page-2", kind: "resign", target: { resource: "orders" } });
    expect(repairEffect("remove", anchor, withTable)).toBe('Removes the table "orders" from the page, with its connections.');
    expect(repairEffect("removeDescription", anchor, withTable)).toBe('Removes the description of the table "orders". The resource card stays.');
  });
});

describe("file operations on an open board", () => {
  const id = ".unfold/boards/orders.json";
  const ref = { project: P, id };

  beforeEach(() => {
    engine.call.mockReset();
    engine.windowCommand.mockClear();
    store.resetBoards();
    useShownBoardStore.setState({ board: null });
    store.adoptBoardState(boardState(P, id, boardDocument([page("page-1"), page("page-2")])));
    showBoard(ref);
  });

  it("deleting forgets the board and shows Home", async () => {
    engine.call.mockResolvedValueOnce(null);
    await deleteBoard(ref);
    expect(engine.call).toHaveBeenLastCalledWith("delete_board", { project: P, id });
    expect(store.getOpenBoard(ref)).toBeUndefined();
    expect(getShownBoard()).toBeNull();
  });

  it("exports a page as PNG, named after the board and the page", async () => {
    await exportPagePng({ board: ref, page: "page-2" });
    expect(engine.windowCommand).toHaveBeenCalledWith("export_png", { project: P, board: id, page: "page-2", title: "Order placement - Page page-2" });
  });
});
