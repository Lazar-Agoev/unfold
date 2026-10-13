/**
 * A stand-in for the `electron` module, for the main process's tests: enough for the app's window
 * to open, address, focus, close, and message, for the render window to be sized and
 * captured, for the window commands to show dialogs, and for the engine host to fork its utility
 * process. Tests install it with
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

/** What a fake capture returns: an image of the captured size at the window's scale. */
export interface FakeImage {
  getSize(scaleFactor: number): { width: number; height: number };
  toPNG(options: { scaleFactor: number }): Buffer;
}

export class FakeBrowserWindow {
  static focused: FakeBrowserWindow | null = null;
  /** The largest content size a window may take (a screen's limit), or null for any. */
  static largest: { width: number; height: number } | null = null;

  /** Shell events sent to the window's UI, in order. */
  readonly sent: unknown[] = [];
  /** Every capture: its rectangle and options. */
  readonly captures: { rect: Rect; options: unknown }[] = [];
  readonly mainFrame = { url: "http://localhost:5173/" };
  readonly webContents = {
    mainFrame: this.mainFrame,
    on: (event: string, handler: () => void) => {
      this.contentsHandlers.set(event, handler);
    },
    send: (_channel: string, event: unknown) => {
      this.sent.push(event);
    },
    capturePage: (rect: Rect, options: unknown): Promise<FakeImage> => {
      this.captures.push({ rect, options });
      const scale = this.scale();
      return Promise.resolve({
        getSize: (factor: number) => ({ width: rect.width * factor, height: rect.height * factor }),
        toPNG: ({ scaleFactor }: { scaleFactor: number }) => Buffer.from(`png ${rect.width * scale}x${rect.height * scale} at ${scaleFactor}`),
      });
    },
  };
  readonly bounds: Rect;
  /** The URL the window loaded. */
  url: string | null = null;
  contentSize: [number, number];
  maximized = false;
  minimized = false;
  /** How many times the window was shown. */
  shows = 0;
  private destroyed = false;
  private readonly handlers = new Map<string, (() => void)[]>();
  private readonly contentsHandlers = new Map<string, () => void>();

  constructor(readonly options: Partial<Rect> & { width: number; height: number; webPreferences?: Record<string, unknown> }) {
    this.bounds = { x: options.x ?? 0, y: options.y ?? 0, width: options.width, height: options.height };
    this.contentSize = [options.width, options.height];
    createdWindows.push(this);
  }

  on(event: string, handler: () => void): void {
    this.handlers.set(event, [...(this.handlers.get(event) ?? []), handler]);
  }
  once(event: string, handler: () => void): void {
    this.on(event, handler);
  }
  loadURL(url: string): Promise<void> {
    this.url = url;
    return Promise.resolve();
  }
  setContentSize(width: number, height: number): void {
    const largest = FakeBrowserWindow.largest;
    this.contentSize = largest ? [Math.min(width, largest.width), Math.min(height, largest.height)] : [width, height];
  }
  getContentSize(): number[] {
    return [...this.contentSize];
  }
  isDestroyed(): boolean {
    return this.destroyed;
  }
  getNormalBounds(): Rect {
    return { ...this.bounds };
  }
  isMaximized(): boolean {
    return this.maximized;
  }
  isMinimized(): boolean {
    return this.minimized;
  }
  maximize(): void {
    this.maximized = true;
  }
  restore(): void {
    this.maximized = false;
    this.minimized = false;
  }
  show(): void {
    this.shows += 1;
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
  /** Simulates the renderer emitting a contents event (`render-process-gone`, …). */
  emitContents(event: string): void {
    this.contentsHandlers.get(event)?.();
  }
  /** The offscreen device scale factor the window was created with (1 when not offscreen). */
  private scale(): number {
    const offscreen = this.options.webPreferences?.["offscreen"];
    const factor = typeof offscreen === "object" && offscreen !== null ? (offscreen as { deviceScaleFactor?: number }).deviceScaleFactor : undefined;
    return factor ?? 1;
  }
  /** Simulates Electron emitting a window event (`ready-to-show`, `move`, …). */
  emit(event: string): void {
    for (const handler of this.handlers.get(event) ?? []) {
      handler();
    }
  }
}

/** The engine host's utility process: what the main process posted, and a way to answer. */
export class FakeUtilityProcess {
  /** Every message the main process posted, in order. */
  readonly posted: unknown[] = [];
  private readonly handlers = new Map<string, ((value: unknown) => void)[]>();

  postMessage(message: unknown): void {
    this.posted.push(message);
  }
  on(event: string, handler: (value: unknown) => void): void {
    this.handlers.set(event, [...(this.handlers.get(event) ?? []), handler]);
  }
  once(event: string, handler: (value: unknown) => void): void {
    this.on(event, handler);
  }
  kill(): void {
    this.emit("exit", 0);
  }
  /** Simulates the process sending a message (`message`) or exiting (`exit`). */
  emit(event: string, value: unknown): void {
    for (const handler of this.handlers.get(event) ?? []) {
      handler(value);
    }
  }
}

/** The utility processes forked, in order. */
export const forked: FakeUtilityProcess[] = [];

/** Every window created, in order. */
export const createdWindows: FakeBrowserWindow[] = [];

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

/** The options of every `app.focus` call. */
export const appFocuses: unknown[] = [];

/** Forgets every window, handler, and dialog answer. */
export function resetFakeElectron(): void {
  FakeBrowserWindow.focused = null;
  FakeBrowserWindow.largest = null;
  createdWindows.length = 0;
  forked.length = 0;
  ipcHandlers.clear();
  clipboardTexts.length = 0;
  appFocuses.length = 0;
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
  app: {
    getPath: () => "/Users/me/Documents",
    focus: (options?: unknown) => {
      appFocuses.push(options);
    },
  },
  screen: {
    getPrimaryDisplay: () => ({ id: 1, workArea }),
    getAllDisplays: () => [{ id: 1, workArea }],
  },
  utilityProcess: {
    fork: () => {
      const child = new FakeUtilityProcess();
      forked.push(child);
      return child;
    },
  },
  // Imported by modules under test, but never used by the tests.
  session: null,
  MessageChannelMain: null,
};
