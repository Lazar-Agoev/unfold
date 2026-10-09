/** The sidebar: the active view (Files, Entrances, Search, or Boards). */

import { useLayout } from "../state/layout";
import { BoardsView } from "../views/boards/BoardsView";
import { EntrancesView } from "../views/entrances/EntrancesView";
import { FilesView } from "../views/files/FilesView";
import { SearchView } from "../views/search/SearchView";
import "./Panes.css";

export function Sidebar() {
  const view = useLayout((state) => state.view);
  return (
    <aside className="pane sidebar" aria-label="Sidebar">
      {view === "files" ? <FilesView /> : null}
      {view === "entrances" ? <EntrancesView /> : null}
      {view === "search" ? <SearchView /> : null}
      {view === "boards" ? <BoardsView /> : null}
    </aside>
  );
}
