/**
 * Board file operations (architecture §10.5, reader.md "Files"): Publish, Rename, Move back to
 * drafts, Delete, health repairs, and Mermaid export. Each first writes what is unsaved, because
 * the engine works from the version this window last saved; a changed ID (publish, rename,
 * unpublish) keeps the board's session (undo history, camera) through `adoptBoardState`.
 */

import type { BoardState, RepairSpec } from "../../ipc/generated";
import { windowCommand } from "../../ipc/unfold";
import { call } from "./engine";
import { adoptBoardState, applyServerState, editBoard, flushBoard, forgetBoard, getBoard } from "./store";
import { setTitle } from "./document";

/** What Publish or a file rename did: done (the board's new ID), or the name is taken. */
export type FileOutcome =
  | { readonly status: "done"; readonly id: string; readonly path: string }
  | { readonly status: "name_taken"; readonly path: string; readonly suggestion: string };

/** A published board's folder-relative path. */
export function publishedPath(name: string): string {
  return `.unfold/boards/${name}.json`;
}

/**
 * The file name Publish proposes for a title (reader.md "Names"): accents dropped, every other run
 * of non-alphanumeric characters one `-`, at most 80 characters, `board` when nothing is left.
 * The engine computes the same slug; this one only shows the path before publishing.
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

function adopted(previous: string, state: BoardState): FileOutcome {
  adoptBoardState(state, { replaces: previous });
  return { status: "done", id: state.id, path: state.id };
}

/** Publishes a draft to `.unfold/boards/<name>.json` (the slug of its title when `name` is absent). */
export async function publishBoard(id: string, name?: string): Promise<FileOutcome> {
  await flushBoard(id);
  const result = await call("publish_board", name !== undefined ? { id, name } : { id });
  if (result.status === "name_taken") {
    return result;
  }
  return adopted(id, result.board);
}

/**
 * Renames a board: the title is an edit like any other (one undo step, saved at once); a
 * published board's file follows the new title's slug with `rename_board`, never overwriting.
 */
export async function renameBoard(id: string, title: string): Promise<FileOutcome> {
  const trimmed = title.trim();
  const board = getBoard(id);
  if (!board || trimmed.length === 0) {
    return { status: "done", id, path: id };
  }
  if (board.document.title !== trimmed) {
    editBoard(id, (document) => {
      setTitle(document, trimmed);
    });
  }
  await flushBoard(id);
  const current = getBoard(id);
  if (current?.place !== "published" || current.name === slugOf(trimmed)) {
    return { status: "done", id, path: id };
  }
  return renamePublishedFile(id, slugOf(trimmed));
}

/** Renames a published board's file (never overwriting another board). */
export async function renamePublishedFile(id: string, name: string): Promise<FileOutcome> {
  await flushBoard(id);
  const result = await call("rename_board", { id, name });
  if (result.status === "name_taken") {
    return result;
  }
  return adopted(id, result.board);
}

/** Moves a published board back to drafts. */
export async function unpublishBoard(id: string): Promise<string> {
  await flushBoard(id);
  const state = await call("unpublish_board", { id });
  adoptBoardState(state, { replaces: id });
  return state.id;
}

/** Deletes the board's file and closes it in this window (the caller has asked first). */
export async function deleteBoard(id: string): Promise<void> {
  // Nothing may still be on its way to the file being deleted.
  await flushBoard(id).catch(() => undefined);
  await call("delete_board", { id });
  forgetBoard(id);
}

/** Applies one health repair (the engine saves it) and adopts the result as the saved board. */
export async function repairBoard(id: string, repair: RepairSpec): Promise<void> {
  await flushBoard(id);
  const state = await call("repair_board", { id, repair });
  applyServerState(id, state);
}

/** Exports the board's saved version as Mermaid through the save dialog; null when cancelled. */
export async function exportBoardMermaid(id: string): Promise<{ path: string; bytes: number } | null> {
  await flushBoard(id);
  const title = getBoard(id)?.document.title;
  return windowCommand("export_mermaid", title !== undefined ? { boardId: id, title } : { boardId: id });
}

/** Puts the board's saved version on the clipboard as Mermaid. */
export async function copyBoardMermaid(id: string): Promise<void> {
  await flushBoard(id);
  const { text } = await call("export_mermaid_text", { board: id });
  await windowCommand("copy_to_clipboard", { text });
}
