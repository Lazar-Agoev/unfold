/**
 * The project's Boards on Home: All / Published / Drafts, sorted by last edited or
 * title (remembered per window), as a grid of board cards; "No boards yet" with New board when empty.
 */

import { useMemo, useRef } from "react";
import { Button, EmptyState, useContextMenu } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type BoardSummary, type ProjectId } from "../../ipc";
import { setBoardsFilter, setBoardsSort, useLayout, type BoardsFilter, type BoardsSort } from "../../state/layout";
import { newBoardIn } from "../actions";
import { BoardGrid, type BoardCardModel } from "./BoardGrid";
import { boardSubline, boardTitle, filterBoards, sortBoards } from "./model";
import "./boards.css";

const FILTERS: readonly { value: BoardsFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "published", label: "Published" },
  { value: "drafts", label: "Drafts" },
];

const SORTS: readonly { value: BoardsSort; label: string }[] = [
  { value: "edited", label: "Last edited" },
  { value: "title", label: "Title" },
];

/** A board summary as a card, optionally led by its project's name (cross-project lists). */
export function cardOf(board: BoardSummary, projectName?: string): BoardCardModel {
  return {
    project: board.project,
    id: board.id,
    title: boardTitle(board),
    draft: board.place === "draft",
    subline: boardSubline(board),
    with: board.projects,
    ...(projectName === undefined ? {} : { projectName }),
  };
}

function SortButton({ sort }: { readonly sort: BoardsSort }) {
  const menu = useContextMenu("Sort boards");
  const button = useRef<HTMLButtonElement>(null);
  const label = SORTS.find((option) => option.value === sort)?.label ?? "";
  return (
    <>
      <button
        ref={button}
        type="button"
        className="home-boards-sort"
        aria-haspopup="menu"
        onClick={() => {
          if (button.current) {
            menu.open(
              button.current,
              SORTS.map((option) => ({
                id: option.value,
                label: option.label,
                checked: option.value === sort,
                onSelect: () => {
                  setBoardsSort(option.value);
                },
              })),
            );
          }
        }}
      >
        {label}
        <Icon name="chevron-down-small" size={10} />
      </button>
      {menu.element}
    </>
  );
}

const EMPTY_FILTER: Readonly<Record<BoardsFilter, string>> = { all: "No boards yet", published: "No published boards", drafts: "No drafts" };

export function BoardsSection({ project }: { readonly project: ProjectId }) {
  const list = useEngineQuery("list_boards", { project });
  const filter = useLayout((layout) => layout.home.boardsFilter);
  const sort = useLayout((layout) => layout.home.boardsSort);
  const boards = useMemo(() => sortBoards(filterBoards(list.data?.boards ?? [], filter), sort).map((board) => cardOf(board)), [list.data, filter, sort]);
  const none = list.data?.boards.length === 0;

  return (
    <section className="home-boards" aria-label="Boards">
      <div className="home-boards-header">
        <h2 className="home-section-title">Boards</h2>
        <div className="home-boards-filters" role="radiogroup" aria-label="Show">
          {FILTERS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={filter === option.value}
              className="home-boards-filter"
              onClick={() => {
                setBoardsFilter(option.value);
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
        <SortButton sort={sort} />
      </div>
      {list.error ? <p className="home-boards-note">{list.error.message}</p> : null}
      {none ? (
        <EmptyState
         
          title="No boards yet"
          body="A board is where you follow the code: trace an entrance above, or start an empty one."
          action={
            <Button variant="secondary" icon="plus" onClick={() => void newBoardIn(project)}>
              New board
            </Button>
          }
        />
      ) : list.data && boards.length === 0 ? (
        <p className="home-boards-note">{EMPTY_FILTER[filter]}</p>
      ) : (
        <BoardGrid boards={boards} label="Boards" />
      )}
    </section>
  );
}
