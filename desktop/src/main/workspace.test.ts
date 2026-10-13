import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineRequestError } from "./engine-host";
import type { FakeBrowserWindow } from "./fake-electron";
import { createdWindows, resetFakeElectron } from "./fake-electron";
import type { WindowEntry } from "./windows";
import { AppWindow } from "./windows";
import { Workspace } from "./workspace";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

const PROJECT = "p-7k2m9q4xab";
const BOARD = { project: PROJECT, board: ".unfold/boards/orders.json" };
const SAVED = { bounds: { x: 80, y: 60, width: 1440, height: 900 }, maximized: false, board: BOARD };

interface Deferred<T> {
  promise: Promise<T>;
  resolve(value: T): void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** Records the main process's trusted engine requests and answers them per command. */
class FakeEngine {
  readonly calls: [number, string, Readonly<Record<string, unknown>>][] = [];
  readonly answers = new Map<string, (args: Readonly<Record<string, unknown>>) => Promise<unknown>>();

  request(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    this.calls.push([windowId, command, args]);
    return this.answers.get(command)?.(args) ?? Promise.resolve(null);
  }
  /** The arguments of every call of `command`, in order. */
  callsOf(command: string): Readonly<Record<string, unknown>>[] {
    return this.calls.filter(([, name]) => name === command).map(([, , args]) => args);
  }
}

type Entry = WindowEntry & { window: FakeBrowserWindow };

/** The window and the workspace, wired as the app wires them. */
function setup() {
  const engine = new FakeEngine();
  const detached: number[] = [];
  const appWindow = new AppWindow(
    { kind: "dev", url: "http://localhost:5173/" },
    {
      onLoaded: () => undefined,
      onPlacementChanged: () => {
        workspace.placementChanged();
      },
      onClosing: () => {
        workspace.windowClosing();
      },
      onClosed: (entry) => {
        void workspace.windowClosed().then(() => {
          detached.push(entry.id);
        });
      },
    },
    false,
  );
  const workspace = new Workspace(appWindow, engine);
  const current = () => appWindow.current() as Entry | undefined;
  const opened = (): Entry => {
    const entry = current();
    if (!entry) {
      throw new Error("the window isn't open");
    }
    return entry;
  };
  return { engine, appWindow, workspace, current, opened, detached };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  resetFakeElectron();
  vi.spyOn(console, "warn").mockImplementation(() => undefined);
  vi.spyOn(console, "error").mockImplementation(() => undefined);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("restore", () => {
  it("opens the window where it was, maximized if it was, and hands the UI the last board shown", async () => {
    const { engine, workspace, current } = setup();
    engine.answers.set("window_state", () => Promise.resolve({ ...SAVED, maximized: true }));
    await workspace.restore();

    expect(engine.calls).toEqual([[0, "window_state", {}]]);
    expect(createdWindows).toHaveLength(1);
    expect(current()?.window.bounds).toEqual(SAVED.bounds);
    current()?.window.emit("ready-to-show");
    expect(current()?.window.maximized).toBe(true);
    expect(workspace.shownBoard()).toEqual(BOARD);
  });

  it("moves the window onto a visible screen when its display is gone", async () => {
    const { engine, workspace, current } = setup();
    engine.answers.set("window_state", () => Promise.resolve({ ...SAVED, bounds: { x: 9000, y: 9000, width: 800, height: 600 } }));
    await workspace.restore();
    expect(current()?.window.bounds).toEqual({ x: 560, y: 240, width: 800, height: 600 });
  });

  it("opens the window on Home at its default size when nothing was saved or the engine can't say", async () => {
    for (const answer of [
      () => Promise.resolve(null),
      () => Promise.reject(new EngineRequestError({ code: "engine_unavailable", message: "down" })),
    ]) {
      resetFakeElectron();
      const { engine, workspace, current } = setup();
      engine.answers.set("window_state", answer);
      await workspace.restore();
      expect(createdWindows).toHaveLength(1);
      expect(current()?.window.bounds).toEqual({ x: 0, y: 0, width: 1280, height: 820 });
      expect(workspace.shownBoard()).toBeNull();
    }
  });

  it("restores once, and a window an agent needs meanwhile opens where it was, on the board it showed", async () => {
    vi.useFakeTimers();
    const { engine, workspace, current } = setup();
    const answer = deferred<unknown>();
    engine.answers.set("window_state", () => answer.promise);
    const first = workspace.restore();
    const second = workspace.restore();
    const opening = workspace.open();
    await vi.advanceTimersByTimeAsync(0);
    expect(createdWindows).toEqual([]);

    answer.resolve(SAVED);
    const opened = (await opening) as Entry;
    await Promise.all([first, second]);
    expect(engine.callsOf("window_state")).toHaveLength(1);
    expect(current()).toBe(opened);
    expect(createdWindows).toHaveLength(1);
    expect(opened.window.bounds).toEqual(SAVED.bounds);
    expect(workspace.shownBoard()).toEqual(BOARD);

    // The window's first save keeps them.
    opened.window.emit("move");
    await vi.advanceTimersByTimeAsync(500);
    expect(engine.callsOf("save_window_state")).toEqual([{ state: SAVED }]);
  });
});

describe("saving the window's state", () => {
  it("saves the bounds and the board shown once they settle, and nothing else", async () => {
    vi.useFakeTimers();
    const { engine, workspace, current } = setup();
    await workspace.restore();
    const entry = current();
    entry?.window.emit("move");
    workspace.showBoard(BOARD);
    entry?.window.emit("resize");
    await vi.advanceTimersByTimeAsync(400);
    expect(engine.callsOf("save_window_state")).toEqual([]);

    await vi.advanceTimersByTimeAsync(100);
    expect(engine.calls.at(-1)).toEqual([entry?.id, "save_window_state", { state: { bounds: { x: 0, y: 0, width: 1280, height: 820 }, maximized: false, board: BOARD } }]);

    workspace.showBoard(null);
    await vi.advanceTimersByTimeAsync(500);
    expect(engine.callsOf("save_window_state").at(-1)).toMatchObject({ state: { board: null } });
    expect(workspace.shownBoard()).toBeNull();
  });

  it("saves a change still waiting as the window closes, and detaches it only after that save settled", async () => {
    const { engine, workspace, opened, detached } = setup();
    await workspace.restore();
    const entry = opened();
    const saved = deferred<unknown>();
    engine.answers.set("save_window_state", () => saved.promise);
    entry.window.maximized = true;
    entry.window.emit("maximize");
    workspace.showBoard(BOARD);

    entry.window.close();
    expect(engine.callsOf("save_window_state")).toEqual([{ state: { bounds: { x: 0, y: 0, width: 1280, height: 820 }, maximized: true, board: BOARD } }]);
    await flush();
    expect(detached).toEqual([]);

    saved.resolve(null);
    await flush();
    expect(detached).toEqual([entry.id]);
  });

  it("saves nothing as the window closes when nothing changed, and nothing while it is closed", async () => {
    vi.useFakeTimers();
    const { engine, workspace, opened, detached } = setup();
    await workspace.restore();
    const entry = opened();
    entry.window.close();
    await vi.advanceTimersByTimeAsync(0);
    expect(detached).toEqual([entry.id]);

    workspace.showBoard(BOARD);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(engine.callsOf("save_window_state")).toEqual([]);
  });
});

describe("showing the window again", () => {
  it("reopens a closed window where it was, on the board it showed, with a new id", async () => {
    const { engine, workspace, opened } = setup();
    engine.answers.set("window_state", () => Promise.resolve(SAVED));
    await workspace.restore();
    const first = opened();
    first.window.bounds.x = 200;
    first.window.close();

    await workspace.show();
    const second = opened();
    expect(second).not.toBe(first);
    expect(second.id).toBeGreaterThan(first.id);
    expect(second.window.bounds).toEqual({ ...SAVED.bounds, x: 200 });
    expect(second.window.shows).toBe(1);
    expect(workspace.shownBoard()).toEqual(BOARD);
    expect(engine.callsOf("window_state")).toHaveLength(1);
  });

  it("brings the open window to the front, after the launch restore", async () => {
    const { engine, workspace, current } = setup();
    const answer = deferred<unknown>();
    engine.answers.set("window_state", () => answer.promise);
    void workspace.restore();
    const shown = workspace.show();
    await flush();
    expect(createdWindows).toEqual([]);

    answer.resolve(SAVED);
    await shown;
    expect(createdWindows).toHaveLength(1);
    expect(current()?.window.bounds).toEqual(SAVED.bounds);
    expect(current()?.window.shows).toBe(1);
  });
});

describe("addProject", () => {
  it("adds the folder with the trusted add_project and tells the window to select it once its UI has loaded", async () => {
    const { engine, workspace, current } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: { id: PROJECT } }));
    await workspace.restore();
    await workspace.addProject("/code/shop");

    expect(engine.calls.at(-1)).toEqual([0, "add_project", { path: "/code/shop" }]);
    expect(current()?.window.sent).toEqual([]);
    current()?.window.finishLoad();
    expect(current()?.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("opens the window when it is closed, and an existing project is selected too", async () => {
    const { engine, workspace, current } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "exists", project: { id: PROJECT } }));
    await workspace.restore();
    current()?.window.close();
    await workspace.addProject("/code/shop");
    current()?.window.finishLoad();
    expect(createdWindows).toHaveLength(2);
    expect(current()?.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("tells the window why the engine refused a folder, and logs it", async () => {
    const { engine, workspace, current } = setup();
    engine.answers.set("add_project", () =>
      Promise.reject(new EngineRequestError({ code: "too_many_projects", message: "The workspace is full." })),
    );
    await workspace.restore();
    current()?.window.finishLoad();
    await workspace.addProject("/code/shop");
    expect(current()?.window.sent).toEqual([{ type: "project_add_failed", path: "/code/shop", message: "The workspace is full." }]);
    expect(console.error).toHaveBeenCalledWith(
      "[main] adding /code/shop to the workspace failed: too_many_projects: The workspace is full.",
    );
  });
});
