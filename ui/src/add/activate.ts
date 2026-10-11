/**
 * What Enter does with Add's selection: an entrance or a definition goes onto the current page (⌘Enter:
 * onto a new page after it; a type declaration as a Code card); from Home it starts a new draft in the
 * result's own project. A file opens in the peek sheet; a text match opens it at its line.
 */

import { showToast } from "../components";
import { toUnfoldError, type Key, type ProjectId, type TextMatch, type View } from "../ipc";
import { addBlock, addPage, getOpenBoard, newBoard, openEntrance, traceEntrance, type PageTarget } from "../state/boards";
import { openPeek, type AddTarget } from "../state/overlays";

export type AddChoice =
  | { readonly kind: "entrance"; readonly project: ProjectId; readonly key: Key }
  | { readonly kind: "definition"; readonly project: ProjectId; readonly key: Key; readonly name: string; readonly view?: View }
  | { readonly kind: "file"; readonly project: ProjectId; readonly path: string }
  | {
      readonly kind: "text";
      readonly project: ProjectId;
      readonly path: string;
      readonly matches: readonly TextMatch[];
      readonly current: number;
    };

/** The page an addition lands on: the target page, or a new page after it. */
function pageFor(target: PageTarget, newPage: boolean): PageTarget | null {
  if (!newPage) {
    return target;
  }
  const page = addPage(target.board);
  return page === undefined ? null : { board: target.board, page };
}

async function addToPage(choice: Extract<AddChoice, { kind: "entrance" | "definition" }>, page: PageTarget): Promise<void> {
  if (choice.kind === "entrance") {
    await openEntrance(page, choice.project, choice.key);
  } else {
    addBlock(page, choice.project, choice.key, choice.view);
  }
}

/** Starts a new draft in the result's project with it on the first page. */
async function addToNewDraft(choice: Extract<AddChoice, { kind: "entrance" | "definition" }>): Promise<void> {
  if (choice.kind === "entrance") {
    await traceEntrance(choice.project, choice.key);
    return;
  }
  const board = await newBoard(choice.project, { title: choice.name });
  const page = getOpenBoard(board)?.activePage;
  if (page !== undefined) {
    addBlock({ board, page }, choice.project, choice.key, choice.view);
  }
}

/** Carries out a choice; failures become a toast. */
export async function activateChoice(choice: AddChoice, target: AddTarget, newPage: boolean): Promise<void> {
  switch (choice.kind) {
    case "file":
      openPeek({ project: choice.project, path: choice.path });
      return;
    case "text": {
      const match = choice.matches[choice.current];
      openPeek({
        project: choice.project,
        path: choice.path,
        ...(match ? { line: match.line } : {}),
        matches: choice.matches,
        current: choice.current,
      });
      return;
    }
    case "entrance":
    case "definition":
      break;
  }
  try {
    if (target.kind === "home") {
      await addToNewDraft(choice);
      return;
    }
    const page = pageFor(target.target, newPage);
    if (page) {
      await addToPage(choice, page);
    }
  } catch (error) {
    showToast({ title: "Couldn't add it", body: toUnfoldError(error).message, tone: "danger" });
  }
}
