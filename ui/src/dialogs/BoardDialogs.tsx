/**
 * The board confirmations (V-73), 400 wide over the canvas: Discard changes (back to the published
 * version) and Delete. Both work on a board this window holds or on one Home's menu names.
 */

import { useEffect } from "react";
import { ConfirmDialog, showToast } from "../components";
import { call, toUnfoldError } from "../ipc";
import { plural } from "../lib/format";
import { deleteBoard, discardChanges, getOpenBoard, type BoardRef } from "../state/boards";
import { useBoardInfo, type BoardInfo } from "./boardInfo";

interface BoardDialogProps {
  readonly board: BoardRef;
  readonly onClose: () => void;
}

/** The board's facts, closing the dialog when the board is gone. */
function useBoardOrClose(board: BoardRef, onClose: () => void): BoardInfo | null | undefined {
  const info = useBoardInfo(board);
  const missing = info === null;
  useEffect(() => {
    if (missing) {
      onClose();
    }
  }, [missing, onClose]);
  return info;
}

/** Returns a board to its published version: through the session when open (an undoable step), else directly. */
async function discardBoardChanges(board: BoardRef): Promise<void> {
  if (getOpenBoard(board)) {
    await discardChanges(board);
  } else {
    await call("discard_changes", { project: board.project, id: board.id });
  }
}

export function DiscardDialog({ board, onClose }: BoardDialogProps) {
  const info = useBoardOrClose(board, onClose);
  if (!info) {
    return null;
  }
  const lost = info.changes > 0 ? `The ${plural(info.changes, "unpublished change")} on this machine are lost.` : "Unpublished changes on this machine are lost.";
  return (
    <ConfirmDialog
      open
      danger
      title={`Discard changes to ${info.title}?`}
      message={`${info.title} goes back to its published version. ${lost}`}
      confirmLabel="Discard changes"
      placement="canvas"
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        discardBoardChanges(board).then(
          () => {
            showToast({ title: `Discarded changes to ${info.title}`, body: "It shows its published version again." });
          },
          (error: unknown) => {
            showToast({ title: "Couldn't discard the changes", body: toUnfoldError(error).message, tone: "danger" });
          },
        );
      }}
    />
  );
}

export function DeleteBoardDialog({ board, onClose }: BoardDialogProps) {
  const info = useBoardOrClose(board, onClose);
  if (!info) {
    return null;
  }
  const what =
    info.place === "published"
      ? `This deletes ${board.id}${info.changes > 0 ? " and its unpublished changes" : ""}. Commit the deletion to share it.`
      : "This deletes the draft on this machine.";
  return (
    <ConfirmDialog
      open
      danger
      title={`Delete “${info.title}”?`}
      message={`${what} It can't be undone.`}
      confirmLabel="Delete"
      placement="canvas"
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        deleteBoard(board).then(
          () => {
            showToast({ title: `Deleted ${info.title}` });
          },
          (error: unknown) => {
            showToast({ title: `Couldn't delete ${info.title}`, body: toUnfoldError(error).message, tone: "danger" });
          },
        );
      }}
    />
  );
}
