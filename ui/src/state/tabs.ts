/**
 * The editor area's tabs (architecture §11.4): boards and read-only source files, in order, with
 * one active. Saved with the window (`window_state.tabs`) as plain JSON and restored on reopen.
 *
 * Board tabs are opened by the board store (`openBoardTab`) once it holds the board. Closing one
 * asks the board store first (`setBoardTabCloser`) so it can save and release the board.
 */

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { JsonValue } from "../ipc/generated";

export interface BoardTab {
  readonly kind: "board";
  readonly id: string;
  /** The board's ID (its folder-relative file path). */
  readonly boardId: string;
}

export interface SourceTab {
  readonly kind: "source";
  readonly id: string;
  /** The folder-relative file. */
  readonly path: string;
  /** The line to reveal, if any. */
  readonly line: number | null;
  /** Increments on every reveal request, so asking for the same line again scrolls again. */
  readonly reveal: number;
}

export type Tab = BoardTab | SourceTab;

export interface TabsState {
  readonly tabs: readonly Tab[];
  readonly activeId: string | null;
}

export const boardTabId = (boardId: string): string => `board:${boardId}`;
export const sourceTabId = (path: string): string => `source:${path}`;

export const createTabsStore = () =>
  create<TabsState>()(
    immer<TabsState>(() => ({ tabs: [], activeId: null })),
  );

export type TabsStore = ReturnType<typeof createTabsStore>;

export const useTabs = createTabsStore();

/** Opens (or focuses) a source tab, revealing `line` when given. */
export function openSource(path: string, line: number | null = null, store: TabsStore = useTabs): void {
  const id = sourceTabId(path);
  store.setState((state) => {
    const existing = state.tabs.find((tab) => tab.id === id);
    if (existing?.kind === "source") {
      existing.line = line ?? existing.line;
      existing.reveal += line === null ? 0 : 1;
    } else {
      const tab: SourceTab = { kind: "source", id, path, line, reveal: 0 };
      const at = state.tabs.findIndex((entry) => entry.id === state.activeId);
      (state.tabs as Tab[]).splice(at < 0 ? state.tabs.length : at + 1, 0, tab);
    }
    state.activeId = id;
  });
}

/** Opens (or focuses) a board tab. */
export function openBoardTab(boardId: string, store: TabsStore = useTabs): void {
  const id = boardTabId(boardId);
  store.setState((state) => {
    if (!state.tabs.some((tab) => tab.id === id)) {
      (state.tabs as Tab[]).push({ kind: "board", id, boardId });
    }
    state.activeId = id;
  });
}

/** A board's ID changed (publish, rename): its tab follows. */
export function renameBoardTab(from: string, to: string, store: TabsStore = useTabs): void {
  const oldId = boardTabId(from);
  const newId = boardTabId(to);
  store.setState((state) => {
    if (state.tabs.every((tab) => tab.id !== oldId)) {
      return;
    }
    // The renamed board may already have announced itself under its new ID; keep the old place.
    const tabs = state.tabs.filter((tab) => tab.id !== newId) as Tab[];
    const index = tabs.findIndex((tab) => tab.id === oldId);
    tabs[index] = { kind: "board", id: newId, boardId: to };
    state.tabs = tabs;
    if (state.activeId === oldId) {
      state.activeId = newId;
    }
  });
}

export function activateTab(id: string, store: TabsStore = useTabs): void {
  store.setState((state) => {
    if (state.tabs.some((tab) => tab.id === id)) {
      state.activeId = id;
    }
  });
}

/** Removes a tab without asking anyone; the neighbour to its right (or left) becomes active. */
export function removeTab(id: string, store: TabsStore = useTabs): void {
  store.setState((state) => {
    const index = state.tabs.findIndex((tab) => tab.id === id);
    if (index < 0) {
      return;
    }
    (state.tabs as Tab[]).splice(index, 1);
    if (state.activeId === id) {
      state.activeId = (state.tabs[index] ?? state.tabs[index - 1])?.id ?? null;
    }
  });
}

type BoardTabCloser = (boardId: string) => Promise<boolean> | boolean;
let boardTabCloser: BoardTabCloser | null = null;

/**
 * Registers what runs before a board tab closes (save, then `close_board`). It resolves false to
 * keep the tab open. Returns the unregister function.
 */
export function setBoardTabCloser(closer: BoardTabCloser): () => void {
  boardTabCloser = closer;
  return () => {
    if (boardTabCloser === closer) {
      boardTabCloser = null;
    }
  };
}

/** Closes a tab; a board tab asks the board store first. */
export async function closeTab(id: string, store: TabsStore = useTabs): Promise<void> {
  const tab = store.getState().tabs.find((entry) => entry.id === id);
  if (!tab) {
    return;
  }
  if (tab.kind === "board" && boardTabCloser && !(await boardTabCloser(tab.boardId))) {
    return;
  }
  removeTab(id, store);
}

/** The active tab, outside React. */
export function activeTab(store: TabsStore = useTabs): Tab | null {
  const { tabs, activeId } = store.getState();
  return tabs.find((tab) => tab.id === activeId) ?? null;
}

/** The active tab, kept current. */
export function useActiveTab(): Tab | null {
  return useTabs((state) => state.tabs.find((tab) => tab.id === state.activeId) ?? null);
}

/** What `window_state` remembers about the tabs. */
export interface TabsSnapshot {
  readonly tabs: JsonValue[];
  readonly activeTab: number | null;
  readonly openBoards: string[];
}

export function tabsSnapshot(store: TabsStore = useTabs): TabsSnapshot {
  const { tabs, activeId } = store.getState();
  const index = tabs.findIndex((tab) => tab.id === activeId);
  return {
    tabs: tabs.map((tab): JsonValue =>
      tab.kind === "board" ? { kind: "board", board: tab.boardId } : { kind: "source", path: tab.path, line: tab.line },
    ),
    activeTab: index < 0 ? null : index,
    openBoards: tabs.flatMap((tab) => (tab.kind === "board" ? [tab.boardId] : [])),
  };
}

function parseTab(value: JsonValue): Tab | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  const { kind, board, path, line } = value;
  if (kind === "board" && typeof board === "string" && board.length > 0) {
    return { kind: "board", id: boardTabId(board), boardId: board };
  }
  if (kind === "source" && typeof path === "string" && path.length > 0) {
    const revealLine = typeof line === "number" && Number.isInteger(line) && line > 0 ? line : null;
    return { kind: "source", id: sourceTabId(path), path, line: revealLine, reveal: 0 };
  }
  return null;
}

/**
 * Restores saved tabs. The input comes from disk: malformed entries and duplicates are dropped,
 * and an out-of-range active index falls back to the first tab.
 */
export function applyTabsSnapshot(saved: readonly JsonValue[], activeIndex: number | null, store: TabsStore = useTabs): void {
  const seen = new Set<string>();
  const tabs: Tab[] = [];
  let activeId: string | null = null;
  for (const [index, value] of saved.entries()) {
    const tab = parseTab(value);
    if (!tab || seen.has(tab.id)) {
      continue;
    }
    seen.add(tab.id);
    tabs.push(tab);
    if (index === activeIndex) {
      activeId = tab.id;
    }
  }
  store.setState({ tabs, activeId: activeId ?? tabs[0]?.id ?? null });
}

/** Calls `onChange` whenever the saved part of the tabs changes. */
export function subscribeToTabs(onChange: (snapshot: TabsSnapshot) => void, store: TabsStore = useTabs): () => void {
  let previous = JSON.stringify(tabsSnapshot(store));
  return store.subscribe(() => {
    const snapshot = tabsSnapshot(store);
    const serialized = JSON.stringify(snapshot);
    if (serialized !== previous) {
      previous = serialized;
      onChange(snapshot);
    }
  });
}
