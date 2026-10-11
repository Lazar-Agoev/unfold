/**
 * The wire protocol between the main process, the engine host, and each window's preload script.
 *
 * - renderer -> host, over the window's MessagePort: {@link RendererToHost}
 * - host -> renderer, over the same port: {@link HostToRenderer}
 * - main <-> host, over the utility process's parent port: {@link MainToHost} and
 *   {@link HostToMain} (lifecycle, the main process's own trusted requests: adding a project from a
 *   dialog, a drop, or the command line, export to a path from a save dialog, and window restore;
 *   the MCP server's untrusted requests for agents; and the events the main process follows)
 * - main <-> preload, over Electron IPC: the {@link IPC} channels
 */

import type { ConnectionState, TransportError, TransportErrorCode, UnfoldEvent } from "./unfold-api";

/** A request from a window to the engine host. `id` is unique per window connection. */
export type RendererToHost =
  | { readonly kind: "invoke"; readonly id: number; readonly command: string; readonly args: unknown }
  | { readonly kind: "cancel"; readonly id: number };

/** A message from the engine host to one window. */
export type HostToRenderer =
  | { readonly kind: "result"; readonly id: number; readonly ok: true; readonly value: unknown }
  | { readonly kind: "result"; readonly id: number; readonly ok: false; readonly error: TransportError }
  | { readonly kind: "stream"; readonly id: number; readonly message: unknown }
  | { readonly kind: "event"; readonly event: UnfoldEvent }
  /**
   * Sent once per attach, before anything else on that port: whether the engine (the Rust addon)
   * can serve this window. The preload turns it into the window's {@link ConnectionState}.
   */
  | {
      readonly kind: "status";
      readonly state: Extract<ConnectionState, "connected" | "unavailable">;
      readonly error?: TransportError;
    };

/** App folders (architecture §8.2), chosen by the main process and handed to `addon.init`. */
export interface AppFolders {
  readonly appDataDir: string;
  readonly logsDir: string;
  /** Parent of the per-folder fallback caches (`<fallbackCacheDir>/<hash of the path>/`). */
  readonly fallbackCacheDir: string;
}

/** Messages from the main process to the engine host: lifecycle, and its own trusted requests. */
export type MainToHost =
  /** First message after every (re)start. `addonPath` is the module to `require`. */
  | { readonly type: "init"; readonly addonPath: string; readonly folders: AppFolders }
  /**
   * Attach a window (its port is transferred with this message). Its commands dispatch at once:
   * every window shows the whole workspace. A re-attach of the same id (a reload) swaps the port.
   */
  | { readonly type: "attach"; readonly windowId: number }
  /** The window closed: its closing writes land, then Rust releases its claims and selection. */
  | { readonly type: "detach"; readonly windowId: number }
  /**
   * A command the main process itself originates, run on behalf of window `windowId` (`0` for
   * none). With `trusted`, it carries `"trusted": true`: the only way the main-process-only commands
   * (`add_project`, `export_mermaid`, `saved_windows`, `save_window_bounds`, `forget_window`) reach
   * Rust. Without it (the MCP server's requests for agents), it is refused or dispatched exactly as
   * a window's would be. Answered with a {@link HostToMain} `result` carrying the same `id`.
   */
  | {
      readonly type: "request";
      readonly id: number;
      readonly windowId: number;
      readonly command: string;
      readonly args: Readonly<Record<string, unknown>>;
      readonly trusted: boolean;
    }
  /** The app is quitting: detach every window, close every live project, then exit. */
  | { readonly type: "shutdown" };

/**
 * Messages from the engine host to the main process, over the utility process's parent port: the
 * outcome of a main-process {@link MainToHost} `request`, and the engine events the main process
 * follows ({@link MAIN_EVENTS}).
 */
export type HostToMain =
  | { readonly type: "result"; readonly id: number; readonly ok: true; readonly value: unknown }
  | { readonly type: "result"; readonly id: number; readonly ok: false; readonly error: TransportError }
  | { readonly type: "event"; readonly event: UnfoldEvent };

/**
 * Engine events the engine host also sends the main process: `preferences_changed`, so it starts or
 * stops the agents' server when Preferences › Agents changes.
 */
export const MAIN_EVENTS: ReadonlySet<string> = new Set(["preferences_changed"]);

/** Electron IPC channels between the main process and the preload script. */
export const IPC = {
  /** main -> preload: a fresh engine port (transferred). Every port arrival starts a new connection. */
  enginePort: "unfold:engine-port",
  /** main -> preload: the engine host stopped. Payload {@link EngineLost}. */
  engineLost: "unfold:engine-lost",
  /** main -> preload: a shell event (menu command, project added, …). Payload is a `ShellEvent`. */
  shellEvent: "unfold:shell-event",
  /** preload -> main (invoke): a window command. Reply is a {@link WindowReply}. */
  windowCommand: "unfold:window-command",
} as const;

/**
 * Window commands only the preload script sends, on the person's behalf, over
 * {@link IPC.windowCommand}; `window.unfold.window(...)` refuses them.
 */
export const PRELOAD_COMMANDS = {
  /** `{path}`: a folder dropped on the window, added as a project (the path comes from the drop's `File`). */
  addDroppedFolder: "add_dropped_folder",
} as const;

/** Payload of {@link IPC.engineLost}. `restarting: false` means the main process gave up. */
export interface EngineLost {
  readonly restarting: boolean;
}

/** The main process's reply to a window command. Errors travel as data so their code survives IPC. */
export type WindowReply =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: TransportError };

/** Builds a {@link TransportError} with a transport-level code. */
export function transportError(code: TransportErrorCode, message: string): TransportError {
  return { code, message };
}
