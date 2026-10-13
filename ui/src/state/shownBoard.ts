/**
 * What the window shows: Home, or one board. Every board opened while the app runs stays open (its
 * session and undo history live in the board store); this says which one is in front. The main
 * process remembers it (`report_shown_board`), so the window comes back to it at launch.
 */

import { create } from "zustand";
import { windowCommand } from "../ipc/unfold";
import { sameBoard } from "./boards/keys";
import type { BoardRef } from "./boards/types";

interface ShownBoardState {
  /** The board in front, or null for Home. */
  readonly board: BoardRef | null;
}

export const useShownBoardStore = create<ShownBoardState>()(() => ({ board: null }));

export function showHome(): void {
  useShownBoardStore.setState({ board: null });
}

/** Puts an open board in front (`openBoard` opens one that isn't open yet, then shows it). */
export function showBoard(ref: BoardRef): void {
  if (!sameBoard(useShownBoardStore.getState().board, ref)) {
    useShownBoardStore.setState({ board: ref });
  }
}

/** A board closed or was deleted: if it was in front, Home shows instead. */
export function hideBoard(ref: BoardRef): void {
  if (sameBoard(useShownBoardStore.getState().board, ref)) {
    showHome();
  }
}

/** The board in front, or null on Home. */
export function getShownBoard(): BoardRef | null {
  return useShownBoardStore.getState().board;
}

/** The board in front (null on Home), kept current. The ref keeps its identity while it is shown. */
export function useShownBoard(): BoardRef | null {
  return useShownBoardStore((state) => state.board);
}

async function report(board: BoardRef | null): Promise<void> {
  try {
    await windowCommand("report_shown_board", { board: board === null ? null : { project: board.project, board: board.id } });
  } catch (error) {
    console.warn("Reporting the shown board failed:", error);
  }
}

/** Tells the main process what the window shows now, and again whenever that changes. Returns the stop function. */
export function reportShownBoard(): () => void {
  void report(getShownBoard());
  return useShownBoardStore.subscribe((state, previous) => {
    if (!sameBoard(state.board, previous.board) && (state.board !== null || previous.board !== null)) {
      void report(state.board);
    }
  });
}
