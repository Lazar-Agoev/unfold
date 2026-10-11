/**
 * The window's session: what the window remembers in `windows.json`, how it comes
 * back at launch or on reload, and how it recovers when the engine host restarts.
 *
 * - Restore: `window_state {key}` → the layout → the Home selection → the saved board tabs, reopened
 *   on their pages (a board that can't open loses its tab) → the active tab. Only then are boards
 *   moved here from another window adopted, agents' requests answered, and the window's state followed.
 * - Save: `save_window_state` 400 ms after the selection, the tabs, a board's active page, or the
 *   layout change, and at once when the page goes away (the window closes, the app quits).
 * - Engine restart: a toast while the engine is away; once it is back, the
 *   selection is sent again, every board tab is claimed again and its unsaved edits saved, the window's
 *   state saved, and the query cache reset, in that order.
 * - `focus_board`: another window asked for a board this one holds, so its tab is shown.
 */

import type { QueryClient } from "@tanstack/react-query";
import { createElement } from "react";
import { create } from "zustand";
import { ProgressRing, dismissToast, showToast } from "../components";
import { call } from "../ipc/commands";
import type { SavedWindow, WindowStateInput } from "../ipc/generated";
import { engineQueryOptions, resetEngineQueries } from "../ipc/queries";
import { onConnection, onShellEvent } from "../ipc/unfold";
import { boardKey, openBoard, reclaimBoards, setAgentChannelReady, setTransfersReady, type BoardRef } from "./boards";
import { applyLayoutSnapshot, layoutSnapshot, subscribeToLayout } from "./layout";
import { activateBoard, moveTab, subscribeToTabs, tabsSnapshot } from "./tabs";
import { useWindowStore } from "./window";
import { selectProject } from "./workspace";

/** How long the window waits after a change before saving its state. */
export const SAVE_DELAY_MS = 400;

/** The engine toast's id, so showing it again replaces it. */
const ENGINE_TOAST = "engine";

const useSession = create<{ readonly restored: boolean }>()(() => ({ restored: false }));

/** Whether the window's saved state is back (its selection and tabs are the restored ones from then on). */
export function useRestored(): boolean {
  return useSession((state) => state.restored);
}

/** The restore, run once per page load (the session may be installed again, as under StrictMode). */
let restoring: Promise<void> | null = null;

function windowKey(): string | null {
  return useWindowStore.getState().info?.key ?? null;
}

/** The window's state as `save_window_state` takes it. */
function windowState(key: string): WindowStateInput {
  const layout = layoutSnapshot();
  return {
    key,
    selectedProject: useWindowStore.getState().selectedProject,
    ...tabsSnapshot(),
    layout: { ...layout, folders: [...layout.folders] },
  };
}

async function saveWindowState(key: string): Promise<void> {
  try {
    await call("save_window_state", { state: windowState(key) });
  } catch (error) {
    console.warn("Saving the window's state failed:", error);
  }
}

/** Selects the saved project if the workspace still has it; otherwise the frame picks one (the workspace's). */
async function restoreSelection(client: QueryClient, saved: SavedWindow): Promise<void> {
  const project = saved.selectedProject;
  if (project === null) {
    return;
  }
  const workspace = await client.query(engineQueryOptions("workspace", {})).catch(() => undefined);
  if (workspace?.projects.some((row) => row.id === project)) {
    await selectProject(project);
  }
}

/** Reopens the saved board tabs on their pages, in their order, then activates the saved one. */
async function reopenTabs(saved: SavedWindow): Promise<void> {
  const opened = await Promise.all(
    saved.tabs.map(async (tab): Promise<BoardRef | null> => {
      try {
        const outcome = await openBoard({ project: tab.project, id: tab.board }, { activate: false, ...(tab.page === null ? {} : { page: tab.page }) });
        return outcome.status === "opened" ? outcome.ref : null;
      } catch (error) {
        console.warn(`Reopening ${tab.board} failed:`, error);
        return null;
      }
    }),
  );
  // The boards open in parallel, so their tabs arrive in any order: put them back in the saved one.
  opened
    .filter((ref) => ref !== null)
    .forEach((ref, index) => {
      moveTab(boardKey(ref), index);
    });
  const active = saved.activeTab === null ? null : opened[saved.activeTab];
  if (active) {
    activateBoard(active);
  }
}

async function restore(client: QueryClient): Promise<void> {
  const key = windowKey();
  const saved =
    key === null
      ? null
      : await call("window_state", { key }).catch((error: unknown) => {
          console.warn("Reading the window's saved state failed:", error);
          return null;
        });
  if (saved) {
    applyLayoutSnapshot(saved.layout);
    await restoreSelection(client, saved);
    await reopenTabs(saved);
  }
  setTransfersReady(true);
  setAgentChannelReady();
  useSession.setState({ restored: true });
}

/**
 * Saves the window's state after each change (debounced). A save still waiting when the page goes away
 * is sent at once, so the last change isn't lost. Returns the stop function.
 */
function followWindowState(key: string): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    timer = null;
    void saveWindowState(key);
  };
  const schedule = () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = setTimeout(save, SAVE_DELAY_MS);
  };
  const saveNow = () => {
    if (timer !== null) {
      clearTimeout(timer);
      save();
    }
  };
  const stops = [
    useWindowStore.subscribe((state, previous) => {
      if (state.selectedProject !== previous.selectedProject) {
        schedule();
      }
    }),
    subscribeToTabs(schedule),
    subscribeToLayout(schedule),
  ];
  window.addEventListener("pagehide", saveNow);
  return () => {
    window.removeEventListener("pagehide", saveNow);
    if (timer !== null) {
      clearTimeout(timer);
    }
    for (const stop of stops) {
      stop();
    }
  };
}

/**
 * After an engine-host restart: Rust came back with every project resting and no
 * claims, so the window sends its selection again, claims its boards again and saves their unsaved
 * edits (`reclaimBoards`), saves its own state, and only then drops what the cache held.
 */
async function reconnect(client: QueryClient): Promise<void> {
  await restoring;
  await selectProject(useWindowStore.getState().selectedProject);
  await reclaimBoards();
  const key = windowKey();
  if (key !== null) {
    await saveWindowState(key);
  }
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
 * Restores the window (once per page load), then saves its state as it changes; follows the engine
 * connection; and shows the tab of a board another window asked for. Returns the uninstall function.
 */
export function installSession(client: QueryClient): () => void {
  let installed = true;
  let stopSaving: (() => void) | null = null;
  const stops = [
    followConnection(client),
    onShellEvent("focus_board", ({ project, board }) => {
      activateBoard({ project, id: board });
    }),
  ];
  restoring ??= restore(client);
  void restoring.then(() => {
    const key = windowKey();
    if (installed && key !== null) {
      stopSaving = followWindowState(key);
    }
  });
  return () => {
    installed = false;
    stopSaving?.();
    for (const stop of stops) {
      stop();
    }
  };
}

/** Forgets the restore (tests). */
export function resetSession(): void {
  restoring = null;
  useSession.setState({ restored: false });
}
