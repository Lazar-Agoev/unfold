/**
 * The ways into the code panel (docs/designs/viewing-code.md, "Opening and closing"): `</>` and View
 * code on a block, a card, or a row of other calls, a gap tick, a file from Add, and a type name in
 * Selected.
 */

import type { CardView, Key, ProjectId, RepoPath, ThinRow } from "../ipc/generated";
import { findPage, getOpenBoard, select, type PageTarget, type Selection } from "../state/boards";
import { openCode, quietly } from "../state/codePanel";
import { rowTarget, selectionCode } from "./targets";

/** Selects `selection` on the page and opens its code in the panel (`views`: what the page's cards draw). */
export function viewCode(target: PageTarget, selection: Selection, views: ReadonlyMap<string, CardView>): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const choice = page ? selectionCode(selection, page, views) : null;
  if (!choice) {
    return;
  }
  quietly(target, () => {
    select(target, selection);
  });
  openCode({ project: target.board.project, target: choice.target, cardId: choice.cardId, gap: null });
}

/** Opens the lines a gap tick counts; the tick is outlined, and nothing else stays selected. */
export function viewLines(target: PageTarget, path: RepoPath, lines: readonly [number, number]): void {
  select(target, { kind: "none" });
  openCode({ project: target.board.project, target: { kind: "lines", path, lines: [...lines] }, cardId: null, gap: { path, line: lines[0] } });
}

/** Opens a row of other calls in card `cardId` (in the file `path` of the flow drawing it); a row is no selection, so nothing stays selected. */
export function viewRow(target: PageTarget, cardId: string, path: RepoPath, row: ThinRow): void {
  select(target, { kind: "none" });
  openCode({ project: target.board.project, target: rowTarget(path, row), cardId, gap: null });
}

/** Opens a whole file (from Add). */
export function viewFile(project: ProjectId, path: RepoPath): void {
  openCode({ project, target: { kind: "file", path }, cardId: null, gap: null });
}

/** Opens a definition (a type name in Selected). */
export function viewDefinition(project: ProjectId, key: Key): void {
  openCode({ project, target: { kind: "block", key }, cardId: null, gap: null });
}
