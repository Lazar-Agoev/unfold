/**
 * What the window's own controls do: showing a project on Home, Add project, and Settings. Screens
 * call them directly.
 */

import type { ProjectId } from "../ipc/generated";
import { setHomeView } from "./layout";
import { openDialog } from "./overlays";
import { showHome } from "./shownBoard";
import { addProjectFromPicker, selectProject } from "./workspace";

/** Shows a project on Home (its overview), as a project row click does and after it is added. */
export async function showProject(project: ProjectId): Promise<void> {
  setHomeView("overview");
  showHome();
  await selectProject(project);
}

/** Add Project…: the folder picker; the added project shows on Home. */
export async function addProject(): Promise<void> {
  if ((await addProjectFromPicker()) !== null) {
    setHomeView("overview");
    showHome();
  }
}

export function openSettings(): void {
  openDialog({ kind: "settings" });
}
