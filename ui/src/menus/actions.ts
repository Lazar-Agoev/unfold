/**
 * What the board and project menus do (V1's flows pattern): each runs the operation and says what
 * happened in a toast. Failures are shown, never thrown, so a menu item or an action can call one and
 * forget it. The menus, Add's commands, and the application menu all run these.
 */

import { showToast } from "../components";
import { call, toUnfoldError, windowCommand, type CompileMode, type ProjectId, type ProjectRow } from "../ipc";
import { requestFocus } from "../keyboard";
import {
  closeBoardsOfProject,
  copyPageMermaid,
  duplicateBoard,
  exportPageMermaid,
  getOpenBoard,
  moveBoardToNewWindow,
  openBoard,
  sameBoard,
  setBoardFolder,
  unpublishBoard,
  type BoardRef,
  type PageTarget,
} from "../state/boards";
import { cancelCompile, setBackgroundUpdates, startCompile } from "../state/compile";
import { setHomeView } from "../state/layout";
import { openDialog, type ProjectSettingsSection } from "../state/overlays";
import { activateHome, getActiveBoard } from "../state/tabs";
import { getProjectIndex, selectProject } from "../state/workspace";

/**
 * The focus target of the board header's title (U2 registers it on its rename field): Rename… asks
 * for it, and the header renames in place (V-74).
 */
const BOARD_TITLE_FOCUS = "board.title";

function fail(title: string, error: unknown): void {
  showToast({ title, body: toUnfoldError(error).message, tone: "danger" });
}

function projectName(project: ProjectId): string {
  return getProjectIndex().byId.get(project)?.name ?? "the project";
}

/** Whether `ref` is this window's active board tab, opening (and activating) it otherwise. */
async function showBoard(ref: BoardRef): Promise<boolean> {
  const active = getActiveBoard();
  if (active && sameBoard(active, ref)) {
    return true;
  }
  try {
    // A board another window holds is focused there instead.
    return (await openBoard(ref, { activate: true })).status === "opened";
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
        run: () => void openBoard({ project: copy.project, id: copy.id }, { activate: true }),
      },
    });
  } catch (error) {
    fail("Couldn't duplicate the board", error);
  }
}

export async function openInNewWindow(ref: BoardRef): Promise<void> {
  try {
    // Only a board this window holds can move; open it here first.
    if (!getOpenBoard(ref) && (await openBoard(ref, { activate: false })).status !== "opened") {
      return;
    }
    await moveBoardToNewWindow(ref);
  } catch (error) {
    fail("Couldn't open the board in a new window", error);
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
  openDialog({ kind: "entranceEditor", project, mode: "add" });
}

/** Health and activity: the project's Home on its Health view. */
export async function showHealth(project: ProjectId): Promise<void> {
  setHomeView("health");
  activateHome();
  await selectProject(project);
}

/** The toast for `boards_open` (V-76): another window holds one of the project's boards. */
function boardsOpenElsewhere(name: string): void {
  showToast({ title: `A board of ${name} is open in another window.`, body: "Close it there, then try again.", tone: "danger" });
}

/**
 * Remove from workspace… (V-76): the main process asks; this window's tabs of the project close
 * (saved); then the engine removes it. Its `.unfold/` stays.
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
    if (toUnfoldError(error).code === "boards_open") {
      boardsOpenElsewhere(row.name);
    } else {
      fail(`Couldn't remove ${row.name}`, error);
    }
  }
}

/** Switches the project to another worktree's folder (V-76, contract §1.2); this window's tabs of it close first. */
export async function switchWorktree(row: Pick<ProjectRow, "id" | "name">, path: string, branch: string): Promise<void> {
  try {
    await closeBoardsOfProject(row.id);
    await call("switch_worktree", { project: row.id, path });
    showToast({ title: `Switched ${row.name} to ${branch}`, body: "Drafts and the graph are per worktree." });
  } catch (error) {
    if (toUnfoldError(error).code === "boards_open") {
      boardsOpenElsewhere(row.name);
    } else {
      fail(`Couldn't switch ${row.name}`, error);
    }
  }
}
