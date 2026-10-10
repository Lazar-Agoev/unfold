/**
 * The sidebar in board mode: a scrolling region with the board's header, its Pages, and On this page,
 * over the Selected section pinned to the bottom. It shares the board's undo scope, so ⌘Z after an edit
 * made here undoes it.
 */

import { useMemo, useRef } from "react";
import { useCommandScope } from "../keyboard";
import { redo, undo, useOpenBoard, type BoardRef } from "../state/boards";
import { BoardHeader } from "./BoardHeader";
import { OnThisPage } from "./OnThisPage";
import { PagesSection } from "./PagesSection";
import { SelectedSection } from "./selected/SelectedSection";
import "./sidebar.css";

export function BoardSidebar({ board }: { readonly board: BoardRef }) {
  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const page = useOpenBoard(board, (open) => open.activePage);
  const undoable = useOpenBoard(board, (open) => open.history.past.length > 0) ?? false;
  const redoable = useOpenBoard(board, (open) => open.history.future.length > 0) ?? false;
  const { project, id } = board;
  const target = useMemo(() => (page !== undefined ? { board: { project, id }, page } : null), [project, id, page]);
  useCommandScope(root, {
    undo: undoable ? () => undo(board) : undefined,
    redo: redoable ? () => redo(board) : undefined,
  });
  return (
    <div ref={root} className="board-sidebar">
      <div ref={scroller} className="board-sidebar-scroll">
        <BoardHeader board={board} />
        <PagesSection board={board} />
        {target ? <OnThisPage target={target} scroller={scroller} /> : null}
      </div>
      {target ? <SelectedSection target={target} /> : null}
    </div>
  );
}
