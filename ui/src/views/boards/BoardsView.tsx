/**
 * The Boards view (architecture §11.3): published boards (filled square) and drafts on this machine
 * (open square), from `list_boards`, with a label only when something is wrong, and **+** for a
 * new board. Below, for the open board: its outline in follow order indented by call depth (click
 * focuses the card), the Flow nodes that still show code names (stepped through one by one), its
 * groups, and its health problems with their repairs.
 */

import { LayoutGrid, Plus, Square, SquareDashed } from "lucide-react";
import { useMemo, type ReactNode } from "react";
import { openBoardDialog } from "../../boards/dialogs";
import { runMoveToNewWindow } from "../../boards/flows";
import { EmptyState, IconButton, TextMark, VirtualList, showToast, useContextMenu, type ListGroup, type MenuEntry } from "../../components";
import type { BoardSummary } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { toUnfoldError, windowCommand } from "../../ipc/unfold";
import { newBoard, openBoard, useActiveBoardId } from "../../state/boards/api";
import { getBoard } from "../../state/boards/store";
import { useWindow } from "../../state/window";
import { ViewHeader } from "../ViewHeader";
import { OpenBoardSections } from "./OpenBoardSections";
import "../views.css";
import "./boards.css";

const ROW_HEIGHT = 22;
const HEADER_HEIGHT = 26;
/** The list grows with its boards up to this height, then scrolls; the open board's sections follow. */
const LIST_MAX_HEIGHT = 300;

/** Opens a board here, or focuses the window that has it open (U-37). */
export function openBoardFromList(board: BoardSummary, windowId: number | null): void {
  if (board.heldBy !== null && board.heldBy !== windowId) {
    void windowCommand("focus_window", { windowId: board.heldBy });
    return;
  }
  // `open_board` itself answers `held` when the list was stale, and the store focuses that window.
  openBoard(board.id).catch((error: unknown) => {
    showToast({ message: `Couldn't open ${board.title ?? board.name}: ${toUnfoldError(error).message}`, tone: "danger" });
  });
}

function boardRow(board: BoardSummary, windowId: number | null): ReactNode {
  const published = board.place === "published";
  const Icon = published ? Square : SquareDashed;
  const elsewhere = board.heldBy !== null && board.heldBy !== windowId;
  return (
    <div className="result-row board-row" title={board.error ?? board.id}>
      <Icon size={12} className={`board-square board-square-${board.place}`} aria-label={published ? "Published" : "Draft"} />
      <span className="result-name">{board.title ?? board.name}</span>
      {board.error ? <TextMark>can't be read</TextMark> : board.label ? <span className="board-label">{board.label}</span> : null}
      {elsewhere ? <TextMark>open in another window</TextMark> : null}
      <span className="result-detail">{board.cards}</span>
    </div>
  );
}

function createBoard(): void {
  newBoard().catch((error: unknown) => {
    showToast({ message: `Couldn't create a board: ${toUnfoldError(error).message}`, tone: "danger" });
  });
}

export function BoardsView({ children }: { readonly children?: ReactNode }) {
  const query = useEngineQuery("list_boards", {});
  const windowId = useWindow((state) => state.info?.windowId ?? null);
  const activeBoardId = useActiveBoardId();
  const menu = useContextMenu("Board actions");

  const groups = useMemo((): ListGroup<BoardSummary>[] => {
    if (!query.data) {
      return [];
    }
    return [
      { id: "published", label: "Boards", count: String(query.data.published.length), items: query.data.published },
      { id: "drafts", label: "Drafts · on this machine", count: String(query.data.drafts.length), items: query.data.drafts },
    ];
  }, [query.data]);
  const nonEmpty = groups.filter((group) => group.items.length > 0);
  const height = nonEmpty.reduce((sum, group) => sum + HEADER_HEIGHT + group.items.length * ROW_HEIGHT, 0);

  const rowMenu = (board: BoardSummary): MenuEntry[] => {
    const here = getBoard(board.id) !== undefined;
    return [
      { id: "open", label: here ? "Show" : "Open", onSelect: () => { openBoardFromList(board, windowId); } },
      ...(here
        ? [
            { id: "move", label: "Move to New Window", onSelect: () => void runMoveToNewWindow(board.id) },
            { kind: "separator" as const, id: "s1" },
            { id: "delete", label: "Delete…", danger: true, onSelect: () => { openBoardDialog({ kind: "delete", boardId: board.id }); } },
          ]
        : []),
    ];
  };

  let list: ReactNode;
  if (query.error) {
    list = <p className="view-error">{query.error.message}</p>;
  } else if (query.isPending) {
    list = <p className="view-hint">Loading…</p>;
  } else if (nonEmpty.length === 0) {
    list = <EmptyState size="compact" icon={LayoutGrid} title="No boards yet" hint="Open an entrance or a definition to start a draft board." />;
  } else {
    list = (
      <div className="boards-list-box" style={{ height: Math.min(height, LIST_MAX_HEIGHT) }}>
        <VirtualList<BoardSummary>
          label="Boards"
          className="boards-list"
          groups={groups}
          getId={(board) => board.id}
          getText={(board) => `${board.title ?? board.name}${board.label ? `, ${board.label}` : ""}`}
          getItemHeight={() => ROW_HEIGHT}
          headerHeight={HEADER_HEIGHT}
          selectedId={activeBoardId}
          renderItem={(board) => boardRow(board, windowId)}
          onActivate={(board) => {
            openBoardFromList(board, windowId);
          }}
          onContextMenu={(board, at) => {
            menu.open(at, rowMenu(board));
          }}
        />
      </div>
    );
  }

  return (
    <section className="view boards-view" aria-labelledby="boards-title">
      <ViewHeader title="Boards" id="boards-title">
        <IconButton icon={Plus} label="New Board" size="small" onClick={createBoard} />
      </ViewHeader>
      <div className="boards-scroll">
        {list}
        {activeBoardId !== null ? <OpenBoardSections boardId={activeBoardId} /> : null}
        {children}
      </div>
      {menu.element}
    </section>
  );
}
