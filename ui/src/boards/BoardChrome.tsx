/**
 * The active board in the window chrome (architecture §11.1): its title in the title bar (click to
 * rename) with a Draft chip or the published path, Publish, and the board menu (Rename, Export
 * Mermaid, Copy Mermaid, Fold all, Unfold all, Move to New Window, Move back to drafts, Delete).
 * The same commands are actions, so the palette and the keyboard reach them too. Also hosts the
 * board dialogs.
 */

import { MoreHorizontal } from "lucide-react";
import { useEffect, useRef } from "react";
import { showInInspector } from "../canvas/cards/cardMenu";
import { Button, Chip, MenuButton, showToast, type MenuEntry } from "../components";
import { toUnfoldError } from "../ipc/unfold";
import { useRegisterActions, type ActionSpec } from "../keyboard";
import { SlotPortal } from "../shell/slots";
import { foldAllCards, newBoard, removeCardFromBoard, useBoardZoom } from "../state/boards/api";
import { getBoard, useBoards } from "../state/boards/store";
import { useActiveTab } from "../state/tabs";
import { BoardDialogs } from "./BoardDialogs";
import { openBoardDialog, setRenaming, useBoardDialogs } from "./dialogs";
import { runCopyMermaid, runExportMermaid, runMoveToNewWindow, runRename, runUnpublish } from "./flows";
import "./boards.css";

/** The board of the active tab, or null when a source tab (or nothing) is active. */
export function useActiveTabBoard(): string | null {
  const tab = useActiveTab();
  const boardId = tab?.kind === "board" ? tab.boardId : null;
  return useBoards((state) => (boardId !== null && state.boards[boardId] ? boardId : null));
}

/** The board menu's entries (the title bar's `⋯`). */
export function boardMenuEntries(boardId: string): MenuEntry[] {
  const board = getBoard(boardId);
  const published = board?.place === "published";
  const empty = (board?.document.cards.length ?? 0) === 0;
  return [
    { id: "rename", label: "Rename…", onSelect: () => { setRenaming(boardId); } },
    { kind: "separator", id: "s1" },
    { id: "export", label: "Export Mermaid…", disabled: empty, onSelect: () => void runExportMermaid(boardId) },
    { id: "copy", label: "Copy Mermaid", disabled: empty, onSelect: () => void runCopyMermaid(boardId) },
    { kind: "separator", id: "s2" },
    { id: "fold", label: "Fold All", disabled: empty, onSelect: () => { foldAllCards(boardId, true); } },
    { id: "unfold", label: "Unfold All", disabled: empty, onSelect: () => { foldAllCards(boardId, false); } },
    { kind: "separator", id: "s3" },
    { id: "move", label: "Move to New Window", onSelect: () => void runMoveToNewWindow(boardId) },
    ...(published ? [{ id: "unpublish", label: "Move Back to Drafts", onSelect: () => void runUnpublish(boardId) }] : []),
    { kind: "separator", id: "s4" },
    { id: "delete", label: "Delete…", danger: true, onSelect: () => { openBoardDialog({ kind: "delete", boardId }); } },
  ];
}

function TitleEditor({ boardId, title }: { readonly boardId: string; readonly title: string }) {
  const input = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    input.current?.select();
  }, []);
  const finish = (value: string | null) => {
    if (done.current) {
      return;
    }
    done.current = true;
    setRenaming(null);
    if (value !== null && value.trim().length > 0 && value.trim() !== title) {
      void runRename(boardId, value);
    }
  };
  return (
    <input
      ref={input}
      autoFocus
      className="board-title-input"
      aria-label="Board title"
      defaultValue={title}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          finish(event.currentTarget.value);
        } else if (event.key === "Escape") {
          event.preventDefault();
          finish(null);
        }
      }}
      onBlur={(event) => {
        finish(event.currentTarget.value);
      }}
    />
  );
}

function BoardTitle({ boardId }: { readonly boardId: string }) {
  const title = useBoards((state) => state.boards[boardId]?.document.title ?? "");
  const place = useBoards((state) => state.boards[boardId]?.place ?? "draft");
  const saving = useBoards((state) => state.boards[boardId]?.saving ?? false);
  const renaming = useBoardDialogs((state) => state.renaming === boardId);
  return (
    <>
      {renaming ? (
        <TitleEditor key={boardId} boardId={boardId} title={title} />
      ) : (
        <button
          type="button"
          className="board-title"
          title="Rename the board"
          aria-label={`${title}, rename`}
          onClick={() => {
            setRenaming(boardId);
          }}
        >
          {title}
        </button>
      )}
      {place === "draft" ? (
        <Chip tone="neutral" title="Saved on this machine only, in .unfold/local/drafts/">
          Draft
        </Chip>
      ) : (
        <code className="board-path" title="Edits save into this file">
          {boardId}
        </code>
      )}
      {saving ? <span className="board-saving">Saving…</span> : null}
    </>
  );
}

function boardActions(boardId: string | null): ActionSpec[] {
  const has = () => boardId !== null && getBoard(boardId) !== undefined;
  const selectedCard = () => (boardId !== null ? (getBoard(boardId)?.view.selectedCardId ?? null) : null);
  const run = (fn: (id: string) => void) => () => {
    if (boardId !== null) {
      fn(boardId);
    }
  };
  return [
    {
      id: "board.new",
      title: "New Board",
      category: "Board",
      run: async () => {
        try {
          await newBoard();
        } catch (error) {
          showToast({ message: `Couldn't create a board: ${toUnfoldError(error).message}`, tone: "danger" });
        }
      },
    },
    {
      id: "board.publish",
      title: "Publish Board…",
      category: "Board",
      enabled: () => has() && getBoard(boardId ?? "")?.place === "draft",
      run: run((id) => { openBoardDialog({ kind: "publish", boardId: id }); }),
    },
    { id: "board.rename", title: "Rename Board…", category: "Board", enabled: has, run: run((id) => { setRenaming(id); }) },
    { id: "board.exportMermaid", title: "Export Board as Mermaid…", category: "Board", enabled: has, run: run((id) => void runExportMermaid(id)) },
    { id: "board.copyMermaid", title: "Copy Board as Mermaid", category: "Board", enabled: has, run: run((id) => void runCopyMermaid(id)) },
    { id: "board.foldAll", title: "Fold All Cards", category: "Board", enabled: has, run: run((id) => { foldAllCards(id, true); }) },
    { id: "board.unfoldAll", title: "Unfold All Cards", category: "Board", enabled: has, run: run((id) => { foldAllCards(id, false); }) },
    { id: "board.moveToNewWindow", title: "Move Board to New Window", category: "Board", enabled: has, run: run((id) => void runMoveToNewWindow(id)) },
    {
      id: "board.unpublish",
      title: "Move Board Back to Drafts",
      category: "Board",
      enabled: () => has() && getBoard(boardId ?? "")?.place === "published",
      run: run((id) => void runUnpublish(id)),
    },
    { id: "board.delete", title: "Delete Board…", category: "Board", enabled: has, run: run((id) => { openBoardDialog({ kind: "delete", boardId: id }); }) },
    {
      id: "board.describeCard",
      title: "Describe Selected Card",
      category: "Board",
      enabled: () => selectedCard() !== null,
      run: () => { showInInspector("inspector.description"); },
    },
    {
      id: "board.makeGroup",
      title: "Make a Group in Selected Card…",
      category: "Board",
      enabled: () => selectedCard() !== null,
      run: run((id) => {
        const cardId = selectedCard();
        if (cardId !== null) {
          openBoardDialog({ kind: "group", boardId: id, cardId });
        }
      }),
    },
    {
      id: "board.removeCard",
      title: "Remove Selected Card from Board",
      category: "Board",
      enabled: () => selectedCard() !== null,
      run: run((id) => {
        const cardId = selectedCard();
        if (cardId !== null) {
          removeCardFromBoard(id, cardId);
        }
      }),
    },
  ];
}

/** The active board's title-bar pieces, its actions, and the board dialogs. Mount once in the workbench. */
export function BoardChrome() {
  const boardId = useActiveTabBoard();
  const place = useBoards((state) => (boardId !== null ? state.boards[boardId]?.place : undefined));
  useRegisterActions(boardActions(boardId));
  return (
    <>
      {boardId !== null ? (
        <>
          <SlotPortal name="titlebar.board">
            <BoardTitle boardId={boardId} />
          </SlotPortal>
          {place === "draft" ? (
            <SlotPortal name="titlebar.actions">
              <Button
                size="small"
                className="titlebar-publish"
                onClick={() => {
                  openBoardDialog({ kind: "publish", boardId });
                }}
              >
                Publish
              </Button>
            </SlotPortal>
          ) : null}
          <SlotPortal name="titlebar.menu">
            <BoardMenu boardId={boardId} />
          </SlotPortal>
          <SlotPortal name="statusbar.zoom">
            <ZoomReadout boardId={boardId} />
          </SlotPortal>
        </>
      ) : null}
      <BoardDialogs />
    </>
  );
}

/** The board's zoom in the status bar (architecture §11.5). */
function ZoomReadout({ boardId }: { readonly boardId: string }) {
  const zoom = useBoardZoom(boardId);
  return (
    <span className="statusbar-item" aria-label="Zoom">
      {Math.round(zoom * 100)}%
    </span>
  );
}

function BoardMenu({ boardId }: { readonly boardId: string }) {
  // Subscribed so the entries (Move back to drafts, disabled states) are current.
  useBoards((state) => state.boards[boardId]?.place);
  useBoards((state) => state.boards[boardId]?.document.cards.length);
  return <MenuButton variant="icon" icon={MoreHorizontal} label="Board Menu" menuLabel="Board" entries={boardMenuEntries(boardId)} />;
}
