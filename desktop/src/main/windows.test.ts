import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FakeBrowserWindow } from "./fake-electron";
import { createdWindows, resetFakeElectron } from "./fake-electron";
import type { WindowEntry, WindowHooks } from "./windows";
import { AppWindow } from "./windows";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

type Entry = WindowEntry & { window: FakeBrowserWindow };

function setup() {
  const heard: string[] = [];
  const hooks: WindowHooks = {
    onLoaded: (entry) => heard.push(`loaded ${entry.id}`),
    onPlacementChanged: (entry) => heard.push(`placement ${entry.id}`),
    onClosing: (entry) => heard.push(`closing ${entry.id}`),
    onClosed: (entry) => heard.push(`closed ${entry.id}`),
  };
  const appWindow = new AppWindow({ kind: "dev", url: "http://localhost:5173/" }, hooks, false);
  const open = (placement?: Parameters<AppWindow["open"]>[0]) => appWindow.open(placement) as Entry;
  return { appWindow, open, heard };
}

beforeEach(() => {
  resetFakeElectron();
});

describe("the app's window", () => {
  it("is one window, with no title bar and the window buttons over the sidebar, in the light theme", () => {
    const { appWindow, open } = setup();
    const entry = open();
    expect(open()).toBe(entry);
    expect(createdWindows).toHaveLength(1);
    expect(appWindow.current()).toBe(entry);
    expect(entry.window.url).toBe("http://localhost:5173/");
    expect(entry.window.options).toMatchObject({
      titleBarStyle: "hidden",
      trafficLightPosition: { x: 16, y: 18 },
      backgroundColor: "#F7F8FA",
      webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false },
    });
  });

  it("opens at the placement it is given, maximized once it can be shown", () => {
    const { open } = setup();
    const entry = open({ bounds: { x: 40, y: 30, width: 1000, height: 700 }, maximized: true });
    expect(entry.window.bounds).toEqual({ x: 40, y: 30, width: 1000, height: 700 });
    expect(entry.window.maximized).toBe(false);
    entry.window.emit("ready-to-show");
    expect(entry.window.maximized).toBe(true);
    expect(entry.window.shows).toBe(1);
  });

  it("reports moves and resizes, then closing (while it can report where it is) and closed", () => {
    const { appWindow, open, heard } = setup();
    const entry = open();
    for (const event of ["move", "resize", "maximize", "unmaximize"]) {
      entry.window.emit(event);
    }
    entry.window.close();
    expect(heard).toEqual([`placement ${entry.id}`, `placement ${entry.id}`, `placement ${entry.id}`, `placement ${entry.id}`, `closing ${entry.id}`, `closed ${entry.id}`]);
    expect(appWindow.current()).toBeUndefined();
    expect(appWindow.get(entry.id)).toBeUndefined();
  });

  it("opens again where it was when it closed, with a new id the render window's ids never take", () => {
    const { appWindow, open } = setup();
    const first = open({ bounds: { x: 40, y: 30, width: 1000, height: 700 }, maximized: false });
    first.window.bounds.width = 1100;
    first.window.maximized = true;
    first.window.close();
    const reserved = appWindow.reserveId();

    const second = open();
    expect(new Set([first.id, reserved, second.id]).size).toBe(3);
    expect(second.window.bounds).toEqual({ x: 40, y: 30, width: 1100, height: 700 });
    second.window.emit("ready-to-show");
    expect(second.window.maximized).toBe(true);
  });

  it("holds shell events until its UI has loaded, and drops them when it closes first", () => {
    const { appWindow, open, heard } = setup();
    const entry = open();
    appWindow.deliver(entry, { type: "project_added", project: "p-1" });
    expect(entry.window.sent).toEqual([]);
    entry.window.finishLoad();
    expect(entry.window.sent).toEqual([{ type: "project_added", project: "p-1" }]);
    expect(heard).toEqual([`loaded ${entry.id}`]);

    const closing = open();
    closing.window.close();
    const next = open();
    appWindow.deliver(closing, { type: "project_added", project: "p-2" });
    next.window.finishLoad();
    expect(next.window.sent).toEqual([]);
  });

  it("finds itself by its web contents and its window, and nothing else", () => {
    const { appWindow, open } = setup();
    const entry = open();
    expect(appWindow.forWebContents(entry.window.webContents as never)).toBe(entry);
    expect(appWindow.forWindow(entry.window as never)).toBe(entry);
    expect(appWindow.forWindow(undefined)).toBeUndefined();
    expect(appWindow.get(entry.id)).toBe(entry);
    expect(appWindow.get(entry.id + 1)).toBeUndefined();
  });

  it("comes to the front when focused, restored if minimized", () => {
    const { appWindow, open } = setup();
    const entry = open();
    entry.window.minimized = true;
    appWindow.focus(entry);
    expect(entry.window.minimized).toBe(false);
    expect(entry.window.shows).toBe(1);
  });
});
