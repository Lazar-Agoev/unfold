/**
 * Board file operations: Delete, Move to folder, Rename, health repairs, PNG export, and board
 * folders. Operations on an open board first write what is unsaved, because the engine works from the
 * version last saved.
 */

import type { ExportedFile } from "@unfold/desktop/unfold-api";
import type { BoardSummary, ProjectId, RenamedFolder, RepairSpec } from "../../ipc/generated";
import { hideBoard } from "../shownBoard";
import { findPage, setTitle } from "./document";
import { call, windowCommand } from "./engine";
import { editBoard, engineStep, flushBoard, forgetBoard, getOpenBoard } from "./store";
import type { BoardRef, PageTarget } from "./types";

/** Deletes the board's file (the caller has asked first) and forgets its session. */
export async function deleteBoard(ref: BoardRef): Promise<void> {
  // Nothing may still be on its way to the file being deleted.
  await flushBoard(ref).catch(() => undefined);
  await call("delete_board", { project: ref.project, id: ref.id });
  forgetBoard(ref);
  hideBoard(ref);
}

/** Moves any board, open or not, into a folder (null: the top level). The folder is Rust's. */
export async function setBoardFolder(ref: BoardRef, folder: string | null): Promise<BoardSummary> {
  return call("set_board_folder", { project: ref.project, id: ref.id, folder });
}

/** Renames an open board: the title is an edit like any other (one undo step), saved at once. */
export async function renameBoard(ref: BoardRef, title: string): Promise<void> {
  const trimmed = title.trim();
  if (trimmed.length === 0 || getOpenBoard(ref)?.document.title === trimmed) {
    return;
  }
  editBoard(ref, (document) => {
    setTitle(document, trimmed);
  });
  await flushBoard(ref);
}

/**
 * Applies one health repair (the engine saves it, from the board as saved, its unsaved edits first) and
 * adopts the result as one undo step on its page; throws when it can't (`engineStep`).
 */
export async function repairBoard(ref: BoardRef, repair: RepairSpec): Promise<void> {
  const outcome = await engineStep(ref, repair.page, () => call("repair_board", { project: ref.project, id: ref.id, repair }));
  if (!outcome.ok) {
    throw outcome.error;
  }
}

/**
 * Exports a page as a PNG image through the save dialog, drawn as this window has it (unsaved edits
 * included), named `<board title> - <page title>`; null when cancelled.
 */
export async function exportPagePng(target: PageTarget): Promise<ExportedFile | null> {
  await flushBoard(target.board);
  const document = getOpenBoard(target.board)?.document;
  const page = document ? findPage(document, target.page) : undefined;
  return windowCommand("export_png", {
    project: target.board.project,
    board: target.board.id,
    page: target.page,
    ...(document && page ? { title: `${document.title} - ${page.title}` } : {}),
  });
}

/** Renames a board folder, moving the boards in it. */
export async function renameFolder(project: ProjectId, from: string, to: string): Promise<RenamedFolder> {
  return call("rename_folder", { project, from, to });
}

/** Deletes an empty board folder (`folder_not_empty` otherwise). */
export async function deleteFolder(project: ProjectId, folder: string): Promise<void> {
  await call("delete_folder", { project, path: folder });
}
