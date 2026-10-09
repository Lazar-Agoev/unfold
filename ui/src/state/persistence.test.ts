// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardState, WindowState } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { useLayout } from "./layout";
import { getLens, setLens } from "./lens";
import { followWindowState, restoreWindow } from "./persistence";
import { openBoardTab, openSource, useTabs } from "./tabs";

const boards = vi.hoisted(() => ({ adopted: [] as string[] }));

vi.mock("./boards/api", async () => {
  const tabs = await import("./tabs");
  return {
    adoptBoardState: (state: BoardState) => {
      boards.adopted.push(state.id);
      // Like the board store: an opened board announces its tab, which activates it.
      tabs.openBoardTab(state.id);
    },
  };
});

const SAVED: WindowState = {
  key: "w1",
  bounds: null,
  maximized: false,
  tabs: [
    { kind: "source", path: "src/a.ts", line: 4 },
    { kind: "board", board: ".unfold/boards/ok.json" },
    { kind: "board", board: ".unfold/boards/held.json" },
  ],
  activeTab: 0,
  layout: { view: "search", sidebarVisible: true, inspectorOpen: true, panelOpen: false, panelTab: "index_log", sizes: { sidebar: 300 }, lens: "flow" },
  openBoards: [".unfold/boards/ok.json", ".unfold/boards/held.json"],
};

let engine: FakeEngine;

beforeEach(() => {
  boards.adopted = [];
  useTabs.setState({ tabs: [], activeId: null });
  setLens("code");
  engine = installFakeEngine({
    handlers: {
      window_state: () => SAVED,
      open_board: ({ id }) =>
        id.includes("held") ? { status: "held", heldBy: 2 } : { status: "opened", board: { id } as unknown as BoardState },
      save_window_state: () => null,
    },
  });
});

afterEach(() => {
  vi.useRealTimers();
  engine.uninstall();
});

describe("restoreWindow", () => {
  it("brings back tabs, layout, lens, and boards; a board held elsewhere loses its tab", async () => {
    await restoreWindow("w1");
    expect(useTabs.getState().tabs.map((tab) => tab.id)).toEqual(["source:src/a.ts", "board:.unfold/boards/ok.json"]);
    expect(useTabs.getState().activeId).toBe("source:src/a.ts");
    expect(boards.adopted).toEqual([".unfold/boards/ok.json"]);
    expect(useLayout.getState()).toMatchObject({ view: "search", inspectorOpen: true, panelOpen: false, panelTab: "index_log" });
    expect(useLayout.getState().sizes.sidebar).toBe(300);
    expect(getLens()).toBe("flow");
  });
});

describe("followWindowState", () => {
  it("saves the window's state once changes settle", () => {
    vi.useFakeTimers();
    const stop = followWindowState("w1", 400);
    openSource("src/b.ts", null);
    openBoardTab(".unfold/boards/c.json");
    setLens("flow");
    vi.advanceTimersByTime(399);
    expect(engine.callsOf("save_window_state")).toEqual([]);
    vi.advanceTimersByTime(1);
    const saves = engine.callsOf("save_window_state");
    expect(saves).toHaveLength(1);
    const state = saves[0]?.["state"] as WindowState;
    expect(state).toMatchObject({
      key: "w1",
      bounds: null,
      tabs: [
        { kind: "source", path: "src/b.ts", line: null },
        { kind: "board", board: ".unfold/boards/c.json" },
      ],
      activeTab: 1,
      openBoards: [".unfold/boards/c.json"],
    });
    expect(state.layout).toMatchObject({ lens: "flow" });
    stop();
  });

  it("sends a save still waiting when the page goes away (the window closes, the app quits)", () => {
    vi.useFakeTimers();
    const stop = followWindowState("w1", 400);
    window.dispatchEvent(new Event("pagehide"));
    expect(engine.callsOf("save_window_state")).toEqual([]);

    openBoardTab(".unfold/local/drafts/board-1.json");
    window.dispatchEvent(new Event("pagehide"));
    const saves = engine.callsOf("save_window_state");
    expect(saves).toHaveLength(1);
    expect(saves[0]?.["state"]).toMatchObject({ key: "w1", openBoards: [".unfold/local/drafts/board-1.json"] });
    // Sent once: the debounced save no longer fires.
    vi.advanceTimersByTime(400);
    expect(engine.callsOf("save_window_state")).toHaveLength(1);

    // After stopping, leaving the page sends nothing.
    openSource("src/b.ts", null);
    stop();
    window.dispatchEvent(new Event("pagehide"));
    vi.advanceTimersByTime(400);
    expect(engine.callsOf("save_window_state")).toHaveLength(1);
  });
});
