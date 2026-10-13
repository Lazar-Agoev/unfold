/**
 * What the board and project menus do (V1's flows pattern): each runs the operation and says what
 * happened in a toast. Failures are shown, never thrown, so a menu item can call one and forget it.
 */

import { showToast } from "../components";
import { call, toUnfoldError, windowCommand, type ProjectId, type ProjectRow } from "../ipc";
import { requestFocus } from "../lib/focus-targets";
import { closeBoardsOfProject, exportPagePng, openBoard, setBoardFolder, type BoardRef, type PageTarget } from "../state/boards";
import { cancelCompile, startCompile } from "../state/compile";
import { getProjectIndex } from "../state/workspace";

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

// Projects

export async function compile(project: ProjectId): Promise<void> {
  try {
    await startCompile(project);
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
