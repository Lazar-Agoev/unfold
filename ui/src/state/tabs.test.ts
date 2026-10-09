import { beforeEach, describe, expect, it } from "vitest";
import { activateTab, applyTabsSnapshot, closeTab, createTabsStore, openBoardTab, openSource, renameBoardTab, setBoardTabCloser, tabsSnapshot, type TabsStore } from "./tabs";

let store: TabsStore;

beforeEach(() => {
  store = createTabsStore();
});

describe("tabs", () => {
  it("opens source tabs beside the active tab and focuses an open one again with a new line", () => {
    openSource("a.ts", null, store);
    openBoardTab(".unfold/boards/x.json", store);
    activateTab("source:a.ts", store);
    openSource("b.ts", 10, store);
    expect(store.getState().tabs.map((tab) => tab.id)).toEqual(["source:a.ts", "source:b.ts", "board:.unfold/boards/x.json"]);
    openSource("b.ts", 20, store);
    expect(store.getState().tabs[1]).toMatchObject({ line: 20, reveal: 1 });
    expect(store.getState().activeId).toBe("source:b.ts");
  });

  it("keeps a board's tab in place when its ID changes, even if the new ID announced itself", () => {
    openBoardTab(".unfold/local/drafts/a.json", store);
    openSource("a.ts", null, store);
    openBoardTab(".unfold/boards/a.json", store);
    renameBoardTab(".unfold/local/drafts/a.json", ".unfold/boards/a.json", store);
    expect(store.getState().tabs.map((tab) => tab.id)).toEqual(["board:.unfold/boards/a.json", "source:a.ts"]);
  });

  it("closes a tab, activating its neighbour, and asks the board store before closing a board", async () => {
    openSource("a.ts", null, store);
    openSource("b.ts", null, store);
    openBoardTab("board.json", store);
    let asked: string | null = null;
    const stop = setBoardTabCloser((id) => {
      asked = id;
      return false;
    });
    await closeTab("board:board.json", store);
    expect(asked).toBe("board.json");
    expect(store.getState().tabs).toHaveLength(3);
    stop();
    await closeTab("board:board.json", store);
    expect(store.getState().activeId).toBe("source:b.ts");
    await closeTab("source:a.ts", store);
    expect(store.getState().tabs.map((tab) => tab.id)).toEqual(["source:b.ts"]);
  });

  it("saves tabs as plain JSON and restores them, dropping malformed and duplicate entries", () => {
    openSource("src/a.ts", 12, store);
    openBoardTab(".unfold/boards/b.json", store);
    const snapshot = tabsSnapshot(store);
    expect(snapshot).toEqual({
      tabs: [
        { kind: "source", path: "src/a.ts", line: 12 },
        { kind: "board", board: ".unfold/boards/b.json" },
      ],
      activeTab: 1,
      openBoards: [".unfold/boards/b.json"],
    });

    const restored = createTabsStore();
    applyTabsSnapshot([...snapshot.tabs, { kind: "source", path: "src/a.ts" }, { kind: "nope" }, 7, null], snapshot.activeTab, restored);
    expect(tabsSnapshot(restored)).toEqual(snapshot);

    applyTabsSnapshot([{ kind: "source", path: "x.ts", line: -3 }], 9, restored);
    expect(restored.getState().tabs).toEqual([{ kind: "source", id: "source:x.ts", path: "x.ts", line: null, reveal: 0 }]);
    expect(restored.getState().activeId).toBe("source:x.ts");
  });
});
