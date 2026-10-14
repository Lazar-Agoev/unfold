/**
 * Connects the open boards to the engine's events once per window: outside changes to a board's file
 * (`board_reloaded`), graph updates and entrance changes of a board's project (its health and label
 * refresh; cards refresh through the query cache's own invalidation), and the page going away (unsaved
 * edits are sent at once). The engine-restart order lives in the window's session code.
 */

import type { ProjectId } from "../../ipc/generated";
import { onEngineEvent } from "./engine";
import { boardKey } from "./keys";
import { fileChanged, openBoards, refreshHealth, saveAllNow } from "./store";

/** Graph updates arrive in bursts while files are saved; a board's health is refreshed once they settle. */
export const HEALTH_DELAY_MS = 250;

/** Installs the window's board subscriptions. Returns the uninstall function. */
export function installBoardEvents(): () => void {
  const healthTimers = new Map<string, ReturnType<typeof setTimeout>>();
  /** Refreshes the health of the project's open boards once its events settle. */
  const healthChanged = ({ project }: { readonly project: ProjectId }) => {
    for (const board of openBoards()) {
      if (board.ref.project !== project) {
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
  };
  const unsubscribers = [
    onEngineEvent("board_reloaded", (event) => {
      fileChanged({ project: event.project, id: event.board }, event.file);
    }),
    onEngineEvent("graph_updated", healthChanged),
    // An `[[app]]` edit compiles nothing, yet a card's entrance may be renamed or gone.
    onEngineEvent("entrances_changed", healthChanged),
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
