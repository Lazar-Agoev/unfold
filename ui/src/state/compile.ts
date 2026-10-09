/**
 * Compile in this window: the running Compile (or Rebuild from scratch) started here, its streamed
 * progress, and progress of compiles and background updates started elsewhere (`compile_progress`
 * events). The graph's state itself comes from `graph_status`.
 */

import { create } from "zustand";
import { call, callStream, onEngineEvent } from "../ipc/commands";
import type { CompileMode, CompileOutcome, CompileProgress } from "../ipc/generated";
import { toUnfoldError, type UnfoldError } from "../ipc/unfold";

export interface CompileState {
  /** The Compile this window started, while it runs. */
  readonly running: { readonly mode: CompileMode; readonly cancel: () => void } | null;
  /** The latest progress report (from this window's stream or any `compile_progress` event). */
  readonly progress: CompileProgress | null;
  /** How this window's last Compile ended. */
  readonly outcome: CompileOutcome | null;
  readonly error: UnfoldError | null;
}

export const useCompileState = create<CompileState>()(() => ({
  running: null,
  progress: null,
  outcome: null,
  error: null,
}));

/**
 * Starts a Compile (`check`) or Rebuild from scratch. Does nothing while one this window started is
 * running. Resolves when it ends; a cancelled compile is not an error.
 */
export async function startCompile(mode: CompileMode = "check"): Promise<void> {
  if (useCompileState.getState().running) {
    return;
  }
  const handle = callStream("compile", { mode }, (progress) => {
    useCompileState.setState({ progress });
  });
  useCompileState.setState({ running: { mode, cancel: handle.cancel }, progress: null, error: null });
  try {
    const outcome = await handle.result;
    useCompileState.setState({ running: null, progress: null, outcome });
  } catch (raw) {
    const error = toUnfoldError(raw);
    useCompileState.setState({ running: null, progress: null, error: error.code === "cancelled" ? null : error });
    if (error.code !== "cancelled") {
      throw error;
    }
  }
}

/** Cancels this window's Compile and any running or queued one (another window's, too). */
export async function cancelCompile(): Promise<void> {
  useCompileState.getState().running?.cancel();
  await call("cancel_compile", {});
}

/** Follows progress of compiles and updates this window didn't start. Returns the unsubscribe function. */
export function followCompileProgress(): () => void {
  return onEngineEvent("compile_progress", ({ stage, done, total }) => {
    useCompileState.setState({ progress: { stage, done, total } });
  });
}

/** Forgets stale progress once the graph is no longer compiling or updating. */
export function clearCompileProgress(): void {
  if (!useCompileState.getState().running) {
    useCompileState.setState({ progress: null });
  }
}
