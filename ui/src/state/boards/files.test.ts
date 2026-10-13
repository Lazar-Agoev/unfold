import { beforeEach, describe, expect, it, vi } from "vitest";
import type { HealthItem, Page } from "../../ipc/generated";
import { boardDocument, boardState, page } from "../../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("./engine", () => engine);
vi.mock("../../components/Toast", () => ({ showToast: vi.fn() }));

const { copyPageMermaid, deleteBoard, exportPagePng, isValidName, publishBoard, publishedPath, slugOf, unpublishBoard } = await import("./files");
const { itemsFor, repairEffect, repairSpec } = await import("./health");
const store = await import("./store");
const { getShownBoard, showBoard, useShownBoardStore } = await import("../shownBoard");

const P = "p-orders0000";
const KEY = "src/routes/orders.ts#function:createOrder";
const PAGE: Page = page("page-2", [{ id: "create-order", ref: { block: KEY, project: "payments" }, at: [0, 0] }], {
  groups: [{ id: "queue", card: "create-order", title: "Queue it", members: [`${KEY}/if[0]`], collapsed: false }],
  links: [{ id: "a-to-b", from: "create-order", to: "create-order-2" }],
});

describe("board file names", () => {
  it("slugs titles the way the engine does", () => {
    expect(slugOf("Order placement")).toBe("order-placement");
    expect(slugOf("Café Crème")).toBe("cafe-creme");
    expect(slugOf("  POST /orders  ")).toBe("post-orders");
    expect(slugOf("***")).toBe("board");
    expect(publishedPath("order-placement")).toBe(".unfold/boards/order-placement.json");
  });

  it("accepts only plain stems", () => {
    expect(isValidName("orders-2")).toBe(true);
    expect(isValidName("v1.2_final")).toBe(true);
    expect(isValidName("-orders")).toBe(false);
    expect(isValidName("../orders")).toBe(false);
    expect(isValidName("bad name")).toBe(false);
  });
});

describe("repairs", () => {
  const missing: HealthItem = { page: "page-2", kind: "missing", target: { card: "create-order" }, message: "not in the graph", repairs: ["reconnect", "removeCard"] };
  const member: HealthItem = {
    page: "page-2",
    kind: "review",
    target: { group: "queue" },
    project: "payments",
    key: `${KEY}/if[0]`,
    message: "the code changed",
    repairs: ["accept", "reconnect", "ungroup"],
  };
  const link: HealthItem = { page: "page-2", kind: "brokenLink", target: { link: "a-to-b" }, message: "an end is missing", repairs: ["removeLink"] };

  it("finds the items about a target on its own page", () => {
    const health = { items: [missing, { ...missing, page: "page-1" }], missing: 2, outdated: 0, brokenGroups: 0, review: 0, notConnected: 0, brokenLinks: 0 };
    expect(itemsFor(health, "page-2", { card: "create-order" })).toEqual([missing]);
    expect(itemsFor(health, "page-2", { group: "create-order" })).toEqual([]);
  });

  it("builds the request for each repair, on the item's page and in its project", () => {
    expect(repairSpec("removeCard", missing, PAGE)).toEqual({ page: "page-2", kind: "remove_card", card: "create-order" });
    expect(repairSpec("reconnect", missing, PAGE)).toBeNull();
    expect(repairSpec("reconnect", missing, PAGE, "src/routes/orders.ts#function:placeOrder")).toEqual({
      page: "page-2",
      kind: "reconnect",
      project: "payments",
      from: KEY,
      to: "src/routes/orders.ts#function:placeOrder",
    });
    expect(repairSpec("accept", member, PAGE)).toEqual({ page: "page-2", kind: "accept", project: "payments", key: `${KEY}/if[0]` });
    expect(repairSpec("ungroup", member, PAGE)).toEqual({ page: "page-2", kind: "ungroup", group: "queue" });
    expect(repairSpec("removeLink", link, PAGE)).toEqual({ page: "page-2", kind: "remove_link", link: "a-to-b" });
    expect(repairSpec("resign", { ...missing, kind: "outdated" }, PAGE)).toEqual({ page: "page-2", kind: "resign", target: { card: "create-order" } });
    const label: HealthItem = { page: "page-2", kind: "missing", target: { label: { key: `${KEY}/if[0]` } }, message: "", repairs: [] };
    expect(repairSpec("removeLabel", label, PAGE)).toEqual({ page: "page-2", kind: "remove_label", key: `${KEY}/if[0]` });
  });

  it("says what each repair does, naming what it is about", () => {
    expect(repairEffect("removeCard", missing, PAGE)).toBe("Removes the card createOrder from the page, with the groups inside it and its links.");
    expect(repairEffect("ungroup", member, PAGE)).toContain('Removes the group "Queue it"');
    expect(repairEffect("removeLink", link, PAGE)).toBe("Removes the link. What it joins stays on the page.");
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
      repairs: ["reconnect", "removeResource"],
    };
    expect(itemsFor({ items: [anchor], missing: 1, outdated: 0, brokenGroups: 0, review: 0, notConnected: 0, brokenLinks: 0 }, "page-2", { resource: "orders" })).toEqual([anchor]);
    expect(repairSpec("reconnect", anchor, withTable, `${KEY}/if[1]`)).toEqual({ page: "page-2", kind: "reconnect", from: `${KEY}/if[0]`, to: `${KEY}/if[1]` });
    expect(repairSpec("removeResource", anchor, withTable)).toEqual({ page: "page-2", kind: "remove_resource", resource: "orders" });
    expect(repairSpec("resign", { ...anchor, kind: "outdated" }, withTable)).toEqual({ page: "page-2", kind: "resign", target: { resource: "orders" } });
    expect(repairEffect("removeResource", anchor, withTable)).toBe('Removes the table "orders" from the page, with the links to it.');
    expect(repairEffect("removeDescription", anchor, withTable)).toBe('Removes the description of the table "orders". The resource card stays.');
  });
});

describe("file operations on an open board", () => {
  const draft = ".unfold/local/drafts/orders.json";
  const published = ".unfold/boards/orders.json";
  const ref = { project: P, id: draft };

  beforeEach(() => {
    engine.call.mockReset();
    engine.windowCommand.mockClear();
    store.resetBoards();
    useShownBoardStore.setState({ board: null });
    store.adoptBoardState(boardState(P, draft, boardDocument([page("page-1"), page("page-2")])));
    showBoard(ref);
  });

  it("publishing a draft carries the session over to the published id, and keeps it in front", async () => {
    store.setActivePage(ref, "page-2");
    store.editBoard(ref, (document) => {
      document.title = "Orders";
    });
    engine.call.mockImplementation((command: string) =>
      Promise.resolve(
        command === "publish_board" ? { status: "done", board: boardState(P, published, boardDocument([page("page-1"), page("page-2")]), { place: "published" }) } : boardState(P, draft),
      ),
    );
    const outcome = await publishBoard(ref, { name: "orders" });
    expect(outcome.status).toBe("done");
    expect(engine.call).toHaveBeenCalledWith("publish_board", { project: P, id: draft, name: "orders" });
    const next = { project: P, id: published };
    expect(store.getOpenBoard(ref)).toBeUndefined();
    expect(store.getOpenBoard(next)?.activePage).toBe("page-2");
    expect(store.canUndo(next)).toBe(true);
    expect(getShownBoard()).toEqual(next);
  });

  it("leaves the board as it is when the name is taken", async () => {
    engine.call.mockResolvedValue({ status: "name_taken", path: published, suggestion: "orders-2" });
    await expect(publishBoard(ref, { name: "orders" })).resolves.toMatchObject({ status: "name_taken", suggestion: "orders-2" });
    expect(store.getOpenBoard(ref)).toBeDefined();
  });

  it("moving back to drafts follows the new id; deleting forgets the board and shows Home", async () => {
    engine.call.mockResolvedValueOnce(boardState(P, ".unfold/local/drafts/orders-2.json"));
    const moved = await unpublishBoard(ref);
    expect(getShownBoard()).toEqual(moved);
    engine.call.mockResolvedValueOnce(null);
    await deleteBoard(moved);
    expect(engine.call).toHaveBeenLastCalledWith("delete_board", { project: P, id: moved.id });
    expect(store.getOpenBoard(moved)).toBeUndefined();
    expect(getShownBoard()).toBeNull();
  });

  it("copies a page's saved version as Mermaid", async () => {
    engine.call.mockResolvedValueOnce({ text: "flowchart LR" });
    await copyPageMermaid({ board: ref, page: "page-2" });
    expect(engine.call).toHaveBeenCalledWith("export_mermaid_text", { project: P, board: { id: draft, page: "page-2" } });
    expect(engine.windowCommand).toHaveBeenCalledWith("copy_to_clipboard", { text: "flowchart LR" });
  });

  it("exports a page as PNG, named after the board and the page", async () => {
    await exportPagePng({ board: ref, page: "page-2" });
    expect(engine.windowCommand).toHaveBeenCalledWith("export_png", { project: P, board: draft, page: "page-2", title: "Order placement - Page page-2" });
  });
});
