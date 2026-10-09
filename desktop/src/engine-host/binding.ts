/**
 * The engine host's view of the Rust addon (`unfold-node`). This mirrors the addon's exports exactly;
 * the addon has no other surface (architecture §12, U-25).
 */

import { createRequire } from "node:module";

export interface EngineBinding {
  /** Once, at engine-host start. JSON `{"appDataDir","logsDir","fallbackCacheDir"}`. */
  init(options: string): void;
  /** Register the single sink; Rust calls it from any thread with one JSON message string. */
  subscribe(sink: (message: string) => void): void;
  /** Attach a window to a folder's shared workspace. Resolves with JSON. */
  openWorkspace(windowId: number, root: string): Promise<string>;
  /** Detach a window. The folder's last detach saves drafts and closes its workspace. */
  closeWorkspace(windowId: number): Promise<void>;
  /** Run one command for one window. Rejects with an Error whose message is JSON `{code, message}`. */
  dispatch(windowId: number, requestId: number, command: string): Promise<string>;
  /** Cancel a running command or stream (no-op if finished). */
  cancel(requestId: number): void;
}

const BINDING_FUNCTIONS = [
  "init",
  "subscribe",
  "openWorkspace",
  "closeWorkspace",
  "dispatch",
  "cancel",
] as const satisfies readonly (keyof EngineBinding)[];

/** Checks that a loaded module exposes every addon function, and returns it typed. */
export function asEngineBinding(module: unknown): EngineBinding {
  if (typeof module !== "object" || module === null) {
    throw new Error("the addon module did not export an object");
  }
  const missing = BINDING_FUNCTIONS.filter(
    (name) => typeof (module as Record<string, unknown>)[name] !== "function",
  );
  if (missing.length > 0) {
    throw new Error(`the addon module is missing: ${missing.join(", ")}`);
  }
  return module as EngineBinding;
}

/** Loads the addon from `addonPath` (a napi-rs package directory or a `.node` file). */
export function loadEngineBinding(addonPath: string): EngineBinding {
  const require = createRequire(import.meta.url);
  return asEngineBinding(require(addonPath));
}
