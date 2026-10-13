/**
 * What Enter does with Add's selection: an entrance or a definition goes onto the page (a type
 * declaration as a Diagram card, its fields); a file opens in the peek sheet.
 */

import { showToast } from "../components";
import { toUnfoldError, type Key, type View } from "../ipc";
import { addBlock, openEntrance, type PageTarget } from "../state/boards";
import { openPeek } from "../state/overlays";

export type AddChoice =
  | { readonly kind: "entrance"; readonly key: Key }
  | { readonly kind: "definition"; readonly key: Key; readonly view?: View }
  | { readonly kind: "file"; readonly path: string };

/** Carries out a choice on `page`; failures become a toast. */
export async function activateChoice(choice: AddChoice, page: PageTarget): Promise<void> {
  try {
    switch (choice.kind) {
      case "file":
        openPeek({ project: page.board.project, path: choice.path });
        return;
      case "entrance":
        await openEntrance(page, choice.key);
        return;
      case "definition":
        addBlock(page, choice.key, choice.view);
        return;
    }
  } catch (error) {
    showToast({ title: "Couldn't add it", body: toUnfoldError(error).message, tone: "danger" });
  }
}
