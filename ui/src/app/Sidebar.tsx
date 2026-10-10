/**
 * The sidebar container: 264px by default, resizable 220–360 from its right edge, hidden with ⌘B (width and
 * visibility live in the window layout). It hosts the Home-mode sidebar while Home is active, else the
 * active board's sidebar.
 */

import { ResizeHandle } from "../components";
import { BoardSidebar } from "../board";
import { HomeSidebar } from "../home";
import { boardKey } from "../state/boards";
import { SIDEBAR_WIDTH, setSidebarWidth, useLayout } from "../state/layout";
import { useActiveBoard } from "../state/tabs";

export function Sidebar() {
  const width = useLayout((layout) => layout.sidebar.width);
  const visible = useLayout((layout) => layout.sidebar.visible);
  const board = useActiveBoard();
  if (!visible) {
    return null;
  }
  return (
    <aside id="sidebar" aria-label="Sidebar" className="app-sidebar" style={{ width }}>
      {board ? <BoardSidebar key={boardKey(board)} board={board} /> : <HomeSidebar />}
      <ResizeHandle edge="right" size={width} bounds={SIDEBAR_WIDTH} onResize={setSidebarWidth} label="Sidebar width" controls="sidebar" />
    </aside>
  );
}
