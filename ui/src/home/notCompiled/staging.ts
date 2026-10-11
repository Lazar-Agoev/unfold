/**
 * First-Compile scope changes, kept in memory per project until Compile writes them: nothing is
 * written to `unfold.toml` before the person compiles.
 */

import { create } from "zustand";
import { call, engineKey, queryClient, type ProjectId, type ScopeFolder } from "../../ipc";
import { startCompile } from "../../state/compile";
import { savedChoice, scopeEdits, type StagedScope } from "./model";

const useStaging = create<Readonly<Record<ProjectId, StagedScope>>>()(() => ({}));

const NONE: StagedScope = {};

export function useStagedScope(project: ProjectId): StagedScope {
  return useStaging((state) => state[project] ?? NONE);
}

/** Checks or unchecks a folder; a change back to the saved choice is forgotten. */
export function stageFolder(project: ProjectId, folder: ScopeFolder, checked: boolean): void {
  useStaging.setState((state) => {
    const rest = Object.entries(state[project] ?? NONE).filter(([path]) => path !== folder.path);
    // A change back to the saved choice is no change.
    const staged = (savedChoice(folder) === "checked") === checked ? rest : [...rest, [folder.path, checked] as const];
    return { ...state, [project]: Object.fromEntries(staged) };
  });
}

/** Forgets the staged changes (after they are written, or in tests). */
export function clearStagedScope(project?: ProjectId): void {
  useStaging.setState((state) =>
    project === undefined ? {} : Object.fromEntries(Object.entries(state).filter(([key]) => key !== project)),
  );
}

/**
 * Compile from the not-compiled Home: writes the staged scope (all or nothing) and adopts the returned
 * config, then compiles. A failed write leaves the changes staged and compiles nothing.
 */
export async function compileWithScope(project: ProjectId, folders: readonly ScopeFolder[]): Promise<void> {
  const edits = scopeEdits(folders, useStaging.getState()[project] ?? NONE);
  if (edits.length > 0) {
    const config = await call("update_project_config", { project, edits });
    queryClient.setQueryData(engineKey("project_config", { project }), config);
    clearStagedScope(project);
  }
  await startCompile(project);
}
