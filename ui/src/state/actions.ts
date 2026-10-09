/**
 * What the window-level actions do (UI plan §8.1, the ones the frame registers): Home, closing tabs and
 * windows, New board, Add project, Compile, Preferences, and showing a project. The frame registers
 * them with their ids and titles; screens may call them directly.
 */

import type { ProjectId } from "../ipc/generated";
import { windowCommand } from "../ipc/unfold";
import { newBoard } from "./boards/open";
import type { BoardRef } from "./boards/types";
import { startCompile } from "./compile";
import { setHomeView } from "./layout";
import { openDialog } from "./overlays";
import { closeTab } from "./tabs";
import { activateHome, getActiveBoard, useTabs } from "./tabStore";
import { useWindowStore } from "./window";
import { addProjectFromPicker, selectProject } from "./workspace";

/** The project the window is working in: the active board's, else the one Home shows. */
export function currentProject(): ProjectId | null {
  return getActiveBoard()?.project ?? useWindowStore.getState().selectedProject;
}

/** Shows a project on Home (its overview), as a project row click does (V-52) and after it is added. */
export async function showProject(project: ProjectId): Promise<void> {
  setHomeView("overview");
  activateHome();
  await selectProject(project);
}

/**
 * Close Tab (⌘W): closes the active board tab. On Home it closes the window when Home is the only tab,
 * and does nothing otherwise (V-44).
 */
export async function closeActiveTab(): Promise<void> {
  const { active, boards } = useTabs.getState();
  if (active !== "home") {
    await closeTab(active);
  } else if (boards.length === 0) {
    await windowCommand("close_window");
  }
}

/** New Board (⌘N): a draft in the active board's project, or in the project Home shows (V-63). */
export async function newBoardHere(): Promise<BoardRef | null> {
  const project = currentProject();
  return project === null ? null : newBoard(project);
}

/** Compile (⇧⌘B) the project the window is working in. */
export async function compileHere(): Promise<void> {
  const project = currentProject();
  if (project !== null) {
    await startCompile(project);
  }
}

/** Add Project… (⌘O): the folder picker; the added project shows on Home. */
export async function addProject(): Promise<void> {
  if ((await addProjectFromPicker()) !== null) {
    setHomeView("overview");
    activateHome();
  }
}

export async function newWindow(): Promise<void> {
  await windowCommand("new_window");
}

export async function closeWindow(): Promise<void> {
  await windowCommand("close_window");
}

export function openPreferences(): void {
  openDialog({ kind: "preferences" });
}
