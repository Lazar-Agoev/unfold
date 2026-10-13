/**
 * The sidebar in board mode: a scrolling region with the board's header, its Pages, and On this page,
 * over the Selected section pinned to the bottom.
 */

import { useMemo, useRef } from "react";
import { useOpenBoard, type BoardRef } from "../state/boards";
import { BoardHeader } from "./BoardHeader";
import { OnThisPage } from "./OnThisPage";
import { PagesSection } from "./PagesSection";
import { SelectedSection } from "./selected/SelectedSection";
import "./sidebar.css";

export function BoardSidebar({ board }: { readonly board: BoardRef }) {
  const scroller = useRef<HTMLDivElement>(null);
  const page = useOpenBoard(board, (open) => open.activePage);
  const { project, id } = board;
  const target = useMemo(() => (page !== undefined ? { board: { project, id }, page } : null), [project, id, page]);
  return (
    <div className="board-sidebar">
      <div ref={scroller} className="board-sidebar-scroll">
        <BoardHeader board={board} />
        <PagesSection board={board} />
        {target ? <OnThisPage target={target} scroller={scroller} /> : null}
      </div>
      {target ? <SelectedSection target={target} /> : null}
    </div>
  );
}
