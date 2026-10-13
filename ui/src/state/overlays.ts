/**
 * What the window shows over its content: the Add box, the peek sheet, and one dialog
 * at a time. Each is a request any screen or action can make; the hosts (`AddBox`, `PeekSheet`,
 * `DialogHost`) render them. None of this is saved with the window.
 */

import { create } from "zustand";
import type { ProjectId, TextMatch } from "../ipc/generated";
import { boardKey } from "./boards/keys";
import { useBoards } from "./boards/store";
import type { BoardRef, PageTarget } from "./boards/types";
import { useShownBoard } from "./shownBoard";
import { useWindowStore } from "./window";

/** Where Add puts what it adds: the shown board's active page, or Home with the selected project. */
export type AddTarget = { readonly kind: "page"; readonly target: PageTarget } | { readonly kind: "home"; readonly project: ProjectId | null };

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

/** A file shown in the peek sheet, optionally at a line, with text matches to step through. */
export interface PeekTarget {
  readonly project: ProjectId;
  readonly path: string;
  readonly line?: number;
  readonly matches?: readonly TextMatch[];
  /** The index of the current match in `matches`. */
  readonly current?: number;
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
  | { readonly kind: "publish"; readonly board: BoardRef }
  | { readonly kind: "discard"; readonly board: BoardRef }
  | { readonly kind: "deleteBoard"; readonly board: BoardRef }
  | { readonly kind: "newFolder"; readonly project: ProjectId; readonly parent: string | null }
  | { readonly kind: "renameFolder"; readonly project: ProjectId; readonly path: string }
  /** Make a group from lines, or with `members` (Combine…) the statements of a run already chosen. */
  | { readonly kind: "groupFromLines"; readonly target: PageTarget; readonly cardId: string; readonly members?: readonly string[] };

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

/** Where Add adds, kept current: the shown board's active page, else Home and its selected project. */
export function useAddTarget(): AddTarget {
  const board = useShownBoard();
  const page = useBoards((state) => (board ? state.boards[boardKey(board)]?.activePage : undefined));
  const project = useWindowStore((state) => state.selectedProject);
  if (board && page !== undefined) {
    return { kind: "page", target: { board, page } };
  }
  return { kind: "home", project };
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
