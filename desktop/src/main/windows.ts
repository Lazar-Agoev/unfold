/**
 * The app's one window (desktop/README.md#transport-and-windows). It has no title bar: the macOS
 * window buttons sit over the sidebar, and the UI's top bar moves the window. On macOS, closing it
 * keeps the app running; it opens again from the Dock, a second launch, or an agent that needs it,
 * where it was last. Each opening gets a fresh numeric id, used on the wire to the engine, so a
 * closed window's detach never touches the next one.
 */

import { join } from "node:path";
import type { BaseWindow, WebContents, WebPreferences } from "electron";
import { BrowserWindow, screen } from "electron";
import type { ShellEvent } from "../shared/unfold-api";
import { IPC } from "../shared/transport";
import { entryUrl, type UiSource } from "./app-url";
import type { Rect, WindowPlacement } from "./window-placement";
import { placeOnScreen } from "./window-placement";

export interface WindowEntry {
  readonly id: number;
  readonly window: BrowserWindow;
  /** True once the UI has loaded (and after each reload); only then can it receive an engine port. */
  loaded: boolean;
}

/** What the rest of the main process wants to hear about the window. */
export interface WindowHooks {
  /** The UI finished loading (first load or a reload): hand it a fresh engine port. */
  onLoaded(entry: WindowEntry): void;
  /** The person moved, resized, maximized, or restored the window. */
  onPlacementChanged(entry: WindowEntry): void;
  /** The window is closing and can still report where it is. */
  onClosing(entry: WindowEntry): void;
  /** The window closed. */
  onClosed(entry: WindowEntry): void;
}

/**
 * Every window's web preferences (docs/architecture.md#security): the preload and nothing else, context isolation,
 * the sandbox, no Node, no webviews, no spellchecker. The render window starts from these too.
 */
export function hardenedPreferences(devTools: boolean): WebPreferences {
  return {
    preload: join(__dirname, "../preload/index.js"),
    contextIsolation: true,
    sandbox: true,
    nodeIntegration: false,
    nodeIntegrationInWorker: false,
    nodeIntegrationInSubFrames: false,
    webviewTag: false,
    spellcheck: false,
    webSecurity: true,
    allowRunningInsecureContent: false,
    navigateOnDragDrop: false,
    safeDialogs: true,
    devTools,
  };
}

const BACKGROUND = "#F7F8FA";
/** The window buttons, centered in the sidebar's top, level with the UI's 48px top bar. */
const TRAFFIC_LIGHTS = { x: 16, y: 18 } as const;

export class AppWindow {
  private entry: WindowEntry | null = null;
  /** Shell events for the window while its UI hasn't loaded yet, sent once it has. */
  private queued: ShellEvent[] = [];
  /** Where the window was when it last closed, so it opens there again. */
  private lastPlacement: WindowPlacement | null = null;
  private nextId = 1;

  constructor(
    private readonly ui: UiSource,
    private readonly hooks: WindowHooks,
    private readonly devTools: boolean,
  ) {}

  /**
   * The window: the open one, or a new one at `placement` (moved onto a visible screen if its
   * display is gone), else where it was when it last closed.
   */
  open(placement?: WindowPlacement): WindowEntry {
    if (this.entry) {
      return this.entry;
    }
    const at = placement ?? this.lastPlacement;
    const window = new BrowserWindow({
      width: 1280,
      height: 820,
      minWidth: 760,
      minHeight: 480,
      ...(at ? visibleBounds(at.bounds) : {}),
      show: false,
      title: "Unfold",
      backgroundColor: BACKGROUND,
      titleBarStyle: "hidden",
      trafficLightPosition: TRAFFIC_LIGHTS,
      webPreferences: hardenedPreferences(this.devTools),
    });

    const entry: WindowEntry = { id: this.nextId++, window, loaded: false };
    this.entry = entry;

    window.once("ready-to-show", () => {
      if (at?.maximized) {
        window.maximize();
      }
      window.show();
    });
    const placementChanged = (): void => {
      this.hooks.onPlacementChanged(entry);
    };
    window.on("move", placementChanged);
    window.on("resize", placementChanged);
    window.on("maximize", placementChanged);
    window.on("unmaximize", placementChanged);
    window.webContents.on("did-start-loading", () => {
      entry.loaded = false;
    });
    window.webContents.on("did-finish-load", () => {
      entry.loaded = true;
      for (const event of this.queued.splice(0)) {
        this.send(entry, event);
      }
      this.hooks.onLoaded(entry);
    });
    window.on("close", () => {
      this.lastPlacement = this.placement(entry);
      this.hooks.onClosing(entry);
    });
    window.on("closed", () => {
      this.entry = null;
      this.queued = [];
      this.hooks.onClosed(entry);
    });

    void window.loadURL(entryUrl(this.ui));
    return entry;
  }

  /** The open window, if any. */
  current(): WindowEntry | undefined {
    return this.entry ?? undefined;
  }

  /** The open window, if it has id `id`. */
  get(id: number): WindowEntry | undefined {
    return this.entry !== null && this.entry.id === id ? this.entry : undefined;
  }

  /** An id from the window's sequence for a window it doesn't track (the render window), so engine window ids never collide. */
  reserveId(): number {
    return this.nextId++;
  }

  forWebContents(contents: WebContents): WindowEntry | undefined {
    const entry = this.entry;
    return entry && !entry.window.isDestroyed() && entry.window.webContents === contents ? entry : undefined;
  }

  forWindow(window: BaseWindow | undefined): WindowEntry | undefined {
    return this.entry !== null && this.entry.window === window ? this.entry : undefined;
  }

  /** Where the window is: its normal (unmaximized) bounds. */
  placement(entry: WindowEntry): WindowPlacement {
    return { bounds: entry.window.getNormalBounds(), maximized: entry.window.isMaximized() };
  }

  /** Sends a shell event (a menu command) to the window's UI. */
  send(entry: WindowEntry, event: ShellEvent): void {
    if (!entry.window.isDestroyed()) {
      entry.window.webContents.send(IPC.shellEvent, event);
    }
  }

  /** Sends a shell event that must not be lost: held until the window's UI has loaded. */
  deliver(entry: WindowEntry, event: ShellEvent): void {
    if (entry.loaded) {
      this.send(entry, event);
    } else if (entry === this.entry) {
      this.queued.push(event);
    }
  }

  focus(entry: WindowEntry): void {
    if (entry.window.isMinimized()) {
      entry.window.restore();
    }
    entry.window.show();
    entry.window.focus();
  }
}

/** Saved bounds, moved onto a visible screen if their display is gone. */
function visibleBounds(bounds: Rect): Rect {
  const primary = screen.getPrimaryDisplay();
  const others = screen.getAllDisplays().filter((display) => display.id !== primary.id);
  return placeOnScreen(bounds, [primary.workArea, ...others.map((display) => display.workArea)]);
}
