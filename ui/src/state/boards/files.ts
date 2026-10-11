/**
 * Board file operations: Publish, Discard changes, Move back to drafts, Delete,
 * Duplicate, Move to folder, Rename, health repairs, Mermaid and PNG export, and board folders. Operations on
 * an open board first write what is unsaved, because the engine works from the version this window
 * last saved; an id that changes (first Publish, Move back to drafts) keeps the session (undo history,
 * page views) and the tab.
 */

import type { BoardSummary, ExportedFile, ProjectId, PublishOutcome, RenamedFolder, RepairSpec, Resolve } from "../../ipc/generated";
import { removeBoardTab, renameBoardTab } from "../tabStore";
import { findPage, setTitle } from "./document";
import { call, windowCommand } from "./engine";
import { adoptBoardState, editBoard, engineStep, flushBoard, forgetBoard, getOpenBoard } from "./store";
import type { BoardRef, PageTarget } from "./types";

/** A published board's folder-relative path. */
export function publishedPath(name: string): string {
  return `.unfold/boards/${name}.json`;
}

/**
 * The file name Publish proposes for a title: accents dropped, every other run of non-alphanumeric
 * characters one `-`, at most 80 characters, `board` when nothing is left. The engine computes the
 * same slug; this one only shows the path before publishing.
 */
export function slugOf(title: string): string {
  const words = title
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((word) => word.length > 0);
  const slug = words.join("-").slice(0, 80).replace(/-+$/, "");
  return slug.length > 0 ? slug : "board";
}

/** Whether a name is a plain stem the engine accepts (letters, digits, `.`, `_`, `-`). */
export function isValidName(name: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name) && name.length <= 120;
}

/** Adopts a state whose id may differ from the open one, moving the session and its tab along. */
function adoptAs(previous: BoardRef, state: Parameters<typeof adoptBoardState>[0]): BoardRef {
  const next = adoptBoardState(state, { replaces: previous });
  renameBoardTab(previous, next);
  return next;
}

/**
 * Publishes a draft (`name` names its file on the first Publish) or a published board's unpublished
 * changes (`resolve: "keep_mine"` writes them over a changed published file). On `done` the board
 * carries on under its published id.
 */
export async function publishBoard(ref: BoardRef, options: { readonly name?: string; readonly resolve?: Resolve } = {}): Promise<PublishOutcome> {
  await flushBoard(ref);
  const result = await call("publish_board", {
    project: ref.project,
    id: ref.id,
    ...(options.name !== undefined ? { name: options.name } : {}),
    ...(options.resolve !== undefined ? { resolve: options.resolve } : {}),
  });
  if (result.status === "done") {
    adoptAs(ref, result.board);
  }
  return result;
}

/** Moves a published board back to drafts; resolves with its draft id. */
export async function unpublishBoard(ref: BoardRef): Promise<BoardRef> {
  await flushBoard(ref);
  return adoptAs(ref, await call("unpublish_board", { project: ref.project, id: ref.id }));
}

/** Deletes the board's file (the caller has asked first) and closes it in this window. */
export async function deleteBoard(ref: BoardRef): Promise<void> {
  // Nothing may still be on its way to the file being deleted.
  await flushBoard(ref).catch(() => undefined);
  await call("delete_board", { project: ref.project, id: ref.id });
  forgetBoard(ref);
  removeBoardTab(ref);
}

/** Copies the board (its working version) into a new draft "<title> copy" in the same folder. */
export async function duplicateBoard(ref: BoardRef): Promise<BoardSummary> {
  if (getOpenBoard(ref)) {
    await flushBoard(ref);
  }
  return call("duplicate_board", { project: ref.project, id: ref.id });
}

/** Moves any board, open or not, into a folder (null: the top level). The folder is Rust's (U-47). */
export async function setBoardFolder(ref: BoardRef, folder: string | null): Promise<BoardSummary> {
  return call("set_board_folder", { project: ref.project, id: ref.id, folder });
}

/** Renames an open board: the title is an edit like any other (one undo step), saved at once (U-43). */
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

/** The default file name of a page's export: the board's title, and the page's when there are several. */
function exportTitle(target: PageTarget): string | undefined {
  const document = getOpenBoard(target.board)?.document;
  if (!document) {
    return undefined;
  }
  const page = findPage(document, target.page);
  return document.pages.length > 1 && page ? `${document.title} ${page.title}` : document.title;
}

/** Exports a page's saved version as Mermaid through the save dialog; null when cancelled. */
export async function exportPageMermaid(target: PageTarget): Promise<ExportedFile | null> {
  await flushBoard(target.board);
  const title = exportTitle(target);
  return windowCommand("export_mermaid", {
    project: target.board.project,
    board: target.board.id,
    page: target.page,
    ...(title !== undefined ? { title } : {}),
  });
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

/** Puts a page's saved version on the clipboard as Mermaid. */
export async function copyPageMermaid(target: PageTarget): Promise<void> {
  await flushBoard(target.board);
  const { text } = await call("export_mermaid_text", { project: target.board.project, board: { id: target.board.id, page: target.page } });
  await windowCommand("copy_to_clipboard", { text });
}

/** Adds an empty board folder (a no-op when it exists). */
export async function createFolder(project: ProjectId, path: string): Promise<void> {
  await call("create_folder", { project, path });
}

/** Renames a board folder, moving the boards at or under it. */
export async function renameFolder(project: ProjectId, from: string, to: string): Promise<RenamedFolder> {
  return call("rename_folder", { project, from, to });
}

/** Deletes an empty board folder (`folder_not_empty` otherwise). */
export async function deleteFolder(project: ProjectId, path: string): Promise<void> {
  await call("delete_folder", { project, path });
}
