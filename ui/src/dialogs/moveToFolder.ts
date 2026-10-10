/**
 * The Move to folder prompt: the board menu's "New folder…" and Add's "Move Board to Folder…" name a
 * folder for one board. It is this module's own request (the window's `DialogRequest` has no board for
 * a folder), shown by `DialogHost` like the others.
 */

import { create } from "zustand";
import type { BoardRef } from "../state/boards";

export interface MoveToFolderRequest {
  readonly board: BoardRef;
  /** The board's folder now (null: the top level). */
  readonly folder: string | null;
  /** `new` asks for a new folder (a path from the top level); `path` for any folder, the current one filled in. */
  readonly mode: "new" | "path";
}

export const useMoveToFolder = create<{ readonly request: MoveToFolderRequest | null }>()(() => ({ request: null }));

export function openMoveToFolder(request: MoveToFolderRequest): void {
  useMoveToFolder.setState({ request });
}

export function closeMoveToFolder(): void {
  useMoveToFolder.setState({ request: null });
}
