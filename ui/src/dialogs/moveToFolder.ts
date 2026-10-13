/**
 * The board menu's Move to folder › New folder… prompt: names a new folder and moves one board into it.
 * It is this module's own request (the window's `DialogRequest` has no board for a folder), shown by
 * `DialogHost` like the others.
 */

import { create } from "zustand";
import type { BoardRef } from "../state/boards";

export interface MoveToFolderRequest {
  readonly board: BoardRef;
  /** The board's folder now (null: the top level). */
  readonly folder: string | null;
}

export const useMoveToFolder = create<{ readonly request: MoveToFolderRequest | null }>()(() => ({ request: null }));

export function openMoveToFolder(request: MoveToFolderRequest): void {
  useMoveToFolder.setState({ request });
}

export function closeMoveToFolder(): void {
  useMoveToFolder.setState({ request: null });
}
