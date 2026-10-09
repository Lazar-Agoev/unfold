/**
 * Palette state and quick-open providers. The command palette (⇧⌘P) lists registered actions;
 * quick open (⌘P files, ⌘T symbols) shows results from a provider that a view registers (Search
 * plugs in the `search` command).
 */

import { useEffect, type ReactNode } from "react";
import { create } from "zustand";

export type QuickOpenMode = "files" | "symbols";
export type PaletteMode = "commands" | QuickOpenMode;

/** One quick-open result. */
export interface QuickOpenItem {
  readonly id: string;
  readonly label: string;
  /** Quiet text on the right, e.g. `services/order-service.ts:13`. */
  readonly detail?: string;
  readonly icon?: ReactNode;
  /** Results with a group appear under its heading, in first-seen order. */
  readonly group?: string;
  /** Runs after the palette closes and focus has returned. */
  readonly run: () => void;
}

export interface QuickOpenProvider {
  readonly placeholder: string;
  /**
   * Results for `query` (an empty query may return recent items). Already ranked: the palette
   * doesn't filter or reorder them. Stop work when `signal` aborts; a newer query superseded it.
   */
  readonly search: (query: string, signal: AbortSignal) => Promise<readonly QuickOpenItem[]>;
  /** Shown when a query has no results. */
  readonly emptyMessage?: string;
}

interface PaletteState {
  readonly open: boolean;
  readonly mode: PaletteMode;
  /** The text the input starts with. */
  readonly initialQuery: string;
  /** Increments per opening, so each opening starts fresh. */
  readonly session: number;
  readonly providers: Readonly<Partial<Record<QuickOpenMode, QuickOpenProvider>>>;
}

export const usePalette = create<PaletteState>()(() => ({
  open: false,
  mode: "commands",
  initialQuery: "",
  session: 0,
  providers: {},
}));

/** Opens the palette in a mode. Asking for the mode that is already open closes it. */
export function openPalette(mode: PaletteMode = "commands", initialQuery = ""): void {
  usePalette.setState((state) =>
    state.open && state.mode === mode ? { open: false } : { open: true, mode, initialQuery, session: state.session + 1 },
  );
}

export function closePalette(): void {
  usePalette.setState({ open: false });
}

/** Switches an open palette to another mode, keeping it open. */
export function switchPaletteMode(mode: PaletteMode, initialQuery = ""): void {
  usePalette.setState((state) => ({ mode, initialQuery, session: state.session + 1 }));
}

/** Registers the result provider for a quick-open mode. Returns the unregister function. */
export function registerQuickOpenProvider(mode: QuickOpenMode, provider: QuickOpenProvider): () => void {
  usePalette.setState((state) => ({ providers: { ...state.providers, [mode]: provider } }));
  return () => {
    usePalette.setState((state) => {
      if (state.providers[mode] !== provider) {
        return state;
      }
      const { [mode]: _removed, ...providers } = state.providers;
      return { providers };
    });
  };
}

export function hasQuickOpenProvider(mode: QuickOpenMode): boolean {
  return usePalette.getState().providers[mode] !== undefined;
}

/**
 * Registers a quick-open provider while the component is mounted. Keep `provider` stable (memoize
 * it); a new object re-registers.
 */
export function useQuickOpenProvider(mode: QuickOpenMode, provider: QuickOpenProvider): void {
  useEffect(() => registerQuickOpenProvider(mode, provider), [mode, provider]);
}
