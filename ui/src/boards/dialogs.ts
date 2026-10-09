/**
 * Which board dialog is open (one at a time per window), and whether the title bar's board title is
 * being renamed. Menus, the palette, the Boards view, the inspector, and Problems open dialogs here;
 * `<BoardDialogs />` draws them.
 */

import { create } from "zustand";
import type { HealthItem, RepairKind } from "../ipc/generated";

export type BoardDialog =
  /** Publish: shows the file it will write. */
  | { readonly kind: "publish"; readonly boardId: string }
  /** The name is taken: Publish (or a published board's rename) under another name. */
  | { readonly kind: "name"; readonly boardId: string; readonly action: "publish" | "rename"; readonly path: string; readonly suggestion: string }
  | { readonly kind: "delete"; readonly boardId: string }
  /** One health item's repairs, each explained before it is applied. */
  | { readonly kind: "repair"; readonly boardId: string; readonly item: HealthItem; readonly repair?: RepairKind }
  /** Make a group from lines of a card (the keyboard path to the gutter drag). */
  | { readonly kind: "group"; readonly boardId: string; readonly cardId: string };

interface DialogState {
  readonly dialog: BoardDialog | null;
  /** The board whose title is being edited in the title bar. */
  readonly renaming: string | null;
}

export const useBoardDialogs = create<DialogState>()(() => ({ dialog: null, renaming: null }));

export function openBoardDialog(dialog: BoardDialog): void {
  useBoardDialogs.setState({ dialog });
}

export function closeBoardDialog(): void {
  useBoardDialogs.setState({ dialog: null });
}

/** Starts (or, with null, ends) renaming a board's title in the title bar. */
export function setRenaming(boardId: string | null): void {
  useBoardDialogs.setState({ renaming: boardId });
}
