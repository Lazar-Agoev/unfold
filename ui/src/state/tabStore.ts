/**
 * The window's tab strip as data: Home, implicit, always first and never closable, then
 * the board tabs in order, one of them active. The board store opens and renames board tabs here;
 * `tabs.ts` adds closing (which needs the board store) and is what screens import.
 */

import { create } from "zustand";
import { UnfoldError } from "../ipc/unfold";
import { boardKey, sameBoard } from "./boards/keys";
import type { BoardRef } from "./boards/types";

/** `"home"`, or a board tab's `boardKey(ref)`. */
export type TabKey = "home" | (string & {});

export interface TabsState {
  readonly boards: readonly BoardRef[];
  readonly active: TabKey;
}

/** A window holds at most this many board tabs. */
export const MAX_BOARD_TABS = 100;

export const useTabs = create<TabsState>()(() => ({ boards: [], active: "home" }));

function indexOf(state: TabsState, key: TabKey): number {
  return state.boards.findIndex((ref) => boardKey(ref) === key);
}

function hasBoardTab(ref: BoardRef): boolean {
  return indexOf(useTabs.getState(), boardKey(ref)) >= 0;
}

/** Rejects (code `too_many_tabs`) when another board tab wouldn't fit; call before claiming a board. */
export function assertTabRoom(ref?: BoardRef): void {
  if ((ref === undefined || !hasBoardTab(ref)) && useTabs.getState().boards.length >= MAX_BOARD_TABS) {
    throw new UnfoldError({
      code: "too_many_tabs",
      message: `A window holds at most ${MAX_BOARD_TABS} board tabs. Close one to open another.`,
    });
  }
}

/** Adds a board tab (at `index`, else last) or finds it, and activates it unless `activate` is false. */
export function openBoardTab(ref: BoardRef, options: { readonly activate?: boolean; readonly index?: number } = {}): void {
  const key = boardKey(ref);
  useTabs.setState((state) => {
    const boards = indexOf(state, key) >= 0 ? state.boards : state.boards.toSpliced(options.index ?? state.boards.length, 0, ref);
    return { boards, active: (options.activate ?? true) ? key : state.active };
  });
}

/** Removes a board tab without asking anyone; the neighbour to its right (or left, or Home) becomes active. */
export function removeBoardTab(ref: BoardRef): void {
  const key = boardKey(ref);
  useTabs.setState((state) => {
    const index = indexOf(state, key);
    if (index < 0) {
      return state;
    }
    const boards = state.boards.toSpliced(index, 1);
    const neighbour = boards[index] ?? boards[index - 1];
    const active = state.active !== key ? state.active : neighbour ? boardKey(neighbour) : "home";
    return { boards, active };
  });
}

/** A board's id changed (Publish, Move back to drafts): its tab keeps its place and activity. */
export function renameBoardTab(from: BoardRef, to: BoardRef): void {
  const oldKey = boardKey(from);
  const newKey = boardKey(to);
  useTabs.setState((state) => {
    const index = indexOf(state, oldKey);
    if (index < 0 || oldKey === newKey) {
      return state;
    }
    const boards = state.boards.filter((ref) => !sameBoard(ref, to)).map((ref) => (sameBoard(ref, from) ? to : ref));
    return { boards, active: state.active === oldKey ? newKey : state.active };
  });
}

export function activateHome(): void {
  useTabs.setState({ active: "home" });
}

/** Activates a board's tab if it has one. */
export function activateBoard(ref: BoardRef): void {
  activateTab(boardKey(ref));
}

function activateTab(key: TabKey): void {
  useTabs.setState((state) => (key === "home" || indexOf(state, key) >= 0 ? { active: key } : state));
}

/** Moves a board tab to `index` among the board tabs (Home stays first). */
export function moveTab(key: TabKey, index: number): void {
  useTabs.setState((state) => {
    const from = indexOf(state, key);
    const ref = state.boards[from];
    if (!ref) {
      return state;
    }
    const to = Math.max(0, Math.min(state.boards.length - 1, index));
    return { boards: state.boards.toSpliced(from, 1).toSpliced(to, 0, ref) };
  });
}

function step(delta: 1 | -1): void {
  const state = useTabs.getState();
  const keys: TabKey[] = ["home", ...state.boards.map(boardKey)];
  const at = keys.indexOf(state.active);
  const next = keys[(at + delta + keys.length) % keys.length];
  if (next !== undefined) {
    activateTab(next);
  }
}

/** Activates the next tab, wrapping around to Home. */
export function nextTab(): void {
  step(1);
}

export function previousTab(): void {
  step(-1);
}

function activeRef(state: TabsState): BoardRef | null {
  return state.active === "home" ? null : (state.boards[indexOf(state, state.active)] ?? null);
}

/** The active board tab's board, or null on Home. */
export function getActiveBoard(): BoardRef | null {
  return activeRef(useTabs.getState());
}

/** The active board tab's board (null on Home), kept current. The ref keeps its identity while active. */
export function useActiveBoard(): BoardRef | null {
  return useTabs(activeRef);
}
