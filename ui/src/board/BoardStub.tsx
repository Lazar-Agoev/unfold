/** Placeholder board screen (U1c): the V1 canvas under a plain top bar, and a pages list, until U2. */

import type { JSX } from "react";
import { IconButton, ListRow, Monogram, SectionLabel } from "../components";
import { Icon } from "../icons";
import { BoardTab } from "../canvas/BoardTab";
import { setActivePage, useOpenBoard, type BoardRef } from "../state/boards";
import { activateHome } from "../state/tabs";
import { useProjectRow } from "../state/workspace";
import "./board.css";

export function BoardSidebar({ board }: { readonly board: BoardRef }): JSX.Element {
  const project = useProjectRow(board.project);
  const title = useOpenBoard(board, (open) => open.document.title);
  const pages = useOpenBoard(board, (open) => open.document.pages);
  const active = useOpenBoard(board, (open) => open.activePage);
  return (
    <div className="board-stub-sidebar">
      <div className="board-stub-crumb">
        <IconButton icon="chevron-left" label="Home" size={20} onClick={activateHome} />
        <Monogram name={project?.name ?? "?"} size={14} />
        <span>{project?.name}</span>
      </div>
      <h1 className="board-stub-title">{title}</h1>
      <SectionLabel>Pages</SectionLabel>
      {(pages ?? []).map((page) => (
        <ListRow
          key={page.id}
          role="button"
          tabIndex={0}
          leading={<Icon name="page" />}
          label={page.title}
          selected={page.id === active}
          onClick={() => {
            setActivePage(board, page.id);
          }}
        />
      ))}
    </div>
  );
}

export function BoardView({ board }: { readonly board: BoardRef }): JSX.Element {
  const pageTitle = useOpenBoard(board, (open) => open.document.pages.find((page) => page.id === open.activePage)?.title);
  return (
    <div className="board-stub">
      <header className="board-stub-topbar">{pageTitle}</header>
      <div className="board-stub-stage">
        <BoardTab board={board} active />
      </div>
    </div>
  );
}
