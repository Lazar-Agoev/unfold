/**
 * Compile per project (UI plan §3.11): the Compiles (or Rebuilds from scratch) this window started,
 * with their streamed progress, and the progress of compiles and background updates started anywhere
 * (`compile_progress` events), until the project's state leaves compiling or updating.
 */

import { create } from "zustand";
import { call, callStream, onEngineEvent } from "../ipc/commands";
import type { CompileMode, CompileProgress, ProjectId } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";

interface CompileState {
  /** The Compile this window started, per project, while it runs. */
  readonly running: Readonly<Record<ProjectId, { readonly mode: CompileMode; readonly cancel: () => void }>>;
  /** The latest progress per project (from this window's stream or any `compile_progress` event). */
  readonly progress: Readonly<Record<ProjectId, CompileProgress>>;
}

const useCompileState = create<CompileState>()(() => ({ running: {}, progress: {} }));

function omit<T>(record: Readonly<Record<string, T>>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([entry]) => entry !== key));
}

function setProgress(project: ProjectId, progress: CompileProgress | null): void {
  useCompileState.setState((state) => ({ progress: progress ? { ...state.progress, [project]: progress } : omit(state.progress, project) }));
}

/**
 * Compiles a project (`check`) or rebuilds it from scratch. Does nothing while one this window started
 * for the project runs. Resolves when it ends; a cancelled compile is not an error.
 */
export async function startCompile(project: ProjectId, mode: CompileMode = "check"): Promise<void> {
  if (useCompileState.getState().running[project]) {
    return;
  }
  const handle = callStream("compile", { project, mode }, ({ stage, done, total }) => {
    setProgress(project, { stage, done, total });
  });
  useCompileState.setState((state) => ({ running: { ...state.running, [project]: { mode, cancel: handle.cancel } } }));
  try {
    await handle.result;
  } catch (raw) {
    const error = toUnfoldError(raw);
    if (error.code !== "cancelled") {
      throw error;
    }
  } finally {
    useCompileState.setState((state) => ({ running: omit(state.running, project) }));
  }
}

/** Cancels the project's Compile, this window's and any running or queued elsewhere. */
export async function cancelCompile(project: ProjectId): Promise<void> {
  useCompileState.getState().running[project]?.cancel();
  await call("cancel_compile", { project });
}

/** Turns a project's background updates on or off (the project's state follows by event). */
export async function setBackgroundUpdates(project: ProjectId, on: boolean): Promise<void> {
  await call("set_background_updates", { project, on });
}

/** The project's compile or update progress, kept current; null when none is reported. */
export function useCompileProgress(project: ProjectId): CompileProgress | null {
  return useCompileState((state) => state.progress[project] ?? null);
}

/** Whether a Compile this window started for the project is running (V-67: only this window says "compiled"). */
export function useCompileStartedHere(project: ProjectId): boolean {
  return useCompileState((state) => state.running[project] !== undefined);
}

/** Follows progress reported by any window, and forgets it once the project settles. Returns the uninstall function. */
export function installCompileEvents(): () => void {
  const unsubscribers = [
    onEngineEvent("compile_progress", ({ project, stage, done, total }) => {
      setProgress(project, { stage, done, total });
    }),
    onEngineEvent("project_state", ({ project, state }) => {
      if (state.kind !== "compiling" && state.kind !== "updating") {
        setProgress(project, null);
      }
    }),
  ];
  return () => {
    for (const unsubscribe of unsubscribers) {
      unsubscribe();
    }
  };
}

/** Forgets every Compile and progress report (tests: the store is per window, so per test file otherwise). */
export function resetCompile(): void {
  useCompileState.setState({ running: {}, progress: {} });
}
