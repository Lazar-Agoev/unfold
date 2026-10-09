import { beforeEach, describe, expect, it, vi } from "vitest";
import { IPC, PRELOAD_COMMANDS } from "../shared/transport";
import { EngineRequestError } from "./engine-host";
import type { FakeBrowserWindow } from "./fake-electron";
import { clipboardTexts, dialogs, ipcHandlers, resetFakeElectron } from "./fake-electron";
import { registerWindowCommands } from "./ipc";
import type { WindowEntry } from "./windows";
import { WindowRegistry } from "./windows";
import { Workspace } from "./workspace";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

const UI = { kind: "dev", url: "http://localhost:5173/" } as const;
const PROJECT = "p-7k2m9q4xab";

/** Records the main process's trusted engine requests and answers them per command. */
class FakeEngine {
  readonly calls: [number, string, Readonly<Record<string, unknown>>][] = [];
  readonly answers = new Map<string, (args: Readonly<Record<string, unknown>>) => Promise<unknown>>();

  request(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    this.calls.push([windowId, command, args]);
    return this.answers.get(command)?.(args) ?? Promise.resolve(null);
  }
  /** The commands requested, without `save_window_bounds` (every new window saves its bounds). */
  commands(): [number, string, Readonly<Record<string, unknown>>][] {
    return this.calls.filter(([, command]) => command !== "save_window_bounds");
  }
}

function setup() {
  const engine = new FakeEngine();
  const registry = new WindowRegistry(
    UI,
    { onLoaded: () => undefined, onClosed: () => undefined, onPlacementChanged: () => undefined },
    false,
  );
  registerWindowCommands({ registry, workspace: new Workspace(registry, engine), engine, ui: UI });
  const handler = ipcHandlers.get(IPC.windowCommand);
  if (!handler) {
    throw new Error("window commands not registered");
  }
  const open = (): WindowEntry & { window: FakeBrowserWindow } => {
    const entry = registry.create() as WindowEntry & { window: FakeBrowserWindow };
    entry.window.finishLoad();
    return entry;
  };
  const invoke = (entry: WindowEntry, command: string, args: unknown = {}, frame?: { url: string }) => {
    const window = entry.window as unknown as FakeBrowserWindow;
    return handler({ sender: window.webContents, senderFrame: frame ?? window.mainFrame }, command, args);
  };
  return { engine, registry, open, invoke };
}

beforeEach(() => {
  resetFakeElectron();
});

describe("add_project", () => {
  it("shows the folder picker, then adds the folder with the trusted add_project", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: { id: PROJECT, name: "shop" } }));
    dialogs.folder = "/code/shop";

    expect(await invoke(entry, "add_project")).toEqual({ ok: true, value: { status: "added", project: PROJECT } });
    expect(dialogs.shown.map((d) => d.options["title"])).toEqual(["Add Project"]);
    expect(engine.commands()).toEqual([[entry.id, "add_project", { path: "/code/shop" }]]);
    // The caller has the result: no project_added event as well.
    expect(entry.window.sent).toEqual([]);
  });

  it("resolves with null when the picker is cancelled", async () => {
    const { engine, open, invoke } = setup();
    expect(await invoke(open(), "add_project")).toEqual({ ok: true, value: null });
    expect(engine.commands()).toEqual([]);
  });

  it("rejects with the engine's error", async () => {
    const { engine, open, invoke } = setup();
    engine.answers.set("add_project", () =>
      Promise.reject(new EngineRequestError({ code: "too_many_projects", message: "The workspace has 64 projects." })),
    );
    dialogs.folder = "/code/shop";
    expect(await invoke(open(), "add_project")).toEqual({
      ok: false,
      error: { code: "too_many_projects", message: "The workspace has 64 projects." },
    });
  });

  it("refuses a malformed engine result", async () => {
    const { engine, open, invoke } = setup();
    engine.answers.set("add_project", () => Promise.resolve({ status: "added", project: PROJECT }));
    dialogs.folder = "/code/shop";
    expect(await invoke(open(), "add_project")).toMatchObject({ ok: false, error: { code: "internal" } });
  });
});

describe("add_dropped_folder (from the preload)", () => {
  it("adds the dropped path with the trusted add_project", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    engine.answers.set("add_project", () => Promise.resolve({ status: "exists", project: { id: PROJECT } }));
    expect(await invoke(entry, PRELOAD_COMMANDS.addDroppedFolder, { path: "/code/shop" })).toEqual({
      ok: true,
      value: { status: "exists", project: PROJECT },
    });
    expect(engine.commands()).toEqual([[entry.id, "add_project", { path: "/code/shop" }]]);
  });

  it("refuses a missing path and passes on not_a_folder", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    expect(await invoke(entry, PRELOAD_COMMANDS.addDroppedFolder, {})).toMatchObject({
      ok: false,
      error: { code: "invalid_command" },
    });
    engine.answers.set("add_project", () =>
      Promise.reject(new EngineRequestError({ code: "not_a_folder", message: "Not a folder." })),
    );
    expect(await invoke(entry, PRELOAD_COMMANDS.addDroppedFolder, { path: "/code/notes.txt" })).toEqual({
      ok: false,
      error: { code: "not_a_folder", message: "Not a folder." },
    });
  });
});

describe("confirm_remove_project", () => {
  it("asks with the project's name and reports the answer", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    expect(await invoke(entry, "confirm_remove_project", { name: "shop" })).toEqual({
      ok: true,
      value: { confirmed: true },
    });
    dialogs.response = 1;
    expect(await invoke(entry, "confirm_remove_project", { name: "shop" })).toEqual({
      ok: true,
      value: { confirmed: false },
    });
    expect(dialogs.shown[0]?.options).toMatchObject({
      message: "Remove shop from the workspace?",
      detail: "Its .unfold folder stays.",
      buttons: ["Remove", "Cancel"],
      cancelId: 1,
    });
    // The UI removes the project itself (engine remove_project); nothing reaches the engine here.
    expect(engine.commands()).toEqual([]);
  });

  it("refuses a missing or oversized name", async () => {
    const { open, invoke } = setup();
    const entry = open();
    for (const args of [{}, { name: "" }, { name: 3 }, { name: "x".repeat(65) }]) {
      expect(await invoke(entry, "confirm_remove_project", args)).toMatchObject({
        ok: false,
        error: { code: "invalid_command" },
      });
    }
    expect(dialogs.shown).toEqual([]);
  });
});

describe("export_mermaid", () => {
  it("exports a board page to the chosen path with the trusted export_mermaid", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    engine.answers.set("export_mermaid", () => Promise.resolve({ path: "/out/Orders.mmd", bytes: 120 }));
    dialogs.savePath = "/out/Orders.mmd";

    const reply = await invoke(entry, "export_mermaid", {
      project: PROJECT,
      board: ".unfold/boards/orders.json",
      page: "page-1",
      title: "Orders: lifecycle",
    });
    expect(reply).toEqual({ ok: true, value: { path: "/out/Orders.mmd", bytes: 120 } });
    expect(dialogs.shown[0]?.options["defaultPath"]).toBe("/Users/me/Documents/Orders- lifecycle.mmd");
    expect(engine.commands()).toEqual([
      [
        entry.id,
        "export_mermaid",
        { project: PROJECT, board: { id: ".unfold/boards/orders.json", page: "page-1" }, path: "/out/Orders.mmd" },
      ],
    ]);
  });

  it("exports an entrance, named after the board id or entrance when there is no title", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    dialogs.savePath = "/out/e.mmd";
    await invoke(entry, "export_mermaid", { project: PROJECT, entrance: "entrance:http:GET:/orders" });
    expect(engine.commands()).toEqual([
      [entry.id, "export_mermaid", { project: PROJECT, entrance: "entrance:http:GET:/orders", path: "/out/e.mmd" }],
    ]);
    await invoke(entry, "export_mermaid", { project: PROJECT, board: ".unfold/boards/orders.json", page: "page-1" });
    expect(dialogs.shown.map((d) => d.options["defaultPath"])).toEqual([
      "/Users/me/Documents/entrance-http-GET-orders.mmd",
      "/Users/me/Documents/orders.mmd",
    ]);
  });

  it("resolves with null when the dialog is cancelled, and passes on the engine's error", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    const args = { project: PROJECT, board: ".unfold/boards/orders.json", page: "page-1" };
    expect(await invoke(entry, "export_mermaid", args)).toEqual({ ok: true, value: null });
    expect(engine.commands()).toEqual([]);

    dialogs.savePath = "/out/o.mmd";
    engine.answers.set("export_mermaid", () =>
      Promise.reject(new EngineRequestError({ code: "board_not_held", message: "Open it first." })),
    );
    expect(await invoke(entry, "export_mermaid", args)).toEqual({
      ok: false,
      error: { code: "board_not_held", message: "Open it first." },
    });
  });

  it("refuses anything but exactly one source with a project", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    for (const args of [
      { board: "b", page: "p" },
      { project: PROJECT, board: "b" },
      { project: PROJECT, board: "b", page: "p", entrance: "e" },
      { project: PROJECT, entrance: "e", page: "p" },
      { project: PROJECT, entrance: "e", title: 3 },
      { project: PROJECT, boardId: "b" },
    ]) {
      expect(await invoke(entry, "export_mermaid", args)).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
    expect(dialogs.shown).toEqual([]);
    expect(engine.commands()).toEqual([]);
  });
});

describe("windows", () => {
  it("new_window and move_tab_to_new_window open a window and resolve with its id at once", async () => {
    const { registry, open, invoke } = setup();
    const entry = open();
    const created = await invoke(entry, "new_window");
    const moved = await invoke(entry, "move_tab_to_new_window");
    expect(registry.all().map((e) => e.id)).toEqual([entry.id, entry.id + 1, entry.id + 2]);
    expect(created).toEqual({ ok: true, value: { windowId: entry.id + 1 } });
    expect(moved).toEqual({ ok: true, value: { windowId: entry.id + 2 } });
  });

  it("focus_window focuses a window, and with a board tells it to show that board", async () => {
    const { registry, open, invoke } = setup();
    const caller = open();
    const holder = open();
    const board = { project: PROJECT, board: ".unfold/boards/orders.json" };

    expect(await invoke(caller, "focus_window", { windowId: holder.id })).toEqual({ ok: true, value: null });
    expect(registry.focused()).toBe(holder);
    expect(holder.window.sent).toEqual([]);

    await invoke(caller, "focus_window", { windowId: holder.id, board });
    expect(holder.window.sent).toEqual([{ type: "focus_board", ...board }]);
  });

  it("focus_window refuses an unknown window or a malformed board", async () => {
    const { open, invoke } = setup();
    const caller = open();
    for (const args of [{ windowId: 99 }, { windowId: "1" }, { windowId: caller.id, board: { project: PROJECT } }]) {
      expect(await invoke(caller, "focus_window", args)).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
  });

  it("transfer_board_state hands the payload to the other window once its UI has loaded", async () => {
    const { registry, open, invoke } = setup();
    const from = open();
    const to = registry.create() as WindowEntry & { window: FakeBrowserWindow };
    const payload = { undo: [1, 2] };

    expect(await invoke(from, "transfer_board_state", { toWindow: to.id, payload })).toEqual({ ok: true, value: null });
    expect(to.window.sent).toEqual([]);
    to.window.finishLoad();
    expect(to.window.sent).toEqual([{ type: "board_state_transferred", fromWindow: from.id, payload }]);

    expect(await invoke(from, "transfer_board_state", { toWindow: from.id, payload })).toMatchObject({
      ok: false,
      error: { code: "invalid_command" },
    });
  });

  it("window_info describes the window without a folder", async () => {
    const { open, invoke } = setup();
    const entry = open();
    const reply = await invoke(entry, "window_info");
    expect(reply).toEqual({ ok: true, value: { windowId: entry.id, key: entry.key, platform: expect.any(String) as string } });
  });

  it("close_window closes the calling window", async () => {
    const { registry, open, invoke } = setup();
    const entry = open();
    await invoke(entry, "close_window");
    expect(registry.all()).toEqual([]);
  });
});

describe("copy_to_clipboard", () => {
  it("writes text and refuses anything else", async () => {
    const { open, invoke } = setup();
    const entry = open();
    expect(await invoke(entry, "copy_to_clipboard", { text: "graph TD" })).toEqual({ ok: true, value: null });
    expect(await invoke(entry, "copy_to_clipboard", { text: 1 })).toMatchObject({ ok: false });
    expect(clipboardTexts).toEqual(["graph TD"]);
  });
});

describe("the window command channel", () => {
  it("refuses deleted folder commands and unknown names", async () => {
    const { open, invoke } = setup();
    const entry = open();
    for (const command of [
      "close_folder",
      "choose_folder",
      "open_folder",
      "choose_export_path",
      "open_dropped_folder",
      "toString",
      3,
    ]) {
      expect(await invoke(entry, command as string)).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
  });

  it("refuses args that are not an object", async () => {
    const { open, invoke } = setup();
    expect(await invoke(open(), "window_info", [])).toMatchObject({ ok: false, error: { code: "invalid_command" } });
  });

  it("refuses commands from a subframe or from a page that isn't Unfold's UI", async () => {
    const { engine, open, invoke } = setup();
    const entry = open();
    dialogs.folder = "/code/shop";

    const fromSubframe = await invoke(entry, "add_project", {}, { url: "http://localhost:5173/" });
    entry.window.mainFrame.url = "https://example.com/";
    const fromElsewhere = await invoke(entry, "add_project");

    for (const reply of [fromSubframe, fromElsewhere]) {
      expect(reply).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
    expect(dialogs.shown).toEqual([]);
    expect(engine.commands()).toEqual([]);
  });
});
