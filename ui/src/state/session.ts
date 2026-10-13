/**
 * The window's session: how it comes back at launch or on reload, and how it recovers when the engine
 * host restarts.
 *
 * - Restore: the board `window_info` names is opened and shown (Home when it is gone). Only then are
 *   agents' requests answered, and the shown board reported to the main process as it changes.
 * - Engine restart: a toast while the engine is away; once it is back, the selection is sent again,
 *   every open board is opened again and its unsaved edits saved, and the query cache reset, in that
 *   order.
 */

import type { QueryClient } from "@tanstack/react-query";
import { createElement } from "react";
import { create } from "zustand";
import { ProgressRing, dismissToast, showToast } from "../components";
import { resetEngineQueries } from "../ipc/queries";
import { onConnection } from "../ipc/unfold";
import { openBoard, reopenBoards, setAgentChannelReady } from "./boards";
import { reportShownBoard } from "./shownBoard";
import { useWindowStore } from "./window";
import { selectProject } from "./workspace";

/** The engine toast's id, so showing it again replaces it. */
const ENGINE_TOAST = "engine";

const useSession = create<{ readonly restored: boolean }>()(() => ({ restored: false }));

/** Whether the window's last board is back (shown, or found gone). */
export function useRestored(): boolean {
  return useSession((state) => state.restored);
}

/** The restore, run once per page load (the session may be installed again, as under StrictMode). */
let restoring: Promise<void> | null = null;

/** Shows the board the window showed last, if it still exists. */
async function restore(): Promise<void> {
  const board = useWindowStore.getState().info?.board ?? null;
  if (board !== null) {
    try {
      await openBoard({ project: board.project, id: board.board });
    } catch (error) {
      console.warn(`Reopening ${board.board} failed:`, error);
    }
  }
  setAgentChannelReady();
  useSession.setState({ restored: true });
}

/**
 * After an engine-host restart: Rust came back with every project resting and no board open, so the
 * window sends its selection again, opens its boards again and saves their unsaved edits
 * (`reopenBoards`), and only then drops what the cache held.
 */
async function reconnect(client: QueryClient): Promise<void> {
  await restoring;
  await selectProject(useWindowStore.getState().selectedProject);
  await reopenBoards();
  await resetEngineQueries(client);
}

/** Shows the engine toast while the engine is away, and runs the reconnect sequence when it is back. */
function followConnection(client: QueryClient): () => void {
  let lost = false;
  // Counts losses, so a sequence overtaken by another loss leaves the toast up.
  let losses = 0;
  return onConnection((state) => {
    if (state === "reconnecting" || state === "unavailable") {
      lost = true;
      losses += 1;
      showToast({
        id: ENGINE_TOAST,
        title: "Reconnecting to the engine…",
        body: "Your boards stay open. Edits are saved once it's back.",
        leading: createElement(ProgressRing, { size: 14 }),
        duration: null,
      });
    } else if (state === "connected" && lost) {
      lost = false;
      const loss = losses;
      void reconnect(client).finally(() => {
        if (losses === loss) {
          dismissToast(ENGINE_TOAST);
        }
      });
    }
  });
}

/**
 * Restores the window (once per page load), then reports the shown board as it changes, and follows
 * the engine connection. Returns the uninstall function.
 */
export function installSession(client: QueryClient): () => void {
  let installed = true;
  let stopReporting: (() => void) | null = null;
  const stopFollowing = followConnection(client);
  restoring ??= restore();
  void restoring.then(() => {
    if (installed) {
      stopReporting = reportShownBoard();
    }
  });
  return () => {
    installed = false;
    stopReporting?.();
    stopFollowing();
  };
}

/** Forgets the restore (tests). */
export function resetSession(): void {
  restoring = null;
  useSession.setState({ restored: false });
}
