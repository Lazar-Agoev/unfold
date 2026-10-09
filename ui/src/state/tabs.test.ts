import { beforeEach, describe, expect, it, vi } from "vitest";
import { boardDocument, boardState, page } from "../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(() => Promise.resolve(null)),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("./boards/engine", () => engine);
vi.mock("../ipc/unfold", async (original) => ({ ...(await original<Record<string, unknown>>()), windowCommand: engine.windowCommand }));
vi.mock("../components/Toast", () => ({ showToast: vi.fn() }));

const tabs = await import("./tabs");
const store = await import("./boards/store");
const { closeActiveTab } = await import("./actions");

const P = "p-orders0000";
const a = { project: P, id: ".unfold/boards/a.json" };
const b = { project: P, id: ".unfold/boards/b.json" };
const c = { project: "p-payments00", id: ".unfold/boards/a.json" };
const key = (ref: { project: string; id: string }) => `${ref.project}\u0000${ref.id}`;

beforeEach(() => {
  engine.call.mockClear();
  engine.windowCommand.mockClear();
  store.resetBoards();
  tabs.useTabs.setState({ boards: [], active: "home" });
});

describe("tabs", () => {
  it("start on Home, which is always first and never closes", async () => {
    expect(tabs.useTabs.getState()).toEqual({ boards: [], active: "home" });
    await tabs.closeTab("home");
    expect(tabs.useTabs.getState().active).toBe("home");
    expect(tabs.getActiveBoard()).toBeNull();
  });

  it("open boards in order, keep one tab per board, and tell boards of two projects apart", () => {
    tabs.openBoardTab(a);
    tabs.openBoardTab(c, { activate: false });
    tabs.openBoardTab(b, { index: 1 });
    tabs.openBoardTab(a, { activate: false });
    expect(tabs.useTabs.getState().boards).toEqual([a, b, c]);
    expect(tabs.getActiveBoard()).toEqual(b);
  });

  it("cycle through Home and the board tabs, and reorder board tabs", () => {
    tabs.openBoardTab(a);
    tabs.openBoardTab(b);
    tabs.nextTab();
    expect(tabs.useTabs.getState().active).toBe("home");
    tabs.nextTab();
    expect(tabs.getActiveBoard()).toEqual(a);
    tabs.previousTab();
    tabs.previousTab();
    expect(tabs.getActiveBoard()).toEqual(b);
    tabs.moveTab(key(b), 0);
    expect(tabs.useTabs.getState().boards).toEqual([b, a]);
  });

  it("follow a board whose id changed (Publish, Move back to drafts), keeping place and activity", () => {
    tabs.openBoardTab(a);
    tabs.openBoardTab(b, { activate: false });
    const published = { project: P, id: ".unfold/boards/a-published.json" };
    tabs.renameBoardTab(a, published);
    expect(tabs.useTabs.getState()).toEqual({ boards: [published, b], active: key(published) });
  });

  it("close a board tab by saving and closing its board; the neighbour becomes active", async () => {
    for (const ref of [a, b, c]) {
      store.adoptBoardState(boardState(ref.project, ref.id));
      tabs.openBoardTab(ref);
    }
    tabs.activateBoard(b);
    await tabs.closeTab(key(b));
    expect(engine.call).toHaveBeenCalledWith("close_board", { project: P, id: b.id });
    expect(tabs.useTabs.getState()).toEqual({ boards: [a, c], active: key(c) });
    await tabs.closeOtherTabs(key(c));
    expect(tabs.useTabs.getState()).toEqual({ boards: [c], active: key(c) });
    expect(store.getOpenBoard(a)).toBeUndefined();
  });

  it("snapshot board tabs with their active page and the active index for the window state", () => {
    store.adoptBoardState(boardState(a.project, a.id));
    tabs.openBoardTab(a, { activate: false });
    tabs.openBoardTab(b);
    expect(tabs.tabsSnapshot()).toEqual({
      tabs: [
        { project: P, board: a.id, page: "page-1" },
        { project: P, board: b.id, page: null },
      ],
      activeTab: 1,
    });
    tabs.activateHome();
    expect(tabs.tabsSnapshot().activeTab).toBeNull();
  });

  it("tell the window state when tabs or an open board's active page change, and only then", () => {
    store.adoptBoardState(boardState(a.project, a.id, boardDocument([page("page-1"), page("page-2")])));
    const changed = vi.fn();
    const stop = tabs.subscribeToTabs(changed);
    tabs.openBoardTab(a);
    expect(changed).toHaveBeenCalledTimes(1);
    store.setCamera({ board: a, page: "page-1" }, { x: 1, y: 1, zoom: 1 });
    expect(changed).toHaveBeenCalledTimes(1);
    store.setActivePage(a, "page-2");
    expect(changed).toHaveBeenLastCalledWith({ tabs: [{ project: P, board: a.id, page: "page-2" }], activeTab: 0 });
    stop();
  });

  it("⌘W on Home closes the window only when Home is the only tab (V-44)", async () => {
    tabs.openBoardTab(a, { activate: false });
    await closeActiveTab();
    expect(engine.windowCommand).not.toHaveBeenCalled();
    tabs.useTabs.setState({ boards: [], active: "home" });
    await closeActiveTab();
    expect(engine.windowCommand).toHaveBeenCalledWith("close_window");
  });
});
