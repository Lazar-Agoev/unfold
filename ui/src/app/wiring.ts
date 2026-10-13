/**
 * Window-wide wiring: cache invalidation and the board and compile event handlers, the view reports
 * agents default to, the window's session (restore, engine restarts), `project_added` and
 * `project_add_failed`, and the project Home shows at first.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { installViewReports } from "../agent";
import { showToast } from "../components";
import { installEngineInvalidation, onShellEvent, type ProjectId } from "../ipc";
import { displayPath } from "../lib/format";
import { showProject } from "../state/actions";
import { installBoardEvents } from "../state/boards";
import { installCompileEvents } from "../state/compile";
import { installSession, useRestored } from "../state/session";
import { useSelectedProject } from "../state/window";
import { selectProject, useWorkspace } from "../state/workspace";

export function useWindowWiring(): void {
  const client = useQueryClient();

  useEffect(() => installEngineInvalidation(client), [client]);
  useEffect(() => installBoardEvents(), []);
  useEffect(() => installCompileEvents(), []);
  useEffect(() => installViewReports(), []);

  // A project added from File › Add Project….
  useEffect(() => onShellEvent("project_added", (event) => void showProject(event.project)), []);

  // Adding a folder from File › Add Project… failed.
  useEffect(
    () =>
      onShellEvent("project_add_failed", ({ path, message }) => {
        showToast({ title: `Couldn't add ${displayPath(path)}`, body: message, tone: "danger" });
      }),
    [],
  );

  // Restores the board the window showed last, then reports what it shows.
  useEffect(() => installSession(client), [client]);

  useDefaultProject();
}

/**
 * Home shows a project from the start: the workspace's selected one, else the first. When the shown project leaves the workspace, it picks again. (A project
 * just added may be selected before the workspace lists it, so only a project the workspace listed
 * before counts as gone.)
 */
function useDefaultProject(): void {
  const restored = useRestored();
  const workspace = useWorkspace();
  const selected = useSelectedProject();
  const listed = useRef<ReadonlySet<ProjectId>>(new Set());
  useEffect(() => {
    if (!restored || !workspace) {
      return;
    }
    const rows = workspace.projects;
    const wasListed = selected !== null && listed.current.has(selected);
    listed.current = new Set(rows.map((row) => row.id));
    const gone = selected !== null && wasListed && !listed.current.has(selected);
    if (selected !== null && !gone) {
      return;
    }
    const fallback = rows.find((row) => row.id === workspace.selected)?.id ?? rows[0]?.id ?? null;
    if (fallback !== selected) {
      void selectProject(fallback);
    }
  }, [restored, workspace, selected]);
}
