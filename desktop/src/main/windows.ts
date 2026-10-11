/**
 * The window factory and registry. Every window gets a numeric id (used on the wire to the engine)
 * and a stable key (its remembered state in `windows.json` survives restarts). Every window shows
 * the whole workspace (architecture §11.10, U-37).
 */

import { randomUUID } from "node:crypto";
import { join } from "node:path";
import type { BaseWindow, Rectangle, WebContents, WebPreferences } from "electron";
import { BrowserWindow, nativeTheme, screen } from "electron";
import type { ShellEvent } from "../shared/unfold-api";
import { IPC } from "../shared/transport";
import { entryUrl, type UiSource } from "./app-url";
import type { Rect, RestoredWindow } from "./window-placement";
import { debouncedSave, placeOnScreen } from "./window-placement";

export interface WindowEntry {
  readonly id: number;
  readonly window: BrowserWindow;
  /** The stable key the window's state is remembered by in `windows.json`. */
  readonly key: string;
  /** True once the UI has loaded (and after each reload); only then can it receive an engine port. */
  loaded: boolean;
}

/** A window's position and size, and whether it is maximized, as `save_window_bounds` takes them. */
export interface WindowPlacement {
  readonly bounds: Rectangle;
  readonly maximized: boolean;
}

/** What the rest of the main process wants to hear about windows. */
export interface WindowHooks {
  /** The UI finished loading (first load or a reload): hand it a fresh engine port. */
  onLoaded(entry: WindowEntry): void;
  /** The window closed. */
  onClosed(entry: WindowEntry): void;
  /** The window gained focus: it is the one used last. */
  onFocused(entry: WindowEntry): void;
  /**
   * The window was created, or the person moved, resized, maximized, or restored it (debounced), so
   * it comes back where it is.
   */
  onPlacementChanged(entry: WindowEntry): void;
}

/**
 * Every window's web preferences (architecture §13): the preload and nothing else, context isolation,
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

const CASCADE_OFFSET = 24;
const PLACEMENT_DEBOUNCE_MS = 500;
const BACKGROUND = { light: "#F7F8FA", dark: "#15161A" } as const;

export class WindowRegistry {
  private readonly entries = new Map<number, WindowEntry>();
  /** Shell events for windows whose UI hasn't loaded yet, sent once it has. */
  private readonly queued = new Map<number, ShellEvent[]>();
  private nextId = 1;

  constructor(
    private readonly ui: UiSource,
    private readonly hooks: WindowHooks,
    private readonly devTools: boolean,
  ) {}

  /**
   * Opens a window on the workspace. A restored window comes back with its key and bounds (moved
   * onto a visible screen if its display is gone); a new one is cascaded from the focused window.
   */
  create(restore?: RestoredWindow): WindowEntry {
    const near = BrowserWindow.getFocusedWindow();
    const position = near ? near.getPosition() : undefined;
    const bounds = restore?.bounds ? visibleBounds(restore.bounds) : undefined;
    const window = new BrowserWindow({
      width: 1280,
      height: 820,
      minWidth: 760,
      minHeight: 480,
      ...(position ? { x: (position[0] ?? 0) + CASCADE_OFFSET, y: (position[1] ?? 0) + CASCADE_OFFSET } : {}),
      ...(bounds ?? {}),
      show: false,
      title: "Unfold",
      backgroundColor: nativeTheme.shouldUseDarkColors ? BACKGROUND.dark : BACKGROUND.light,
      ...(process.platform === "darwin"
        ? { titleBarStyle: "hiddenInset" as const, trafficLightPosition: { x: 14, y: 13 } }
        : {}),
      webPreferences: hardenedPreferences(this.devTools),
    });

    const entry: WindowEntry = { id: this.nextId++, window, key: restore?.key ?? randomUUID(), loaded: false };
    this.entries.set(entry.id, entry);

    window.once("ready-to-show", () => {
      if (restore?.maximized) {
        window.maximize();
      }
      window.show();
    });
    const placement = debouncedSave(() => {
      if (!window.isDestroyed()) {
        this.hooks.onPlacementChanged(entry);
      }
    }, PLACEMENT_DEBOUNCE_MS);
    const placementChanged = (): void => {
      placement.schedule();
    };
    window.on("move", placementChanged);
    window.on("resize", placementChanged);
    window.on("maximize", placementChanged);
    window.on("unmaximize", placementChanged);
    window.on("focus", () => {
      this.hooks.onFocused(entry);
    });
    window.webContents.on("did-start-loading", () => {
      entry.loaded = false;
    });
    window.webContents.on("did-finish-load", () => {
      entry.loaded = true;
      for (const event of this.queued.get(entry.id) ?? []) {
        this.send(entry, event);
      }
      this.queued.delete(entry.id);
      this.hooks.onLoaded(entry);
    });
    // Save a move or resize still waiting out the debounce while the window can still report its
    // bounds: it is sent before `onClosed`, so it reaches the engine before the window detaches.
    window.on("close", () => {
      placement.flush();
    });
    window.on("closed", () => {
      placement.cancel();
      this.entries.delete(entry.id);
      this.queued.delete(entry.id);
      this.hooks.onClosed(entry);
    });

    void window.loadURL(entryUrl(this.ui));
    // Remember the window from the start, before it is ever moved.
    this.hooks.onPlacementChanged(entry);
    return entry;
  }

  get(id: number): WindowEntry | undefined {
    return this.entries.get(id);
  }

  /** An id from the windows' sequence for a window the registry doesn't list (the render window), so engine window ids never collide. */
  reserveId(): number {
    return this.nextId++;
  }

  all(): WindowEntry[] {
    return [...this.entries.values()];
  }

  forWebContents(contents: WebContents): WindowEntry | undefined {
    return this.all().find((entry) => !entry.window.isDestroyed() && entry.window.webContents === contents);
  }

  forWindow(window: BaseWindow): WindowEntry | undefined {
    return this.all().find((entry) => entry.window === window);
  }

  focused(): WindowEntry | undefined {
    const window = BrowserWindow.getFocusedWindow();
    return window ? this.all().find((entry) => entry.window === window) : undefined;
  }

  /** Where the window is, for `save_window_bounds`: its normal (unmaximized) bounds. */
  placement(entry: WindowEntry): WindowPlacement {
    return { bounds: entry.window.getNormalBounds(), maximized: entry.window.isMaximized() };
  }

  /** Sends a shell event (a menu command) to one window's UI. */
  send(entry: WindowEntry, event: ShellEvent): void {
    if (!entry.window.isDestroyed()) {
      entry.window.webContents.send(IPC.shellEvent, event);
    }
  }

  /** Sends a shell event that must not be lost: held until the window's UI has loaded. */
  deliver(entry: WindowEntry, event: ShellEvent): void {
    if (entry.loaded) {
      this.send(entry, event);
    } else {
      this.queued.set(entry.id, [...(this.queued.get(entry.id) ?? []), event]);
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
function visibleBounds(bounds: Rect): Rectangle {
  const primary = screen.getPrimaryDisplay();
  const others = screen.getAllDisplays().filter((display) => display.id !== primary.id);
  return placeOnScreen(bounds, [primary.workArea, ...others.map((display) => display.workArea)]);
}
