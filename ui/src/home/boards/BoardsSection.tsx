/**
 * The project's Boards on Home: one quiet list, most recently edited first (`list_boards`' order).
 * Each row is the board's icon, its title, its folder, and when it was last edited. Click opens the
 * board; the right-click menu is the board menu. "No boards yet" with New board when there are none.
 */

import type { MouseEvent } from "react";
import { Button, EmptyState, useContextMenu } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type BoardSummary, type ProjectId } from "../../ipc";
import { boardMenuEntries } from "../../menus";
import { newBoardIn, openBoardFromHome } from "../actions";
import { useWindowed } from "../windowed";
import { boardTitle, editedText } from "./model";
import "./boards.css";

/** A row's height, its top rule included. */
const ROW_HEIGHT = 36;
/** Past this many boards the list renders only the rows in view. */
const WINDOW_THRESHOLD = 200;

function BoardRow({ board, first, onMenu }: { readonly board: BoardSummary; readonly first: boolean; readonly onMenu: (event: MouseEvent) => void }) {
  const ref = { project: board.project, id: board.id };
  return (
    <div
      role="button"
      tabIndex={0}
      className={first ? "home-board-row first" : "home-board-row"}
      data-board-key={`${board.project}\u0000${board.id}`}
      onClick={() => void openBoardFromHome(ref)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) {
          event.preventDefault();
          void openBoardFromHome(ref);
        }
      }}
      onContextMenu={onMenu}
    >
      <Icon name="board" />
      <span className="home-board-title">{boardTitle(board)}</span>
      <span className="home-board-folder">{board.folder}</span>
      {board.error === null ? (
        <span className="home-board-edited">{editedText(board)}</span>
      ) : (
        <span className="home-board-edited" title={board.error}>
          can't be read
        </span>
      )}
    </div>
  );
}

function BoardList({ boards }: { readonly boards: readonly BoardSummary[] }) {
  const menu = useContextMenu("Board menu", 220);
  const { attach, rows, height, offsets } = useWindowed({ count: boards.length, size: () => ROW_HEIGHT, threshold: WINDOW_THRESHOLD });
  const row = (board: BoardSummary, index: number) => (
    <BoardRow
      key={board.id}
      board={board}
      first={index === 0}
      onMenu={(event) => {
        menu.open(event, boardMenuEntries({ project: board.project, id: board.id }));
      }}
    />
  );
  return (
    <div ref={attach} role="group" aria-label="Boards" className="home-board-list" style={rows ? { height } : undefined}>
      {rows
        ? rows.map((item, slot) => {
            const board = boards[item.index];
            return board ? (
              <div key={board.id} className="home-board-slot" style={{ transform: `translateY(${offsets[slot] ?? 0}px)` }}>
                {row(board, item.index)}
              </div>
            ) : null;
          })
        : boards.map(row)}
      {menu.element}
    </div>
  );
}

export function BoardsSection({ project }: { readonly project: ProjectId }) {
  const list = useEngineQuery("list_boards", { project });
  return (
    <section className="home-boards" aria-label="Boards">
      <h2 className="home-section-title">Boards</h2>
      {list.error ? <p className="home-boards-note">{list.error.message}</p> : null}
      {list.data?.boards.length === 0 ? (
        <EmptyState
          title="No boards yet"
          body="A board is where you follow the code: trace an entrance above, or start an empty one."
          action={
            <Button variant="secondary" icon="plus" onClick={() => void newBoardIn(project)}>
              New board
            </Button>
          }
        />
      ) : list.data ? (
        <BoardList boards={list.data.boards} />
      ) : null}
    </section>
  );
}
