/**
 * The window's state and adding projects, as the main process sees them (desktop/README.md#transport-and-windows).
 *
 * - At launch, the window comes back ({@link Workspace.restore}): the main process asks the engine
 *   (trusted `window_state`) and opens the window with its bounds, moved onto a visible screen if
 *   its display is gone. The UI asks `window_info` for the board to show. With nothing saved, or no
 *   answer from the engine, the window opens at its default size on Home.
 * - The UI reports the board it shows (`report_shown_board`). The window's bounds, whether it is
 *   maximized, and that board are saved with `save_window_state` whenever one of them changes
 *   (debounced, and flushed as the window closes so the last change before closing or quitting is
 *   kept). Nothing else is remembered.
 * - A folder from File › Add Project… joins the workspace through the trusted `add_project`, the
 *   only door for an outside path, and the window is told to select it (`project_added`), or why it
 *   couldn't be added (`project_add_failed`).
 */

import { transportError } from "../shared/transport";
import type { AddedProject, ShellEvent, ShownBoard } from "../shared/unfold-api";
import type { EngineHost } from "./engine-host";
import { EngineRequestError } from "./engine-host";
import type { WindowState } from "./window-placement";
import { debouncedSave, savedWindowState } from "./window-placement";
import type { AppWindow } from "./windows";

/** How long launch waits for the saved state before opening the window at its default size. */
const RESTORE_TIMEOUT_MS = 3_000;
/** How long the window's state must stay unchanged before it is saved. */
const SAVE_DEBOUNCE_MS = 500;

/** The main-process requests this module sends. */
type EngineRequests = Pick<EngineHost, "request">;

export class Workspace {
  /** The board the window shows (null: Home): as saved at launch, then as the UI reports it. */
  private shown: ShownBoard | null = null;
  /** The latest `save_window_state` until it settles (never rejects). */
  private saving: Promise<void> = Promise.resolve();
  /** The launch restore, so a dock click or a menu command during it waits for the window. */
  private restoring: Promise<void> | null = null;
  private readonly pendingSave = debouncedSave(() => {
    this.save();
  }, SAVE_DEBOUNCE_MS);

  constructor(
    private readonly window: AppWindow,
    private readonly engine: EngineRequests,
  ) {}

  /** At launch: opens the window where it was when the app last quit, on the board it showed. */
  restore(): Promise<void> {
    this.restoring ??= this.restoreNow();
    return this.restoring;
  }

  /** Opens the window if it is closed (after the launch restore), and brings it to the front. */
  async show(): Promise<void> {
    await this.restoring;
    this.window.focus(this.window.open());
  }

  /** The board the window shows, for `window_info`. */
  shownBoard(): ShownBoard | null {
    return this.shown;
  }

  /** The UI shows another board, or Home: remember it. */
  showBoard(board: ShownBoard | null): void {
    this.shown = board;
    this.pendingSave.schedule();
  }

  /** The person moved or resized the window: remember where it is. */
  placementChanged(): void {
    this.pendingSave.schedule();
  }

  /** The window is closing: save a change still waiting out the debounce while it can report its bounds. */
  windowClosing(): void {
    this.pendingSave.flush();
  }

  /**
   * The window closed (before it is detached): resolves once its last save has settled, so the
   * detach the caller does next can't race it.
   */
  async windowClosed(): Promise<void> {
    this.pendingSave.cancel();
    await this.saving;
  }

  /**
   * Adds `path` (absolute, from the folder picker) to the workspace on behalf of window `windowId`
   * (`0` for none); the engine compiles it at once. Rejects with an {@link EngineRequestError} whose
   * message the person sees (`not_a_folder`, `too_many_projects`, a folder Unfold can't write
   * `.unfold/` into, …).
   */
  async add(windowId: number, path: string): Promise<AddedProject> {
    return addedProject(await this.engine.request(windowId, "add_project", { path }));
  }

  /** Adds a project from File › Add Project…, then tells the window to select it, or why it couldn't be added. */
  async addProject(path: string): Promise<void> {
    let event: ShellEvent;
    try {
      event = { type: "project_added", project: (await this.add(0, path)).project };
    } catch (error) {
      console.error(`[main] adding ${path} to the workspace failed: ${describe(error)}`);
      const message = error instanceof EngineRequestError ? error.error.message : describe(error);
      event = { type: "project_add_failed", path, message };
    }
    await this.restoring;
    this.window.deliver(this.window.open(), event);
  }

  private async restoreNow(): Promise<void> {
    let saved: WindowState | null = null;
    try {
      saved = savedWindowState(await withTimeout(this.engine.request(0, "window_state", {}), RESTORE_TIMEOUT_MS));
    } catch (error) {
      console.warn(`[main] could not read the window's state: ${describe(error)}`);
    }
    if (this.window.current()) {
      return; // An agent needed the window before the saved state arrived.
    }
    this.shown = saved?.board ?? null;
    this.window.open(saved ?? undefined);
  }

  /** Saves the window's bounds and the board it shows, so it comes back there. */
  private save(): void {
    const entry = this.window.current();
    if (!entry || entry.window.isDestroyed()) {
      return;
    }
    const state: WindowState = { ...this.window.placement(entry), board: this.shown };
    this.saving = this.engine.request(entry.id, "save_window_state", { state }).then(
      () => undefined,
      (error: unknown) => {
        // Expected while the engine restarts; the next change saves.
        console.warn(`[main] could not save the window's state: ${describe(error)}`);
      },
    );
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
