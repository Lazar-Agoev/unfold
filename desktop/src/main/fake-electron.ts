/**
 * A stand-in for the `electron` module, for the main process's tests: enough for the window registry
 * to create, address, focus, close, and message windows, and for the window commands to show
 * dialogs. Tests install it with
 * `vi.mock("electron", async () => (await import("./fake-electron")).electronModule)` and drive it
 * through the exports below.
 */

import type { WindowReply } from "../shared/transport";

type InvokeHandler = (event: unknown, command: unknown, args?: unknown) => Promise<WindowReply>;

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export class FakeBrowserWindow {
  static focused: FakeBrowserWindow | null = null;
  static getFocusedWindow(): FakeBrowserWindow | null {
    return FakeBrowserWindow.focused;
  }

  /** Shell events sent to the window's UI, in order. */
  readonly sent: unknown[] = [];
  readonly mainFrame = { url: "http://localhost:5173/" };
  readonly webContents = {
    mainFrame: this.mainFrame,
    on: (event: string, handler: () => void) => {
      this.contentsHandlers.set(event, handler);
    },
    send: (_channel: string, event: unknown) => {
      this.sent.push(event);
    },
  };
  readonly bounds: Rect;
  maximized = false;
  private destroyed = false;
  private readonly handlers = new Map<string, (() => void)[]>();
  private readonly contentsHandlers = new Map<string, () => void>();

  constructor(options: Partial<Rect> & { width: number; height: number }) {
    this.bounds = { x: options.x ?? 0, y: options.y ?? 0, width: options.width, height: options.height };
  }

  on(event: string, handler: () => void): void {
    this.handlers.set(event, [...(this.handlers.get(event) ?? []), handler]);
  }
  once(event: string, handler: () => void): void {
    this.on(event, handler);
  }
  loadURL(): Promise<void> {
    return Promise.resolve();
  }
  isDestroyed(): boolean {
    return this.destroyed;
  }
  getPosition(): number[] {
    return [this.bounds.x, this.bounds.y];
  }
  getNormalBounds(): Rect {
    return { ...this.bounds };
  }
  isMaximized(): boolean {
    return this.maximized;
  }
  isMinimized(): boolean {
    return false;
  }
  maximize(): void {
    this.maximized = true;
  }
  restore(): void {
    this.maximized = false;
  }
  show(): void {
    // Shown windows need no state here.
  }
  focus(): void {
    FakeBrowserWindow.focused = this;
    this.emit("focus");
  }
  close(): void {
    this.emit("close");
    this.destroyed = true;
    if (FakeBrowserWindow.focused === this) {
      FakeBrowserWindow.focused = null;
    }
    this.emit("closed");
  }

  /** Simulates the UI finishing loading. */
  finishLoad(): void {
    this.contentsHandlers.get("did-finish-load")?.();
  }
  /** Simulates Electron emitting a window event (`ready-to-show`, `move`, …). */
  emit(event: string): void {
    for (const handler of this.handlers.get(event) ?? []) {
      handler();
    }
  }
}

/** What the dialogs answer, and what they were asked. */
export const dialogs = {
  /** The folder the picker returns; null cancels it. */
  folder: null as string | null,
  /** The path the save dialog returns; null cancels it. */
  savePath: null as string | null,
  /** The button the message box returns (0: Remove, 1: Cancel). */
  response: 0,
  /** Every dialog shown: its kind and options. */
  shown: [] as { kind: string; options: Record<string, unknown> }[],
};

/** The `ipcMain.handle` handlers, by channel. */
export const ipcHandlers = new Map<string, InvokeHandler>();

/** What `clipboard.writeText` received. */
export const clipboardTexts: string[] = [];

/** Forgets every window, handler, and dialog answer. */
export function resetFakeElectron(): void {
  FakeBrowserWindow.focused = null;
  ipcHandlers.clear();
  clipboardTexts.length = 0;
  dialogs.folder = null;
  dialogs.savePath = null;
  dialogs.response = 0;
  dialogs.shown = [];
}

const workArea = { x: 0, y: 0, width: 1920, height: 1080 };

export const electronModule = {
  BrowserWindow: FakeBrowserWindow,
  ipcMain: {
    handle: (channel: string, handler: InvokeHandler) => {
      ipcHandlers.set(channel, handler);
    },
  },
  clipboard: {
    writeText: (text: string) => {
      clipboardTexts.push(text);
    },
  },
  dialog: {
    showOpenDialog: (_parent: unknown, options: Record<string, unknown>) => {
      dialogs.shown.push({ kind: "open", options });
      return Promise.resolve({ canceled: dialogs.folder === null, filePaths: dialogs.folder ? [dialogs.folder] : [] });
    },
    showSaveDialog: (_parent: unknown, options: Record<string, unknown>) => {
      dialogs.shown.push({ kind: "save", options });
      return Promise.resolve({ canceled: dialogs.savePath === null, filePath: dialogs.savePath ?? "" });
    },
    showMessageBox: (_parent: unknown, options: Record<string, unknown>) => {
      dialogs.shown.push({ kind: "message", options });
      return Promise.resolve({ response: dialogs.response });
    },
  },
  app: { getPath: () => "/Users/me/Documents" },
  nativeTheme: { shouldUseDarkColors: false },
  screen: {
    getPrimaryDisplay: () => ({ id: 1, workArea }),
    getAllDisplays: () => [{ id: 1, workArea }],
  },
  // Imported by modules under test, but never used by the tests.
  session: null,
  MessageChannelMain: null,
  utilityProcess: null,
};
