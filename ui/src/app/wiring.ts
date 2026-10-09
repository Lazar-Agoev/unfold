/**
 * Window-wide wiring (UI plan §2 `app/`, core API "Wiring the frame"): the shortcut dispatcher and menu
 * commands, the saved theme and key bindings, cache invalidation and the board and compile event
 * handlers, a toast for actions that fail, `project_added`, and the project Home shows at first.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { showToast } from "../components";
import { installEngineInvalidation, onShellEvent, type ProjectId } from "../ipc";
import { setActionErrorHandler, useMenuCommands, useShortcutDispatcher } from "../keyboard";
import { usePreferencesKeyBindings, usePreferencesTheme } from "../preferences";
import { showProject } from "../state/actions";
import { installBoardEvents, setTransfersReady } from "../state/boards";
import { installCompileEvents } from "../state/compile";
import { useSelectedProject } from "../state/window";
import { selectProject, useWorkspace } from "../state/workspace";

export function useWindowWiring(): void {
  const client = useQueryClient();
  useShortcutDispatcher();
  useMenuCommands();
  usePreferencesTheme();
  usePreferencesKeyBindings();

  useEffect(() => installEngineInvalidation(client), [client]);
  useEffect(() => installBoardEvents(), []);
  useEffect(() => installCompileEvents(), []);

  useEffect(() => {
    setActionErrorHandler((error, action) => {
      const message = error instanceof Error ? error.message : String(error);
      showToast({ title: `${action.title.replace(/…$/, "")} failed`, body: message, tone: "danger" });
    });
    return () => {
      setActionErrorHandler(null);
    };
  }, []);

  // A project added from the menu, the command line, the OS, or a drop the frame didn't handle.
  useEffect(() => onShellEvent("project_added", (event) => void showProject(event.project)), []);

  // Boards moved here from another window are adopted once the window is set up.
  useEffect(() => {
    setTransfersReady(true);
  }, []);

  useDefaultProject();
}

/**
 * Home shows a project from the start: the workspace's selected one, else the first (UI plan §3.2). When
 * the shown project leaves the workspace, it picks again. (A project just added may be selected before
 * the workspace lists it, so only a project the workspace listed before counts as gone.)
 */
function useDefaultProject(): void {
  const workspace = useWorkspace();
  const selected = useSelectedProject();
  const listed = useRef<ReadonlySet<ProjectId>>(new Set());
  useEffect(() => {
    if (!workspace) {
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
  }, [workspace, selected]);
}
