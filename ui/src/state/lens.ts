/**
 * The window's Code/Flow lens (architecture §11.1): a window-level switch in the title bar. Cards
 * without their own view follow it. It is saved with the window's layout.
 */

import { create } from "zustand";
import type { View } from "../ipc/generated";

interface LensState {
  readonly lens: View;
}

const useLensStore = create<LensState>()(() => ({ lens: "code" }));

/** The current lens, kept current. */
export function useLens(): View {
  return useLensStore((state) => state.lens);
}

/** The current lens, outside React. */
export function getLens(): View {
  return useLensStore.getState().lens;
}

export function setLens(lens: View): void {
  useLensStore.setState({ lens });
}

/** Restores a saved lens; anything but `code` or `flow` is ignored. */
export function applyLens(saved: unknown): void {
  if (saved === "code" || saved === "flow") {
    setLens(saved);
  }
}

/** Calls `onChange` whenever the lens changes. */
export function subscribeToLens(onChange: (lens: View) => void): () => void {
  return useLensStore.subscribe((state, previous) => {
    if (state.lens !== previous.lens) {
      onChange(state.lens);
    }
  });
}
