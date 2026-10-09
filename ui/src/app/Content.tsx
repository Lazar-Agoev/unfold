/**
 * The content area (`#content-area`, a positioned flex row): Home's main area while Home is active, else
 * the active board's view (only that board is mounted, V-46), with the peek sheet as its last child. The
 * window's title follows what it shows.
 */

import { useEffect } from "react";
import { BoardView } from "../board";
import { HomeMain } from "../home";
import { PeekSheet } from "../peek";
import { boardKey, useOpenBoard, type BoardRef } from "../state/boards";
import { useActiveBoard } from "../state/tabs";
import { useSelectedProject } from "../state/window";
import { useProjectRow } from "../state/workspace";

/** "<subject> — Unfold", as the Window menu and the Dock list the window. */
function useWindowTitle(subject: string | undefined): void {
  useEffect(() => {
    document.title = subject ? `${subject} — Unfold` : "Unfold";
  }, [subject]);
}

function HomeContent() {
  useWindowTitle(useProjectRow(useSelectedProject())?.name);
  return <HomeMain />;
}

function BoardContent({ board }: { readonly board: BoardRef }) {
  useWindowTitle(useOpenBoard(board, (open) => open.document.title));
  return <BoardView board={board} />;
}

export function Content() {
  const board = useActiveBoard();
  return (
    <main id="content-area" className="app-content">
      {board ? <BoardContent key={boardKey(board)} board={board} /> : <HomeContent />}
      <PeekSheet />
    </main>
  );
}
