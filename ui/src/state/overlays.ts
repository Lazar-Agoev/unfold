/**
 * What the window shows over its content: the Add box, the peek sheet, and one dialog
 * at a time. Each is a request any screen or action can make; the hosts (`AddBox`, `PeekSheet`,
 * `DialogHost`) render them. None of this is saved with the window.
 */

import { useMemo } from "react";
import { create } from "zustand";
import type { ProjectId } from "../ipc/generated";
import { boardKey } from "./boards/keys";
import { useBoards } from "./boards/store";
import type { BoardRef, PageTarget } from "./boards/types";
import { useShownBoard } from "./shownBoard";

/**
 * The source View code highlights: one-based lines `startLine`–`endLine`, and with columns (UTF-16
 * offsets from each line's start, the end exclusive) the exact expression from `startColumn` on its
 * first line to `endColumn` on its last.
 */
export interface PeekFocus {
  readonly startLine: number;
  readonly endLine: number;
  readonly startColumn?: number;
  readonly endColumn?: number;
}

/** A file shown in the peek sheet, optionally at a line. */
export interface PeekTarget {
  readonly project: ProjectId;
  readonly path: string;
  readonly line?: number;
  /** What View code highlights (only on source that verified against the graph). */
  readonly focus?: PeekFocus;
  /** The page whose selection the sheet follows, when View code opened it from that page. */
  readonly follow?: PageTarget;
  /**
   * Where View code finds this source again on the card models as they are now (null while they
   * refresh, or when the item has no source any more). The sheet draws `focus` only while this agrees.
   */
  readonly resolve?: () => PeekTarget | null;
}

export type DialogRequest =
  | { readonly kind: "settings" }
  | { readonly kind: "deleteBoard"; readonly board: BoardRef }
  | { readonly kind: "renameFolder"; readonly project: ProjectId; readonly folder: string }
  /** Combine…: the title of a combined block made from the statements of a run already chosen. */
  | { readonly kind: "combine"; readonly target: PageTarget; readonly cardId: string; readonly members: readonly string[] };

interface OverlaysState {
  readonly add: boolean;
  readonly peek: PeekTarget | null;
  readonly dialog: DialogRequest | null;
}

const useOverlays = create<OverlaysState>()(() => ({ add: false, peek: null, dialog: null }));

export function openAdd(): void {
  useOverlays.setState({ add: true });
}

export function closeAdd(): void {
  useOverlays.setState({ add: false });
}

export function useAddOpen(): boolean {
  return useOverlays((state) => state.add);
}

/** The page Add adds to, kept current: the shown board's active page (null on Home). */
export function useAddPage(): PageTarget | null {
  const board = useShownBoard();
  const page = useBoards((state) => (board ? state.boards[boardKey(board)]?.activePage : undefined));
  return useMemo(() => (board && page !== undefined ? { board, page } : null), [board, page]);
}

/** Shows a file in the peek sheet (one per window, kept when the window shows another board). */
export function openPeek(target: PeekTarget): void {
  useOverlays.setState({ peek: target });
}

export function closePeek(): void {
  useOverlays.setState({ peek: null });
}

export function usePeekTarget(): PeekTarget | null {
  return useOverlays((state) => state.peek);
}

/** What the peek sheet shows now, outside React. */
export function getPeekTarget(): PeekTarget | null {
  return useOverlays.getState().peek;
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
