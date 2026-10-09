/**
 * Connects the open boards to the engine's events once per window: outside edits (`board_conflict`),
 * graph updates of any project a board's cards resolve to (its health, outlines, and label refresh;
 * card models refresh through the query cache's own invalidation), a project's folder changing
 * (`project_reset` forgets its remembered renders), and the page going away (unsaved edits and views
 * are sent at once). The engine-restart order lives in the window's session code.
 */

import { forgetVerifiedCode } from "../../canvas/cardData";
import type { ProjectId } from "../../ipc/generated";
import { getProjectIndex } from "../workspace";
import { onEngineEvent } from "./engine";
import { boardKey } from "./keys";
import { handleOutsideChange, openBoards, refreshHealth, saveAllNow } from "./store";
import type { OpenBoard } from "./types";

/** Graph updates arrive in bursts while files are saved; a board's health is refreshed once they settle. */
export const HEALTH_DELAY_MS = 250;

/** Whether any card of the board can resolve to `project` (its own project, or a named one connected as it). */
function readsProject(board: OpenBoard, project: ProjectId): boolean {
  if (board.ref.project === project) {
    return true;
  }
  const index = getProjectIndex();
  return board.document.projects.some((name) => index.byName.get(name) === project);
}

/** Installs the window's board subscriptions. Returns the uninstall function. */
export function installBoardEvents(): () => void {
  const healthTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const unsubscribers = [
    onEngineEvent("board_conflict", (event) => {
      void handleOutsideChange({ project: event.project, id: event.board }, event.diskRevision);
    }),
    onEngineEvent("graph_updated", (event) => {
      for (const board of openBoards()) {
        if (!readsProject(board, event.project)) {
          continue;
        }
        const key = boardKey(board.ref);
        clearTimeout(healthTimers.get(key));
        healthTimers.set(
          key,
          setTimeout(() => {
            healthTimers.delete(key);
            void refreshHealth(board.ref);
          }, HEALTH_DELAY_MS),
        );
      }
    }),
    onEngineEvent("project_reset", (event) => {
      forgetVerifiedCode(event.project);
    }),
  ];
  window.addEventListener("pagehide", saveAllNow);
  return () => {
    window.removeEventListener("pagehide", saveAllNow);
    for (const timer of healthTimers.values()) {
      clearTimeout(timer);
    }
    for (const unsubscribe of unsubscribers) {
      unsubscribe();
    }
  };
}
