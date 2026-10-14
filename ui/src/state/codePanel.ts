/**
 * The code panel (docs/ui-shell.md#the-code-panel): whether it is open, what it shows, and the line the
 * pointer is on in it. It belongs to the board it opened on and closes when the window shows another
 * board, or Home. None of this is saved.
 *
 * While it is open, the panel follows the page's selection. A selection the panel makes itself (a click
 * on a line, `⊕`) is made `quietly`, so the panel stays where it is.
 *
 * Lines opened from a drawn flow (a combined step, a row of other calls, a gap tick) keep what they were
 * opened from (`CodeOpen.from`), and the panel rebuilds their lines from the flow as it is drawn now
 * (`retargetCode`): line numbers taken at the click would go stale after a compile.
 */

import { create } from "zustand";
import type { CodeTarget, Key, ProjectId, RepoPath } from "../ipc/generated";
import { sameBoard } from "./boards/keys";
import { getPageView } from "./boards/store";
import type { PageTarget } from "./boards/types";
import { useShownBoardStore } from "./shownBoard";

/** One line of a file. */
interface CodeLineRef {
  readonly path: RepoPath;
  readonly line: number;
}

/**
 * Where a gap tick sits in a card's flow: on the route into a step (by its block's key, a row's first
 * call's, or a combined or proposed step's id), or at the end of a body (by its definition's or part's
 * key, or its open combined step's id).
 */
export type GapPlace = { readonly before: string } | { readonly end: string };

/** Lines opened from a drawn flow, by what draws them in the card: a combined step, a row of other calls (by its first call), or a gap tick. */
export type CodeSource =
  | { readonly kind: "combined"; readonly item: string }
  | { readonly kind: "row"; readonly key: Key }
  | { readonly kind: "gap"; readonly place: GapPlace };

/** What the panel shows. */
export interface CodeOpen {
  readonly project: ProjectId;
  /** The selection: it heads the path and stays marked. */
  readonly target: CodeTarget;
  /** What the path widened the view to; null shows the selection's own lines. */
  readonly show: CodeTarget | null;
  /** The card the selection is in (or that draws `from`): its repair actions show when its code isn't found. */
  readonly cardId: string | null;
  /** What in card `cardId` drew the lines it was opened on, which rebuilds them as the card is drawn now. */
  readonly from?: CodeSource;
}

/**
 * The line the pointer is on in the panel: its block (a call's, when on the call's name), or none; and
 * the line's own block, which lights when that call isn't drawn on its own.
 */
export interface CodeHover extends CodeLineRef {
  readonly key: Key | null;
  readonly block: Key | null;
}

interface CodePanelState {
  readonly open: CodeOpen | null;
  readonly hover: CodeHover | null;
}

const useCodePanelStore = create<CodePanelState>()(() => ({ open: null, hover: null }));

/** Opens the panel on a selection (or moves it there), showing the selection's own lines. */
export function openCode(open: Omit<CodeOpen, "show">): void {
  useCodePanelStore.setState({ open: { ...open, show: null }, hover: null });
}

/** Widens the view to a step of the path (null: back to the selection's own lines). */
export function widenCode(show: CodeTarget | null): void {
  const { open } = useCodePanelStore.getState();
  if (open) {
    useCodePanelStore.setState({ open: { ...open, show } });
  }
}

/** Points the panel at its `from` as the flow draws it now. */
export function retargetCode(target: CodeTarget): void {
  const { open } = useCodePanelStore.getState();
  if (open && JSON.stringify(open.target) !== JSON.stringify(target)) {
    useCodePanelStore.setState({ open: { ...open, target } });
  }
}

/** The gap tick `open` shows the lines of, by its first line: outlined on the canvas while nothing is selected. */
export function openGap(open: CodeOpen): CodeLineRef | null {
  return open.from?.kind === "gap" && open.target.kind === "lines" ? { path: open.target.path, line: open.target.lines[0] } : null;
}

export function closeCode(): void {
  useCodePanelStore.setState({ open: null, hover: null });
}

export function setCodeHover(hover: CodeHover | null): void {
  const current = useCodePanelStore.getState().hover;
  if (current?.key !== hover?.key || current?.block !== hover?.block || current?.line !== hover?.line || current?.path !== hover?.path) {
    useCodePanelStore.setState({ hover });
  }
}

/** What the panel shows now (null while it is closed), outside React. */
export function getCodeOpen(): CodeOpen | null {
  return useCodePanelStore.getState().open;
}

/** What the panel shows (null while it is closed), kept current. */
export function useCodePanel(): CodeOpen | null {
  return useCodePanelStore((state) => state.open);
}

/** Calls `listener` when what the panel shows changes (it opens, moves, widens, or closes); returns the unsubscribe function. */
export function onCodePanelChange(listener: () => void): () => void {
  return useCodePanelStore.subscribe((state, previous) => {
    if (state.open !== previous.open) {
      listener();
    }
  });
}

/** The line hovered in the panel, kept current. */
export function useCodeHover(): CodeHover | null {
  return useCodePanelStore((state) => state.hover);
}

// The panel belongs to the board it opened on.
useShownBoardStore.subscribe((state, previous) => {
  if (!sameBoard(state.board, previous.board)) {
    closeCode();
  }
});

let held = false;

/**
 * Runs `change` (which may select something on the page) so that the panel doesn't follow the
 * selection it makes.
 */
export function quietly(target: PageTarget, change: () => void): void {
  const before = JSON.stringify(getPageView(target).selection);
  held = true;
  change();
  if (JSON.stringify(getPageView(target).selection) === before) {
    held = false;
  }
}

/** Whether the selection that just changed was made `quietly` (and lets go of it). */
export function takeQuiet(): boolean {
  const was = held;
  held = false;
  return was;
}
