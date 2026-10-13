/**
 * The board-mode sidebar's header: back to Home, the project's monogram with "project / folder", the
 * board's title (renamed in place on double-click), and the board menu. The title is the focus target
 * `board.title`: focusing it (the board menu's Rename…) starts renaming.
 */

import { useRef, useState } from "react";
import { IconButton, Monogram, showToast } from "../components";
import { toUnfoldError } from "../ipc";
import { useFocusTarget } from "../lib/focus-targets";
import { BoardMenuButton } from "../menus";
import { renameBoard, useOpenBoard, type BoardRef } from "../state/boards";
import { showHome } from "../state/shownBoard";
import { useProjectRow } from "../state/workspace";
import { InlineRename } from "./InlineRename";
import { useBoardSummary } from "./publishState";

/** The focus target that starts renaming the board in place. */
const BOARD_TITLE_FIELD = "board.title";

export function BoardHeader({ board }: { readonly board: BoardRef }) {
  const project = useProjectRow(board.project);
  const title = useOpenBoard(board, (open) => open.document.title) ?? "";
  const savedFolder = useOpenBoard(board, (open) => open.document.folder);
  // The list's folder once it has the board (null = top level), else the one the board opened with.
  const summary = useBoardSummary(board);
  const folder = summary ? summary.folder : (savedFolder ?? null);
  const [editing, setEditing] = useState(false);
  const slot = useRef<HTMLDivElement>(null);
  useFocusTarget(BOARD_TITLE_FIELD, slot);
  const name = project?.name ?? "";
  return (
    <div className="board-header">
      <div className="board-crumb">
        <IconButton icon="chevron-left" label="Home" size={20} onClick={showHome} />
        <Monogram name={name} size={14} />
        <span className="board-crumb-text">{folder !== null ? `${name} / ${folder}` : name}</span>
      </div>
      <div className="board-title-row">
        {/* The focus target: focused (the board menu's Rename…), it starts renaming; a click doesn't focus it. */}
        <div
          ref={slot}
          className="board-title-slot"
          tabIndex={-1}
          onMouseDown={(event) => {
            if (!(event.target instanceof HTMLInputElement)) {
              event.preventDefault();
            }
          }}
          onFocus={(event) => {
            if (event.target === event.currentTarget) {
              setEditing(true);
            }
          }}
        >
          <InlineRename
            value={title}
            editing={editing}
            label="Board title"
            className="board-title"
            onDone={(next) => {
              setEditing(false);
              if (next !== null) {
                renameBoard(board, next).catch((error: unknown) => {
                  showToast({ title: "Couldn't rename the board", body: toUnfoldError(error).message, tone: "danger" });
                });
              }
            }}
          >
            <h1
              className="board-title"
              title="Double-click to rename"
              onDoubleClick={() => {
                setEditing(true);
              }}
            >
              {title}
            </h1>
          </InlineRename>
        </div>
        <BoardMenuButton board={board} />
      </div>
    </div>
  );
}
