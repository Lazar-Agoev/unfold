/**
 * What the window shows over its content: the Add box and one dialog at a time. Each is a request any
 * screen or action can make; the hosts (`AddBox`, `DialogHost`) render them. None of this is saved with
 * the window.
 */

import { useMemo } from "react";
import { create } from "zustand";
import type { ProjectId } from "../ipc/generated";
import { boardKey, sameBoard } from "./boards/keys";
import { useBoards } from "./boards/store";
import type { BoardRef, PageTarget } from "./boards/types";
import { useShownBoard, useShownBoardStore } from "./shownBoard";

export type DialogRequest =
  | { readonly kind: "settings" }
  | { readonly kind: "deleteBoard"; readonly board: BoardRef }
  | { readonly kind: "renameFolder"; readonly project: ProjectId; readonly folder: string };

interface OverlaysState {
  readonly add: boolean;
  readonly dialog: DialogRequest | null;
}

const useOverlays = create<OverlaysState>()(() => ({ add: false, dialog: null }));

export function openAdd(): void {
  useOverlays.setState({ add: true });
}

export function closeAdd(): void {
  useOverlays.setState({ add: false });
}

// Add belongs to the board it opened on: it closes when the window shows another board, or Home.
useShownBoardStore.subscribe((state, previous) => {
  if (!sameBoard(state.board, previous.board)) {
    closeAdd();
  }
});

export function useAddOpen(): boolean {
  return useOverlays((state) => state.add);
}

/** The page Add adds to, kept current: the shown board's active page (null on Home). */
export function useAddPage(): PageTarget | null {
  const board = useShownBoard();
  const page = useBoards((state) => (board ? state.boards[boardKey(board)]?.activePage : undefined));
  return useMemo(() => (board && page !== undefined ? { board, page } : null), [board, page]);
}

/** Opens a dialog, replacing any open one. */
export function openDialog(request: DialogRequest): void {
  useOverlays.setState({ dialog: request });
}

export function closeDialog(): void {
  useOverlays.setState({ dialog: null });
}

export function useDialog(): DialogRequest | null {
  return useOverlays((state) => state.dialog);
}
