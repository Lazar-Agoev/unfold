/**
 * What the board and project menus do (V1's flows pattern): each runs the operation and says what
 * happened in a toast. Failures are shown, never thrown, so a menu item can call one and forget it.
 */

import { showToast } from "../components";
import { call, toUnfoldError, windowCommand, type CompileMode, type ProjectId, type ProjectRow } from "../ipc";
import { requestFocus } from "../lib/focus-targets";
import {
  closeBoardsOfProject,
  copyPageMermaid,
  duplicateBoard,
  exportPageMermaid,
  exportPagePng,
  getOpenBoard,
  openBoard,
  setBoardFolder,
  unpublishBoard,
  type BoardRef,
  type PageTarget,
} from "../state/boards";
import { cancelCompile, setBackgroundUpdates, startCompile } from "../state/compile";
import { setHomeView } from "../state/layout";
import { openDialog, type ProjectSettingsSection } from "../state/overlays";
import { showHome } from "../state/shownBoard";
import { getProjectIndex, selectProject } from "../state/workspace";

/**
 * The focus target of the board header's title (the header registers it on its rename field): Rename… asks
 * for it, and the header renames in place.
 */
const BOARD_TITLE_FOCUS = "board.title";

function fail(title: string, error: unknown): void {
  showToast({ title, body: toUnfoldError(error).message, tone: "danger" });
}

function projectName(project: ProjectId): string {
  return getProjectIndex().byId.get(project)?.name ?? "the project";
}

/** Shows `ref` (opening it if needed); false when it couldn't be opened. */
async function showBoard(ref: BoardRef): Promise<boolean> {
  try {
    await openBoard(ref);
    return true;
  } catch (error) {
    fail("Couldn't open the board", error);
    return false;
  }
}

// Boards

/** Rename…: shows the board and puts its title into rename mode in the sidebar header. */
export async function renameBoardInPlace(ref: BoardRef): Promise<void> {
  if (await showBoard(ref)) {
    requestFocus(BOARD_TITLE_FOCUS);
  }
}

export async function moveToFolder(ref: BoardRef, folder: string | null): Promise<void> {
  try {
    await setBoardFolder(ref, folder);
  } catch (error) {
    fail("Couldn't move the board", error);
  }
}

export async function duplicate(ref: BoardRef): Promise<void> {
  try {
    const copy = await duplicateBoard(ref);
    showToast({
      title: `Made ${copy.title ?? copy.name}`,
      body: "A draft, in the same folder.",
      action: {
        label: "Open",
        run: () => void openBoard({ project: copy.project, id: copy.id }),
      },
    });
  } catch (error) {
    fail("Couldn't duplicate the board", error);
  }
}

export async function moveBackToDrafts(ref: BoardRef, title: string): Promise<void> {
  try {
    if (getOpenBoard(ref)) {
      await unpublishBoard(ref);
    } else {
      await call("unpublish_board", { project: ref.project, id: ref.id });
    }
    showToast({ title: `${title} is a draft again`, body: "Only on this machine. Commit the removed published file to share that." });
  } catch (error) {
    fail("Couldn't move the board back to drafts", error);
  }
}

export async function exportMermaid(target: PageTarget): Promise<void> {
  try {
    const written = await exportPageMermaid(target);
    if (written) {
      showToast({ title: "Exported Mermaid", body: written.path });
    }
  } catch (error) {
    fail("Couldn't export Mermaid", error);
  }
}

export async function exportPng(target: PageTarget): Promise<void> {
  try {
    const written = await exportPagePng(target);
    if (written) {
      showToast({ title: "Exported PNG", body: written.path });
    }
  } catch (error) {
    fail("Couldn't export the page as PNG", error);
  }
}

export async function copyMermaid(target: PageTarget): Promise<void> {
  try {
    await copyPageMermaid(target);
    showToast({ title: "Copied the page as Mermaid" });
  } catch (error) {
    fail("Couldn't copy Mermaid", error);
  }
}

// Projects

export async function compile(project: ProjectId, mode: CompileMode): Promise<void> {
  try {
    await startCompile(project, mode);
  } catch (error) {
    fail(`Couldn't compile ${projectName(project)}`, error);
  }
}

export async function cancel(project: ProjectId): Promise<void> {
  try {
    await cancelCompile(project);
  } catch (error) {
    fail("Couldn't cancel the compile", error);
  }
}

export async function backgroundUpdates(project: ProjectId, on: boolean): Promise<void> {
  try {
    await setBackgroundUpdates(project, on);
  } catch (error) {
    fail(on ? "Couldn't resume background updates" : "Couldn't pause background updates", error);
  }
}

export function openSettings(project: ProjectId, section?: ProjectSettingsSection): void {
  openDialog(section === undefined ? { kind: "projectSettings", project } : { kind: "projectSettings", project, section });
}

export function addEntrance(project: ProjectId): void {
  openDialog({ kind: "launchEditor", project, mode: "add" });
}

/** Health and activity: the project's Home on its Health view. */
export async function showHealth(project: ProjectId): Promise<void> {
  setHomeView("health");
  showHome();
  await selectProject(project);
}

/**
 * Remove from workspace…: the main process asks; the project's open boards close (saved), since the
 * engine refuses to remove a project with a board open; then the engine removes it. Its `.unfold/` stays.
 */
export async function removeProject(row: Pick<ProjectRow, "id" | "name">): Promise<void> {
  try {
    const { confirmed } = await windowCommand("confirm_remove_project", { name: row.name });
    if (!confirmed) {
      return;
    }
    await closeBoardsOfProject(row.id);
    await call("remove_project", { project: row.id });
  } catch (error) {
    fail(`Couldn't remove ${row.name}`, error);
  }
}

/** Switches the project to another worktree's folder (worktree metadata); its open boards close first. */
export async function switchWorktree(row: Pick<ProjectRow, "id" | "name">, path: string, branch: string): Promise<void> {
  try {
    await closeBoardsOfProject(row.id);
    await call("switch_worktree", { project: row.id, path });
    showToast({ title: `Switched ${row.name} to ${branch}`, body: "Drafts and the graph are per worktree." });
  } catch (error) {
    fail(`Couldn't switch ${row.name}`, error);
  }
}
