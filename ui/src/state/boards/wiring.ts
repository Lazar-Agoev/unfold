/**
 * Connects the board store to the window once: the top Code/Flow lens, outside edits
 * (`board_conflict`), graph updates (board health; card models are refreshed by the query cache's
 * own invalidation in `ipc/queries`), engine-host restarts (boards are claimed again and unsaved
 * edits written), and the page going away (unsaved edits are sent at once).
 */

import { onConnection } from "../../ipc/unfold";
import { subscribeToLens } from "../lens";
import { onEngineEvent } from "./engine";
import { applyLensToBoards, handleOutsideChange, reclaimBoards, refreshHealth, saveAllNow } from "./store";

/** Graph updates arrive in bursts while files are saved; health is refreshed once they settle. */
const HEALTH_DELAY_MS = 250;

let installed = false;
let healthTimer: ReturnType<typeof setTimeout> | null = null;

/** Installs the window-wide subscriptions (idempotent). */
export function installBoardWiring(): void {
  if (installed) {
    return;
  }
  installed = true;
  subscribeToLens(applyLensToBoards);
  onEngineEvent("board_conflict", (event) => {
    void handleOutsideChange(event.board, event.diskRevision);
  });
  onEngineEvent("graph_updated", () => {
    if (healthTimer !== null) {
      clearTimeout(healthTimer);
    }
    healthTimer = setTimeout(() => {
      healthTimer = null;
      void refreshHealth();
    }, HEALTH_DELAY_MS);
  });
  // The first `connected` is the window's own attach; a later one follows an engine-host restart.
  let lost = false;
  onConnection((state) => {
    if (state === "reconnecting" || state === "unavailable") {
      lost = true;
    } else if (state === "connected" && lost) {
      lost = false;
      void reclaimBoards();
    }
  });
  if (typeof window !== "undefined") {
    window.addEventListener("pagehide", saveAllNow);
  }
}
