/**
 * The window's selection: the definition or entrance the person is looking at. The canvas sets it
 * when a card is selected (`select({ key })`); the References panel and the inspector read it.
 */

import { create } from "zustand";

export interface Selection {
  /** A block or entrance key. */
  readonly key: string;
}

interface SelectionState {
  readonly selection: Selection | null;
}

const useSelectionStore = create<SelectionState>()(() => ({ selection: null }));

/** The current selection, kept current; null when nothing is selected. */
export function useSelection(): Selection | null {
  return useSelectionStore((state) => state.selection);
}

/** The current selection, outside React. */
export function getSelection(): Selection | null {
  return useSelectionStore.getState().selection;
}

/** Selects a key, or clears the selection with `null`. */
export function select(selection: Selection | null): void {
  const current = useSelectionStore.getState().selection;
  if (current?.key === selection?.key) {
    return;
  }
  useSelectionStore.setState({ selection: selection ? { key: selection.key } : null });
}
