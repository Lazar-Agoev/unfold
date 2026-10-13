/**
 * What the window's own controls do: showing a project on Home, Add project, and Settings. Screens
 * call them directly.
 */

import { showToast } from "../components";
import type { ProjectId } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
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

/** Add Project…: the folder picker; the added project shows on Home, or a toast says why it can't be added. */
export async function addProject(): Promise<void> {
  try {
    if ((await addProjectFromPicker()) !== null) {
      setHomeView("overview");
      showHome();
    }
  } catch (error) {
    showToast({ title: "Couldn't add the folder", body: toUnfoldError(error).message, tone: "danger" });
  }
}

export function openSettings(): void {
  openDialog({ kind: "settings" });
}
