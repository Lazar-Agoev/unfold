/**
 * A board's life on disk: its file in `.unfold/boards/`, saved as it is edited, and the edits the
 * journeys make to it.
 */

import { existsSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { readBoard, selected, type BoardFile } from "./board";
import { enterText } from "./window";

/** A project's folder of board files. */
export const BOARDS = join(".unfold", "boards");

/** Waits until the board file `id` exists and `check` holds for it (autosave is debounced). */
export async function expectBoardFile(folder: string, id: string, check: (board: BoardFile) => boolean, message: string): Promise<BoardFile> {
  let found: BoardFile | undefined;
  await expect
    .poll(
      () => {
        found = existsSync(join(folder, id)) ? readBoard(folder, id) : undefined;
        return found !== undefined && check(found);
      },
      { message },
    )
    .toBe(true);
  if (!found) {
    throw new Error(message);
  }
  return found;
}

/** Describes the selected card in Selected's Description field (one edit). */
export async function describeSelected(page: Page, text: string): Promise<void> {
  await enterText(selected(page).getByRole("textbox", { name: "Description" }), text);
}

/** The first description on the board's first page, if any. */
export function firstDescription(board: BoardFile): string | undefined {
  return board.pages[0]?.cards.find((card) => card.description !== undefined)?.description?.text;
}
