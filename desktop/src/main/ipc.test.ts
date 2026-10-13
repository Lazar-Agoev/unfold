import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentServerStatus } from "../shared/agent-api";
import { IPC } from "../shared/transport";
import { EngineRequestError } from "./engine-host";
import { clipboardTexts, dialogs, FakeBrowserWindow, ipcHandlers, resetFakeElectron } from "./fake-electron";
import { AgentBridge } from "./agents/bridge";
import type { ScreenshotOutcome, ScreenshotRequest } from "./agents/render";
import { ViewTracker } from "./agents/views";
import { registerWindowCommands } from "./ipc";
import type { WindowEntry } from "./windows";
import { AppWindow } from "./windows";
import { Workspace } from "./workspace";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

const UI = { kind: "dev", url: "http://localhost:5173/" } as const;
const PROJECT = "p-7k2m9q4xab";
const SERVER_STATUS: AgentServerStatus = {
  state: "listening",
  address: "http://127.0.0.1:29180/mcp",
  agents: [{ author: "agent:claude", name: "Claude", lastSeenMs: 2, lastActedMs: 1 }],
};

/** Records the main process's trusted engine requests and answers them per command. */
class FakeEngine {
  readonly calls: [number, string, Readonly<Record<string, unknown>>][] = [];
  readonly answers = new Map<string, (args: Readonly<Record<string, unknown>>) => Promise<unknown>>();

  request(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    this.calls.push([windowId, command, args]);
    return this.answers.get(command)?.(args) ?? Promise.resolve(null);
  }
  /** The commands requested, without `save_window_state` (the window saves its state as it changes). */
  commands(): [number, string, Readonly<Record<string, unknown>>][] {
    return this.calls.filter(([, command]) => command !== "save_window_state");
  }
}

/** Stands in for the page renderer: owns one render window, and records what it was asked to draw. */
class FakeRenderer {
  readonly window = new FakeBrowserWindow({ width: 1600, height: 1000 });
  readonly settled: [unknown, unknown][] = [];
  readonly shots: ScreenshotRequest[] = [];
  outcome: ScreenshotOutcome = { ok: true, png: Buffer.from("a png"), width: 4, height: 2 };

  owns(contents: unknown): boolean {
    return contents === this.window.webContents;
  }
  settle(requestId: unknown, result: unknown): void {
    this.settled.push([requestId, result]);
  }
  screenshot(request: ScreenshotRequest): Promise<ScreenshotOutcome> {
    this.shots.push(request);
    return Promise.resolve(this.outcome);
  }
}

function setup() {
  const engine = new FakeEngine();
  const renderer = new FakeRenderer();
  const appWindow = new AppWindow(
    UI,
    { onLoaded: () => undefined, onPlacementChanged: () => undefined, onClosing: () => undefined, onClosed: () => undefined },
    false,
  );
  const agents = new AgentBridge({ windows: appWindow, views: new ViewTracker() });
  const mcp = { status: () => SERVER_STATUS };
  const workspace = new Workspace(appWindow, engine);
  registerWindowCommands({ window: appWindow, workspace, engine, ui: UI, agents, mcp, renderer });
  const handler = ipcHandlers.get(IPC.windowCommand);
  if (!handler) {
    throw new Error("window commands not registered");
  }
  const open = (): WindowEntry & { window: FakeBrowserWindow } => {
    const entry = appWindow.open() as WindowEntry & { window: FakeBrowserWindow };
    entry.window.finishLoad();
    return entry;
  };
  const invoke = (entry: { readonly window: unknown }, command: string, args: unknown = {}, frame?: { url: string }) => {
    const window = entry.window as FakeBrowserWindow;
    return handler({ sender: window.webContents, senderFrame: frame ?? window.mainFrame }, command, args);
  };
  return { engine, appWindow, workspace, agents, renderer, open, invoke };
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
      Promise.reject(new EngineRequestError({ code: "board_not_open", message: "Open it first." })),
    );
    expect(await invoke(entry, "export_mermaid", args)).toEqual({
      ok: false,
      error: { code: "board_not_open", message: "Open it first." },
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

describe("export_png", () => {
  const ARGS = { project: PROJECT, board: ".unfold/boards/orders.json", page: "page-1", title: "Orders - Placing an order" };

  it("draws the page at twice the size and writes it where the person chose", async () => {
    const { open, invoke, renderer } = setup();
    const path = join(mkdtempSync(join(tmpdir(), "unfold-png-")), "orders.png");
    dialogs.savePath = path;

    expect(await invoke(open(), "export_png", ARGS)).toEqual({ ok: true, value: { path, bytes: 5 } });
    expect(dialogs.shown).toEqual([
      {
        kind: "save",
        options: {
          title: "Export",
          defaultPath: "/Users/me/Documents/Orders - Placing an order.png",
          filters: [{ name: "PNG image", extensions: ["png"] }],
          properties: ["createDirectory", "showOverwriteConfirmation"],
        },
      },
    ]);
    expect(renderer.shots).toEqual([{ board: { project: PROJECT, id: ".unfold/boards/orders.json" }, page: "page-1", scale: 2 }]);
    expect(readFileSync(path, "utf8")).toBe("a png");
  });

  it("draws nothing when the dialog is cancelled, and is named after the board without a title", async () => {
    const { open, invoke, renderer } = setup();
    expect(await invoke(open(), "export_png", { ...ARGS, title: undefined })).toEqual({ ok: true, value: null });
    expect(dialogs.shown[0]?.options["defaultPath"]).toBe("/Users/me/Documents/orders.png");
    expect(renderer.shots).toEqual([]);
  });

  it("rejects with why the page couldn't be drawn, and refuses malformed args", async () => {
    const { open, invoke, renderer } = setup();
    const entry = open();
    dialogs.savePath = join(mkdtempSync(join(tmpdir(), "unfold-png-")), "never.png");
    renderer.outcome = { ok: false, code: "render_timeout", message: "Unfold couldn't draw the page within 30 s." };
    expect(await invoke(entry, "export_png", ARGS)).toEqual({ ok: false, error: { code: "render_timeout", message: "Unfold couldn't draw the page within 30 s." } });
    for (const args of [{ project: PROJECT, board: "b" }, { ...ARGS, title: 3 }]) {
      expect(await invoke(entry, "export_png", args)).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
  });
});

describe("the render window", () => {
  it("may only answer render requests, which go to the renderer", async () => {
    const { invoke, renderer } = setup();
    const result = { ok: true, bounds: { x: 0, y: 0, width: 10, height: 10 } };
    expect(await invoke(renderer, "render_ready", { requestId: 3, result })).toEqual({ ok: true, value: null });
    expect(renderer.settled).toEqual([[3, result]]);
    for (const command of ["window_info", "copy_to_clipboard", "export_png"]) {
      expect(await invoke(renderer, command, { text: "x" })).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
    expect(clipboardTexts).toEqual([]);
  });

  it("is the only window that may answer them", async () => {
    const { open, invoke, renderer } = setup();
    expect(await invoke(open(), "render_ready", { requestId: 3, result: { ok: true } })).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    expect(renderer.settled).toEqual([]);
  });
});

describe("the window", () => {
  const BOARD = { project: PROJECT, board: ".unfold/boards/orders.json" };

  it("window_info describes the window and the board it shows: the one saved at launch, else Home", async () => {
    const { engine, workspace, open, invoke } = setup();
    engine.answers.set("window_state", () => Promise.resolve({ bounds: { x: 0, y: 0, width: 900, height: 600 }, maximized: false, board: BOARD }));
    await workspace.restore();
    const entry = open();
    expect(await invoke(entry, "window_info")).toEqual({ ok: true, value: { windowId: entry.id, board: BOARD } });

    resetFakeElectron();
    const fresh = setup();
    expect(await fresh.invoke(fresh.open(), "window_info")).toMatchObject({ ok: true, value: { board: null } });
  });

  it("report_shown_board hands the board the window shows, or Home, to the workspace", async () => {
    const { workspace, open, invoke } = setup();
    const entry = open();
    expect(await invoke(entry, "report_shown_board", { board: BOARD })).toEqual({ ok: true, value: null });
    expect(workspace.shownBoard()).toEqual(BOARD);
    expect(await invoke(entry, "window_info")).toMatchObject({ value: { board: BOARD } });
    expect(await invoke(entry, "report_shown_board", { board: null })).toEqual({ ok: true, value: null });
    expect(workspace.shownBoard()).toBeNull();
  });

  it("report_shown_board refuses a malformed board", async () => {
    const { workspace, open, invoke } = setup();
    const entry = open();
    for (const args of [{}, { board: { project: PROJECT } }, { board: "orders.json" }]) {
      expect(await invoke(entry, "report_shown_board", args)).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
    expect(workspace.shownBoard()).toBeNull();
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

describe("the agent channel", () => {
  const BOARD = { project: PROJECT, id: ".unfold/boards/orders.json" };

  it("report_view keeps the window's view, and refuses a malformed one", async () => {
    const { agents, open, invoke } = setup();
    const entry = open();
    const report = { board: BOARD, page: "page-1", selection: { kind: "none" }, lens: "code", visible: [], tabs: [BOARD] };
    expect(await invoke(entry, "report_view", report)).toEqual({ ok: true, value: null });
    expect(agents.views.get(entry.id)).toEqual(report);
    expect(agents.views.holder(BOARD)).toBe(entry.id);
    expect(await invoke(entry, "report_view", { ...report, selection: { kind: "card" } })).toMatchObject({
      ok: false,
      error: { code: "invalid_command" },
    });
  });

  it("hands a window's answers to the bridge", async () => {
    const { agents, open, invoke } = setup();
    const entry = open();
    await invoke(entry, "report_view", { selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    const read = agents.read({ board: BOARD, page: "page-1" });
    const sent = entry.window.sent.at(-1) as { type: string; requestId: number };
    expect(sent.type).toBe("agent_read");
    const page = { id: "page-1", title: "Placing an order" };
    const board = { title: "Orders", place: "draft", changes: 0, revision: "r1", unsaved: false };
    expect(await invoke(entry, "agent_read_result", { requestId: sent.requestId, result: { ok: true, page, board } })).toEqual({ ok: true, value: null });
    expect(await read).toMatchObject({ ok: true, page, board });
  });

  it("agent_server_status reports the MCP server for Settings", async () => {
    const { open, invoke } = setup();
    expect(await invoke(open(), "agent_server_status")).toEqual({ ok: true, value: SERVER_STATUS });
  });
});

describe("the window command channel", () => {
  it("refuses deleted window commands and unknown names", async () => {
    const { open, invoke } = setup();
    const entry = open();
    for (const command of [
      "focus_window",
      "close_window",
      "add_dropped_folder",
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
