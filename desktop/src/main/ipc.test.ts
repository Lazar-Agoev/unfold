import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WindowReply } from "../shared/transport";
import { IPC } from "../shared/transport";
import type { ShellEvent } from "../shared/unfold-api";

type InvokeHandler = (event: unknown, command: unknown, args?: unknown) => Promise<WindowReply>;

const electron = vi.hoisted(() => {
  /** A stand-in BrowserWindow: enough for the registry to create, address, and message windows. */
  class FakeBrowserWindow {
    static focused: FakeBrowserWindow | null = null;
    static getFocusedWindow(): FakeBrowserWindow | null {
      return FakeBrowserWindow.focused;
    }
    readonly sent: unknown[] = [];
    readonly mainFrame = { url: "http://localhost:5173/" };
    readonly webContents = {
      mainFrame: this.mainFrame,
      on: () => undefined,
      send: (_channel: string, event: unknown) => {
        this.sent.push(event);
      },
    };
    destroyed = false;
    readonly once = (): undefined => undefined;
    readonly on = (): undefined => undefined;
    loadURL(): Promise<void> {
      return Promise.resolve();
    }
    isDestroyed(): boolean {
      return this.destroyed;
    }
    getPosition(): number[] {
      return [0, 0];
    }
    readonly close = (): undefined => undefined;
  }
  return { FakeBrowserWindow, handlers: new Map<string, InvokeHandler>() };
});

vi.mock("electron", () => ({
  BrowserWindow: electron.FakeBrowserWindow,
  ipcMain: {
    handle: (channel: string, handler: InvokeHandler) => {
      electron.handlers.set(channel, handler);
    },
  },
  clipboard: { writeText: () => undefined },
  nativeTheme: { shouldUseDarkColors: false },
  screen: {},
  app: {},
  session: {},
  dialog: {},
  MessageChannelMain: vi.fn(),
  utilityProcess: {},
}));

const { Folders } = await import("./folders");
const { registerWindowCommands } = await import("./ipc");
const { WindowRegistry } = await import("./windows");

const UI = { kind: "dev", url: "http://localhost:5173/" } as const;
type FakeWindow = InstanceType<typeof electron.FakeBrowserWindow>;

function setup() {
  /** Every main-process step, in order: engine requests and the window's re-attach. */
  const steps: string[] = [];
  const registry = new WindowRegistry(
    UI,
    {
      onLoaded: () => undefined,
      // As in index.ts: a folder change re-attaches the window to the engine host with its new folder.
      onFolderChanged: (entry) => steps.push(`attach ${entry.id} ${String(entry.folder)}`),
      onClosed: () => undefined,
      onPlacementChanged: () => undefined,
    },
    false,
  );
  const engine = {
    request: (windowId: number, command: string) => {
      steps.push(`${command} ${windowId}`);
      return Promise.resolve(null);
    },
    whenAttached: () => Promise.resolve(null),
  };
  registerWindowCommands({ registry, folders: new Folders(registry, engine), engine, ui: UI });
  const handler = electron.handlers.get(IPC.windowCommand);
  if (!handler) {
    throw new Error("window commands not registered");
  }
  const invoke = (window: FakeWindow, command: string, frame: { url: string } = window.mainFrame) =>
    handler({ sender: window.webContents, senderFrame: frame }, command, {});
  return { registry, steps, invoke };
}

const shellEvents = (window: FakeWindow) => window.sent as ShellEvent[];

describe("close_folder", () => {
  beforeEach(() => {
    electron.handlers.clear();
  });

  it("returns the window to the welcome screen and leaves other windows on the folder alone", async () => {
    const { registry, steps, invoke } = setup();
    const closing = registry.create("/repo");
    const other = registry.create("/repo");
    const oldKey = closing.key;

    const reply = await invoke(closing.window as unknown as FakeWindow, "close_folder");

    expect(reply).toEqual({ ok: true, value: { windowKey: closing.key } });
    expect(closing.key).not.toBe(oldKey);
    expect(closing.folder).toBeNull();
    expect(other.folder).toBe("/repo");
    // The window's state in the folder is forgotten first (it must still be attached), then the
    // window re-attaches with no folder, which detaches it from the folder's workspace.
    expect(steps).toEqual([`forget_window_state ${closing.id}`, `attach ${closing.id} null`]);
    expect(shellEvents(closing.window as unknown as FakeWindow)).toEqual([
      { type: "folder_changed", folder: null, windowKey: closing.key },
    ]);
    expect(shellEvents(other.window as unknown as FakeWindow)).toEqual([]);
  });

  it("keeps the state of the folder's last window, so it comes back", async () => {
    const { registry, steps, invoke } = setup();
    const last = registry.create("/repo");
    await invoke(last.window as unknown as FakeWindow, "close_folder");
    expect(steps).toEqual([`attach ${last.id} null`]);
    expect(last.folder).toBeNull();
  });

  it("is a no-op on the welcome screen", async () => {
    const { registry, steps, invoke } = setup();
    const welcome = registry.create(null);
    expect(await invoke(welcome.window as unknown as FakeWindow, "close_folder")).toEqual({ ok: true, value: null });
    expect(steps).toEqual([]);
  });

  it("is refused from a subframe or from a page that isn't Unfold's UI", async () => {
    const { registry, steps, invoke } = setup();
    const entry = registry.create("/repo");
    const window = entry.window as unknown as FakeWindow;

    const fromSubframe = await invoke(window, "close_folder", { url: "http://localhost:5173/" });
    window.mainFrame.url = "https://example.com/";
    const fromElsewhere = await invoke(window, "close_folder");

    for (const reply of [fromSubframe, fromElsewhere]) {
      expect(reply).toMatchObject({ ok: false, error: { code: "invalid_command" } });
    }
    expect(entry.folder).toBe("/repo");
    expect(steps).toEqual([]);
  });
});
