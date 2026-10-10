import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineRequestError } from "./engine-host";
import type { FakeBrowserWindow } from "./fake-electron";
import { resetFakeElectron } from "./fake-electron";
import type { WindowEntry } from "./windows";
import { WindowRegistry } from "./windows";
import { Workspace } from "./workspace";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

const PROJECT = "p-7k2m9q4xab";

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
  readonly calls: string[] = [];
  readonly answers = new Map<string, (args: Readonly<Record<string, unknown>>) => Promise<unknown>>();

  request(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    this.calls.push(`${command} ${windowId} ${JSON.stringify(args)}`);
    return this.answers.get(command)?.(args) ?? Promise.resolve(null);
  }
  /** The commands requested, by name and window. */
  names(): string[] {
    return this.calls.map((call) => call.split(" ").slice(0, 2).join(" "));
  }
}

type Entry = WindowEntry & { window: FakeBrowserWindow };

function setup() {
  const engine = new FakeEngine();
  const registry = new WindowRegistry(
    { kind: "dev", url: "http://localhost:5173/" },
    {
      onLoaded: () => undefined,
      onClosed: (entry) => {
        void workspace.windowClosed(entry);
      },
      onPlacementChanged: (entry) => {
        workspace.savePlacement(entry);
      },
    },
    false,
  );
  const workspace = new Workspace(registry, engine);
  const windows = () => registry.all() as Entry[];
  return { engine, registry, workspace, windows };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  resetFakeElectron();
  vi.spyOn(console, "warn").mockImplementation(() => undefined);
  vi.spyOn(console, "error").mockImplementation(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("restore", () => {
  it("brings back the saved windows with their keys and bounds, onto a visible screen", async () => {
    const { engine, workspace, windows } = setup();
    engine.answers.set("saved_windows", () =>
      Promise.resolve({
        windows: [
          { key: "a", bounds: { x: 80, y: 60, width: 1440, height: 900 }, maximized: false, tabs: [] },
          { key: "b", bounds: { x: 9000, y: 9000, width: 800, height: 600 }, maximized: true, tabs: [] },
          { key: "", bounds: null },
        ],
      }),
    );
    await workspace.restore();

    expect(windows().map((entry) => entry.key)).toEqual(["a", "b"]);
    expect(windows().map((entry) => entry.window.bounds)).toEqual([
      { x: 80, y: 60, width: 1440, height: 900 },
      { x: 560, y: 240, width: 800, height: 600 },
    ]);
    windows()[1]?.window.emit("ready-to-show");
    expect(windows()[1]?.window.maximized).toBe(true);
    expect(engine.names().filter((name) => !name.startsWith("save_window_bounds"))).toEqual(["saved_windows 0"]);
  });

  it("brings back at most 16 windows", async () => {
    const { engine, workspace, windows } = setup();
    const saved = Array.from({ length: 20 }, (_, i) => ({ key: `w${i}`, bounds: null, maximized: false }));
    engine.answers.set("saved_windows", () => Promise.resolve({ windows: saved }));
    await workspace.restore();
    expect(windows()).toHaveLength(16);
  });

  it("opens one new window when none were saved or the engine can't say", async () => {
    for (const answer of [
      () => Promise.resolve({ windows: [] }),
      () => Promise.reject(new EngineRequestError({ code: "engine_unavailable", message: "down" })),
    ]) {
      const { engine, workspace, windows } = setup();
      engine.answers.set("saved_windows", answer);
      await workspace.restore();
      expect(windows()).toHaveLength(1);
    }
  });

  it("restores once when asked again while restoring, and skips windows already open", async () => {
    const { engine, registry, workspace, windows } = setup();
    const answer = deferred<unknown>();
    engine.answers.set("saved_windows", () => answer.promise);
    const first = workspace.restore();
    const second = workspace.restore();
    answer.resolve({ windows: [{ key: "a", bounds: null, maximized: false }] });
    await Promise.all([first, second]);
    expect(windows().map((entry) => entry.key)).toEqual(["a"]);

    engine.answers.set("saved_windows", () =>
      Promise.resolve({ windows: [{ key: "a", bounds: null, maximized: false }, { key: "b", bounds: null }] }),
    );
    await workspace.restore();
    expect(registry.all().map((entry) => entry.key)).toEqual(["a", "b"]);
  });
});

describe("addProject", () => {
  it("adds the folder with the trusted add_project and tells the focused window to select it", async () => {
    const { engine, registry, workspace } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: { id: PROJECT } }));
    const [a, b] = [registry.create() as Entry, registry.create() as Entry];
    a.window.finishLoad();
    b.window.finishLoad();
    b.window.focus();

    await workspace.addProject("/code/shop");
    expect(engine.names()).toContain("add_project 0");
    expect(engine.calls.find((call) => call.startsWith("add_project"))).toBe('add_project 0 {"path":"/code/shop"}');
    expect(a.window.sent).toEqual([]);
    expect(b.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("tells the first window when none is focused, and an existing project is selected too", async () => {
    const { engine, registry, workspace } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "exists", project: { id: PROJECT } }));
    const first = registry.create() as Entry;
    registry.create();
    first.window.finishLoad();
    await workspace.addProject("/code/shop");
    expect(first.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("opens a window when there is none, and tells it once its UI has loaded", async () => {
    const { engine, workspace, windows } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: { id: PROJECT } }));
    await workspace.addProject("/code/shop");
    const [created] = windows();
    expect(created?.window.sent).toEqual([]);
    created?.window.finishLoad();
    expect(created?.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("at launch, waits for the restored windows instead of opening one", async () => {
    const { engine, workspace, windows } = setup();
    const saved = deferred<unknown>();
    engine.answers.set("saved_windows", () => saved.promise);
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: { id: PROJECT } }));
    const restored = workspace.restore();
    const added = workspace.addProject("/code/shop");
    await flush();
    expect(windows()).toEqual([]);

    saved.resolve({ windows: [{ key: "a", bounds: null, maximized: false }] });
    await Promise.all([restored, added]);
    expect(windows().map((entry) => entry.key)).toEqual(["a"]);
    windows()[0]?.window.finishLoad();
    expect(windows()[0]?.window.sent).toEqual([{ type: "project_added", project: PROJECT }]);
  });

  it("tells the focused window why the engine refused a folder, and logs it", async () => {
    const { engine, registry, workspace } = setup();
    engine.answers.set("add_project", () =>
      Promise.reject(new EngineRequestError({ code: "not_a_folder", message: "Not a folder: /code/gone." })),
    );
    const [a, b] = [registry.create() as Entry, registry.create() as Entry];
    a.window.finishLoad();
    b.window.finishLoad();
    b.window.focus();
    await workspace.addProject("/code/gone");
    expect(a.window.sent).toEqual([]);
    expect(b.window.sent).toEqual([
      { type: "project_add_failed", path: "/code/gone", message: "Not a folder: /code/gone." },
    ]);
    expect(console.error).toHaveBeenCalledWith(
      "[main] adding /code/gone to the workspace failed: not_a_folder: Not a folder: /code/gone.",
    );
  });

  it("at launch, tells a restored window about a folder it couldn't add once its UI has loaded", async () => {
    const { engine, workspace, windows } = setup();
    engine.answers.set("saved_windows", () => Promise.resolve({ windows: [{ key: "a", bounds: null, maximized: false }] }));
    engine.answers.set("add_project", () =>
      Promise.reject(new EngineRequestError({ code: "too_many_projects", message: "The workspace is full." })),
    );
    await Promise.all([workspace.restore(), workspace.addProject("/code/shop")]);
    expect(windows().map((entry) => entry.key)).toEqual(["a"]);
    windows()[0]?.window.finishLoad();
    expect(windows()[0]?.window.sent).toEqual([
      { type: "project_add_failed", path: "/code/shop", message: "The workspace is full." },
    ]);
  });
});

describe("remembering windows", () => {
  it("saves a window's bounds as soon as it is created", () => {
    const { engine, registry } = setup();
    const entry = registry.create({ key: "a", bounds: { x: 10, y: 20, width: 900, height: 700 }, maximized: false });
    expect(engine.calls).toEqual([
      `save_window_bounds ${entry.id} ${JSON.stringify({ key: "a", bounds: { x: 10, y: 20, width: 900, height: 700 }, maximized: false })}`,
    ]);
  });

  it("forgets a closed window unless it was the last one", async () => {
    const { engine, registry } = setup();
    const [a, b] = [registry.create() as Entry, registry.create() as Entry];
    engine.calls.length = 0;
    a.window.close();
    await flush();
    expect(engine.calls).toEqual([`forget_window ${a.id} ${JSON.stringify({ key: a.key })}`]);

    b.window.close();
    await flush();
    expect(engine.names()).toEqual([`forget_window ${a.id}`]);
  });

  it("remembers every window closing while the app quits", async () => {
    const { engine, registry, workspace } = setup();
    const [a] = [registry.create() as Entry, registry.create()];
    engine.calls.length = 0;
    workspace.setQuitting();
    a.window.close();
    await flush();
    expect(engine.calls).toEqual([]);
  });

  it("forgets a window only after its last bounds save settled", async () => {
    const { engine, registry } = setup();
    const [a] = [registry.create() as Entry, registry.create()];
    const saved = deferred<unknown>();
    engine.answers.set("save_window_bounds", () => saved.promise);
    a.window.emit("move");
    engine.calls.length = 0;
    a.window.close(); // Flushes the debounced move.
    await flush();
    expect(engine.names()).toEqual([`save_window_bounds ${a.id}`]);

    saved.resolve(null);
    await flush();
    expect(engine.names()).toEqual([`save_window_bounds ${a.id}`, `forget_window ${a.id}`]);
  });
});
