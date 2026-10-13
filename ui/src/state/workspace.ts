/**
 * The workspace: its projects as every window shows them, this window's selection, and
 * how a board's cards find their project by shared name (shared project names).
 *
 * The `workspace` query is replaced by `workspace_changed` and patched by `project_state`, so the
 * project index below changes identity exactly when the workspace does, and every card resolves again
 * at once when a project is connected, renamed, or removed.
 */

import { call } from "../ipc/commands";
import { queryClient } from "../ipc/client";
import type { ProjectId, ProjectName, ProjectRow, WorkspaceView } from "../ipc/generated";
import { engineKey, useEngineQuery } from "../ipc/queries";
import { windowCommand } from "../ipc/unfold";
import { useWindowStore } from "./window";

const WORKSPACE_KEY = engineKey("workspace", {});

/** The workspace, kept current (refetched when the window gains focus: branches change outside, worktree metadata). */
export function useWorkspace(): WorkspaceView | undefined {
  return useEngineQuery("workspace", {}, { refetchOnWindowFocus: true }).data;
}

/** The workspace from the cache, outside React. */
function getWorkspace(): WorkspaceView | undefined {
  return queryClient.getQueryData<WorkspaceView>(WORKSPACE_KEY);
}

/** The projects by id and by shared name. */
export interface ProjectIndex {
  readonly rows: readonly ProjectRow[];
  readonly byId: ReadonlyMap<ProjectId, ProjectRow>;
  /** The first row in workspace order wins a shared name (shared project names). */
  readonly byName: ReadonlyMap<ProjectName, ProjectId>;
}

const EMPTY_INDEX: ProjectIndex = { rows: [], byId: new Map(), byName: new Map() };
const indexes = new WeakMap<WorkspaceView, ProjectIndex>();

/** The index of a workspace, built once per workspace object. */
export function projectIndex(workspace: WorkspaceView | undefined): ProjectIndex {
  if (!workspace) {
    return EMPTY_INDEX;
  }
  let index = indexes.get(workspace);
  if (!index) {
    const byName = new Map<ProjectName, ProjectId>();
    for (const row of workspace.projects) {
      if (!byName.has(row.name)) {
        byName.set(row.name, row.id);
      }
    }
    index = { rows: workspace.projects, byId: new Map(workspace.projects.map((row) => [row.id, row])), byName };
    indexes.set(workspace, index);
  }
  return index;
}

/** The project index, kept current. */
export function useProjectIndex(): ProjectIndex {
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
 * The project a card's `ref.project` names on a board of `boardProject`: the board's own project when
 * it names none, else the connected project with that shared name, or null when none is connected
 * (the card shows as a placeholder).
 */
export function resolveRefProject(index: ProjectIndex, boardProject: ProjectId, refProject: ProjectName | undefined): ProjectId | null {
  return refProject === undefined ? boardProject : (index.byName.get(refProject) ?? null);
}

/** What a new card's `ref.project` is, or why it can't be added. */
export type RefProject = { readonly ok: true; readonly project?: ProjectName } | { readonly ok: false; readonly reason: "same_name" };

/**
 * The `ref.project` a card from `cardProject` gets on a board of `boardProject`: omitted for the
 * board's own project, else its shared name; refused when that name is the board project's own (such
 * a ref can't be saved).
 */
export function refProjectFor(index: ProjectIndex, boardProject: ProjectId, cardProject: ProjectId): RefProject {
  if (cardProject === boardProject) {
    return { ok: true };
  }
  const name = index.byId.get(cardProject)?.name;
  const own = index.byId.get(boardProject)?.name;
  if (name === undefined || name === own) {
    return { ok: false, reason: "same_name" };
  }
  return { ok: true, project: name };
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

/** Adds back a folder "Opened before" lists (app-data state), then selects it. */
export async function readdProject(path: string): Promise<ProjectId> {
  const added = await call("readd_project", { path });
  await selectProject(added.project.id);
  return added.project.id;
}

/** Expands or collapses a project's sidebar row (`workspace_changed` brings the new rows). */
export async function setProjectExpanded(id: ProjectId, expanded: boolean): Promise<void> {
  await call("set_project_expanded", { project: id, expanded });
}
