/**
 * Opening folders in windows, and windows coming back (architecture §11.2).
 *
 * - Opening a folder that a window already shows focuses that window.
 * - Otherwise the main process asks the engine (trusted) for the folder's saved windows
 *   (`folder_windows`) and recreates each one with its key and bounds, moved onto a visible screen
 *   if its display is gone. The UI then restores each window's tabs and layout with `window_state`.
 * - Bounds are saved with `save_window_bounds` when a window attaches and whenever it is moved or
 *   resized (debounced by the registry, and flushed as the window closes so the last move before
 *   closing or quitting is kept).
 * - Close Folder ({@link Folders.close}) returns a window to the welcome screen.
 * - A window the person closes, switches to another folder, or closes the folder of, is forgotten (`forget_window_state`)
 *   unless it was the folder's last window: the last window of a folder, and every window open when
 *   the app quits, comes back.
 */

import { realpathSync, statSync } from "node:fs";
import { isAbsolute } from "node:path";
import type { EngineHost } from "./engine-host";
import type { Rect, RestoredWindow } from "./window-placement";
import { rectangle, savedWindowList } from "./window-placement";
import type { WindowEntry, WindowRegistry } from "./windows";
import { newWindowKey } from "./windows";

/** How long opening a folder waits for its saved windows before opening one fresh window. */
const RESTORE_TIMEOUT_MS = 3_000;
/** At most this many windows come back for one folder. */
const MAX_RESTORED_WINDOWS = 16;

/** The main-process requests this module sends. */
export type EngineRequests = Pick<EngineHost, "request">;

export class Folders {
  private readonly opening = new Map<string, Promise<WindowEntry>>();
  /** Each window's latest `save_window_bounds` until it settles (never rejects). */
  private readonly placementSaves = new Map<number, Promise<void>>();
  private quitting = false;

  constructor(
    private readonly registry: WindowRegistry,
    private readonly engine: EngineRequests,
  ) {}

  /**
   * Opens `folder`: focuses a window already showing it, or brings back its saved windows (the
   * first one in `target`, if given) or one new window. Resolves with the window that shows it.
   */
  open(folder: string, target?: WindowEntry): Promise<WindowEntry> {
    const path = canonicalFolder(folder);
    const pending = this.opening.get(path);
    if (pending) {
      return pending;
    }
    const opened = this.openNow(path, target).finally(() => {
      this.opening.delete(path);
    });
    this.opening.set(path, opened);
    return opened;
  }

  /**
   * Closes the folder `entry` shows: the window switches to the welcome screen (re-attached to the
   * engine host with no folder, which detaches it from the folder's workspace; the folder's last
   * window closes it). Other windows on the folder keep it. Like switching folders, the window's
   * state in the folder is forgotten unless it was the folder's last window. No-op on welcome.
   */
  async close(entry: WindowEntry): Promise<void> {
    if (entry.folder === null) {
      return;
    }
    await this.forgetUnlessLast(entry);
    if (!entry.window.isDestroyed()) {
      this.registry.setFolder(entry, null);
    }
  }

  /** Opens a welcome window where the last one was. */
  async openWelcome(): Promise<WindowEntry> {
    const bounds = await this.welcomeBounds();
    return this.registry.create(null, bounds ? { key: newWindowKey(), bounds, maximized: false } : undefined);
  }

  /** The app is quitting: windows closing from now on are remembered, not forgotten. */
  setQuitting(): void {
    this.quitting = true;
  }

  /** A window closed (it is still attached in the engine): forget it unless it should come back. */
  async windowClosed(entry: WindowEntry): Promise<void> {
    // The bounds flushed as it closed land first, so forgetting the window (or detaching it, which
    // the caller does next) can't race them.
    await this.placementSaves.get(entry.id);
    if (!this.quitting) {
      await this.forgetUnlessLast(entry);
    }
  }

  /** Saves a window's position and size: in its folder's state, or the welcome window's. */
  savePlacement(entry: WindowEntry): void {
    if (entry.window.isDestroyed()) {
      return;
    }
    const { bounds, maximized } = this.registry.placement(entry);
    const args = entry.folder === null ? { bounds, maximized } : { key: entry.key, bounds, maximized };
    const saved = this.engine.request(entry.id, "save_window_bounds", args).then(
      () => undefined,
      (error: unknown) => {
        // Expected while the window is between folders or the engine restarts; the next move saves.
        console.warn(`[main] could not save the bounds of window ${entry.id}: ${describe(error)}`);
      },
    );
    this.placementSaves.set(entry.id, saved);
    void saved.then(() => {
      if (this.placementSaves.get(entry.id) === saved) {
        this.placementSaves.delete(entry.id);
      }
    });
  }

  private async openNow(folder: string, target: WindowEntry | undefined): Promise<WindowEntry> {
    const existing = this.registry.showing(folder);
    if (existing) {
      this.registry.focus(existing);
      return existing;
    }
    const [first, ...rest] = await this.savedWindows(folder);
    // A window opened while the saved windows were read (a double click) wins.
    const raced = this.registry.showing(folder);
    if (raced) {
      this.registry.focus(raced);
      return raced;
    }
    for (const window of rest) {
      this.registry.create(folder, window);
    }
    if (target && !target.window.isDestroyed()) {
      await this.forgetUnlessLast(target);
      this.registry.setFolder(target, folder, first);
      return target;
    }
    return this.registry.create(folder, first);
  }

  /** The folder's saved windows that aren't open already; none if the engine can't say in time. */
  private async savedWindows(folder: string): Promise<RestoredWindow[]> {
    let saved: unknown;
    try {
      saved = await withTimeout(this.engine.request(0, "folder_windows", { root: folder }), RESTORE_TIMEOUT_MS);
    } catch (error) {
      console.warn(`[main] could not read the saved windows of ${folder}: ${describe(error)}`);
      return [];
    }
    const inUse = new Set(this.registry.all().map((entry) => entry.key));
    return savedWindowList(saved)
      .filter((window) => !inUse.has(window.key))
      .slice(0, MAX_RESTORED_WINDOWS);
  }

  private async welcomeBounds(): Promise<Rect | null> {
    try {
      const recent = await withTimeout(this.engine.request(0, "recent_folders", {}), RESTORE_TIMEOUT_MS);
      return isRecord(recent) ? rectangle(recent["welcomeBounds"]) : null;
    } catch {
      return null; // The engine is unavailable: open at the default place.
    }
  }

  /** Forgets `entry`'s state in its folder unless no other window shows that folder. */
  private async forgetUnlessLast(entry: WindowEntry): Promise<void> {
    const folder = entry.folder;
    if (folder === null || !this.registry.all().some((other) => other !== entry && other.folder === folder)) {
      return;
    }
    try {
      await this.engine.request(entry.id, "forget_window_state", { key: entry.key });
    } catch (error) {
      console.warn(`[main] could not forget window ${entry.id} of ${folder}: ${describe(error)}`);
    }
  }
}

/** A folder's canonical path (as Rust reports it), or the path itself if it can't be resolved. */
export function canonicalFolder(folder: string): string {
  try {
    return realpathSync(folder);
  } catch {
    return folder;
  }
}

/** Whether `path` is an absolute path to an existing directory. */
export function isDirectoryPath(path: string): boolean {
  if (!isAbsolute(path)) {
    return false;
  }
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`no answer within ${ms} ms`));
    }, ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error instanceof Error ? error : new Error(String(error)));
      },
    );
  });
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
