/**
 * The board flows behind the title bar, the board menu, the palette, and the dialogs: each runs the
 * file operation and tells the person what happened (a toast, or the next dialog). Failures are
 * shown, never thrown, so a menu item can call them and forget.
 */

import { showToast } from "../components";
import type { RepairSpec } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
import {
  copyBoardMermaid,
  deleteBoard,
  exportBoardMermaid,
  publishBoard,
  renameBoard,
  renamePublishedFile,
  repairBoard,
  unpublishBoard,
  type FileOutcome,
} from "../state/boards/files";
import { getBoard } from "../state/boards/store";
import { moveBoardToNewWindow } from "../state/boards/transfer";
import { closeBoardDialog, openBoardDialog } from "./dialogs";

function fail(what: string, error: unknown): void {
  showToast({ message: `${what}: ${toUnfoldError(error).message}`, tone: "danger" });
}

function titleOf(boardId: string): string {
  return getBoard(boardId)?.document.title ?? "the board";
}

function afterFileWrite(boardId: string, action: "publish" | "rename", outcome: FileOutcome): void {
  if (outcome.status === "name_taken") {
    openBoardDialog({ kind: "name", boardId, action, path: outcome.path, suggestion: outcome.suggestion });
    return;
  }
  closeBoardDialog();
  if (action === "publish") {
    showToast({ message: `Published to ${outcome.path}. Edits now save into that file.`, tone: "success" });
  }
}

/** Publishes a draft (under `name` when the default was taken). */
export async function runPublish(boardId: string, name?: string): Promise<void> {
  try {
    afterFileWrite(boardId, "publish", await publishBoard(boardId, name));
  } catch (error) {
    closeBoardDialog();
    fail(`Couldn't publish ${titleOf(boardId)}`, error);
  }
}

/** Renames a board's title (and a published board's file). */
export async function runRename(boardId: string, title: string): Promise<void> {
  try {
    afterFileWrite(boardId, "rename", await renameBoard(boardId, title));
  } catch (error) {
    fail("Couldn't rename the board", error);
  }
}

/** Renames a published board's file to a name the person chose after a collision. */
export async function runRenameFile(boardId: string, name: string): Promise<void> {
  try {
    afterFileWrite(boardId, "rename", await renamePublishedFile(boardId, name));
  } catch (error) {
    closeBoardDialog();
    fail("Couldn't rename the board's file", error);
  }
}

export async function runUnpublish(boardId: string): Promise<void> {
  try {
    await unpublishBoard(boardId);
    showToast({ message: `${titleOf(boardId)} is a draft again, on this machine only.` });
  } catch (error) {
    fail("Couldn't move the board back to drafts", error);
  }
}

export async function runDelete(boardId: string): Promise<void> {
  const title = titleOf(boardId);
  closeBoardDialog();
  try {
    await deleteBoard(boardId);
    showToast({ message: `Deleted ${title}.` });
  } catch (error) {
    fail(`Couldn't delete ${title}`, error);
  }
}

export async function runExportMermaid(boardId: string): Promise<void> {
  try {
    const written = await exportBoardMermaid(boardId);
    if (written) {
      showToast({ message: `Exported Mermaid to ${written.path}.`, tone: "success" });
    }
  } catch (error) {
    fail("Couldn't export Mermaid", error);
  }
}

export async function runCopyMermaid(boardId: string): Promise<void> {
  try {
    await copyBoardMermaid(boardId);
    showToast({ message: "Copied the board as Mermaid." });
  } catch (error) {
    fail("Couldn't copy Mermaid", error);
  }
}

export async function runMoveToNewWindow(boardId: string): Promise<void> {
  try {
    await moveBoardToNewWindow(boardId);
  } catch (error) {
    fail(`Couldn't move ${titleOf(boardId)} to a new window`, error);
  }
}

/** Applies one repair; resolves true when it was applied. */
export async function runRepair(boardId: string, repair: RepairSpec): Promise<boolean> {
  try {
    await repairBoard(boardId, repair);
    showToast({ message: "Repaired and saved.", tone: "success" });
    return true;
  } catch (error) {
    fail("Couldn't repair the board", error);
    return false;
  }
}
