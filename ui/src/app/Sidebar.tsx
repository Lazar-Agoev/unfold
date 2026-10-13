/**
 * The sidebar container, 264px wide: the window's buttons over its top, which moves the window, then
 * the Home-mode sidebar while Home shows, else the shown board's sidebar.
 */

import { BoardSidebar } from "../board";
import { HomeSidebar } from "../home";
import { boardKey } from "../state/boards";
import { useShownBoard } from "../state/shownBoard";

export function Sidebar() {
  const board = useShownBoard();
  return (
    <aside id="sidebar" aria-label="Sidebar" className="app-sidebar">
      <div className="app-window-bar" />
      {board ? <BoardSidebar key={boardKey(board)} board={board} /> : <HomeSidebar />}
    </aside>
  );
}
