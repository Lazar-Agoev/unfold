/**
 * The workspace's windows and projects, as the main process sees them (architecture §11.2).
 *
 * - At launch, the windows open when the app last quit come back ({@link Workspace.restore}): the
 *   main process asks the engine (trusted `saved_windows`) and recreates each with its key and
 *   bounds, moved onto a visible screen if its display is gone. The UI then restores each window's
 *   tabs and layout with `window_state`. With none saved, or no answer from the engine, one new
 *   window opens.
 * - A folder from the menu, the command line, the OS (`open-file`), a second launch, or a drop joins
 *   the workspace through the trusted `add_project`, the only door for an outside path. The menu,
 *   command line, OS, and second launch also tell the focused window to select it (`project_added`),
 *   or why it couldn't be added (`project_add_failed`).
 * - Bounds are saved with `save_window_bounds` when a window is created and whenever it is moved or
 *   resized (debounced by the registry, and flushed as the window closes so the last move before
 *   closing or quitting is kept).
 * - A window the person closes is forgotten (`forget_window`) unless it was the last one: the last
 *   window, and every window open when the app quits, comes back.
 */

import { transportError } from "../shared/transport";
import type { AddedProject, ShellEvent } from "../shared/unfold-api";
import type { EngineHost } from "./engine-host";
import { EngineRequestError } from "./engine-host";
import { savedWindowList } from "./window-placement";
import type { WindowEntry, WindowRegistry } from "./windows";

/** How long launch waits for the saved windows before opening one fresh window. */
const RESTORE_TIMEOUT_MS = 3_000;
/** At most this many windows come back. */
const MAX_RESTORED_WINDOWS = 16;

/** The main-process requests this module sends. */
type EngineRequests = Pick<EngineHost, "request">;

export class Workspace {
  /** Each window's latest `save_window_bounds` until it settles (never rejects). */
  private readonly placementSaves = new Map<number, Promise<void>>();
  /** The restore in progress, so overlapping launches and activations bring windows back once. */
  private restoring: Promise<void> | null = null;
  private quitting = false;

  constructor(
    private readonly registry: WindowRegistry,
    private readonly engine: EngineRequests,
  ) {}

  /** Brings back the saved windows that aren't open already, or opens one window if none come back. */
  restore(): Promise<void> {
    this.restoring ??= this.restoreNow().finally(() => {
      this.restoring = null;
    });
    return this.restoring;
  }

  /**
   * Adds `path` (absolute, from a dialog, a drop, or the command line) to the workspace on behalf of
   * window `windowId` (`0` for none). Rejects with an {@link EngineRequestError} (`not_a_folder`,
   * `too_many_projects`, …).
   */
  async add(windowId: number, path: string): Promise<AddedProject> {
    return addedProject(await this.engine.request(windowId, "add_project", { path }));
  }

  /**
   * Adds a project from the menu, the command line, the OS, or a second launch, then tells the
   * focused window (else the first one, else a new one) to select it, or why it couldn't be added.
   */
  async addProject(path: string): Promise<void> {
    let event: ShellEvent;
    try {
      event = { type: "project_added", project: (await this.add(0, path)).project };
    } catch (error) {
      console.error(`[main] adding ${path} to the workspace failed: ${describe(error)}`);
      const message = error instanceof EngineRequestError ? error.error.message : describe(error);
      event = { type: "project_add_failed", path, message };
    }
    // At launch, the windows coming back receive it rather than a window of their own.
    await this.restoring;
    const target = this.registry.focused() ?? this.registry.all()[0] ?? this.registry.create();
    this.registry.deliver(target, event);
  }

  /** The app is quitting: windows closing from now on are remembered, not forgotten. */
  setQuitting(): void {
    this.quitting = true;
  }

  /** A window closed (before it is detached): forget it unless it should come back. */
  async windowClosed(entry: WindowEntry): Promise<void> {
    // The bounds flushed as it closed land first, so forgetting the window (or detaching it, which
    // the caller does next) can't race them.
    await this.placementSaves.get(entry.id);
    if (this.quitting || this.registry.all().length === 0) {
      return;
    }
    try {
      await this.engine.request(entry.id, "forget_window", { key: entry.key });
    } catch (error) {
      console.warn(`[main] could not forget window ${entry.id}: ${describe(error)}`);
    }
  }

  /** Saves a window's position and size, so it comes back there. */
  savePlacement(entry: WindowEntry): void {
    if (entry.window.isDestroyed()) {
      return;
    }
    const { bounds, maximized } = this.registry.placement(entry);
    const saved = this.engine.request(entry.id, "save_window_bounds", { key: entry.key, bounds, maximized }).then(
      () => undefined,
      (error: unknown) => {
        // Expected while the engine restarts; the next move saves.
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

  private async restoreNow(): Promise<void> {
    let saved: unknown = null;
    try {
      saved = await withTimeout(this.engine.request(0, "saved_windows", {}), RESTORE_TIMEOUT_MS);
    } catch (error) {
      console.warn(`[main] could not read the saved windows: ${describe(error)}`);
    }
    const inUse = new Set(this.registry.all().map((entry) => entry.key));
    const windows = savedWindowList(saved)
      .filter((window) => !inUse.has(window.key))
      .slice(0, MAX_RESTORED_WINDOWS);
    for (const window of windows) {
      this.registry.create(window);
    }
    if (this.registry.all().length === 0) {
      this.registry.create();
    }
  }
}

/** An `add_project` result as the window commands report it: the row's id, not the row. */
function addedProject(result: unknown): AddedProject {
  if (isRecord(result) && (result["status"] === "added" || result["status"] === "exists")) {
    const project = result["project"];
    if (isRecord(project) && typeof project["id"] === "string") {
      return { status: result["status"], project: project["id"] };
    }
  }
  throw new EngineRequestError(transportError("internal", "The engine's add_project result is malformed."));
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
