/**
 * The workspace: its projects as every window shows them, and this window's selection.
 *
 * The `workspace` query is replaced by `workspace_changed` and patched by `project_state`, so the
 * project index below changes identity exactly when the workspace does.
 */

import { call } from "../ipc/commands";
import { queryClient } from "../ipc/client";
import type { ProjectId, ProjectRow, WorkspaceView } from "../ipc/generated";
import { engineKey, useEngineQuery } from "../ipc/queries";
import { windowCommand } from "../ipc/unfold";
import { useWindowStore } from "./window";

const WORKSPACE_KEY = engineKey("workspace", {});

/** The workspace, kept current. */
export function useWorkspace(): WorkspaceView | undefined {
  return useEngineQuery("workspace", {}).data;
}

/** The workspace from the cache, outside React. */
function getWorkspace(): WorkspaceView | undefined {
  return queryClient.getQueryData<WorkspaceView>(WORKSPACE_KEY);
}

/** The projects by id. */
export interface ProjectIndex {
  readonly rows: readonly ProjectRow[];
  readonly byId: ReadonlyMap<ProjectId, ProjectRow>;
}

const EMPTY_INDEX: ProjectIndex = { rows: [], byId: new Map() };
const indexes = new WeakMap<WorkspaceView, ProjectIndex>();

/** The index of a workspace, built once per workspace object. */
export function projectIndex(workspace: WorkspaceView | undefined): ProjectIndex {
  if (!workspace) {
    return EMPTY_INDEX;
  }
  let index = indexes.get(workspace);
  if (!index) {
    index = { rows: workspace.projects, byId: new Map(workspace.projects.map((row) => [row.id, row])) };
    indexes.set(workspace, index);
  }
  return index;
}

/** The project index, kept current. */
function useProjectIndex(): ProjectIndex {
  return projectIndex(useWorkspace());
}

/** The project index from the cache, outside React. */
export function getProjectIndex(): ProjectIndex {
  return projectIndex(getWorkspace());
}

/** One project's row, kept current; undefined while loading or when it isn't in the workspace. */
export function useProjectRow(id: ProjectId | null): ProjectRow | undefined {
  const index = useProjectIndex();
  return id === null ? undefined : index.byId.get(id);
}

/**
 * Shows a project on this window's Home: `select_project` tells Rust, which remembers the selection and
 * keeps that project live. A failed call only loses liveness, which the
 * next query wakes anyway.
 */
export async function selectProject(id: ProjectId | null): Promise<void> {
  useWindowStore.setState({ selectedProject: id });
  try {
    await call("select_project", { project: id });
  } catch (error) {
    console.warn("Selecting the project failed:", error);
  }
}

/** Shows the folder picker and adds the chosen folder as a project, then selects it. Null when cancelled. */
export async function addProjectFromPicker(): Promise<ProjectId | null> {
  const added = await windowCommand("add_project");
  if (!added) {
    return null;
  }
  await selectProject(added.project);
  return added.project;
}

/** Expands or collapses a project's sidebar row (`workspace_changed` brings the new rows). */
export async function setProjectExpanded(id: ProjectId, expanded: boolean): Promise<void> {
  await call("set_project_expanded", { project: id, expanded });
}
