/**
 * Delete…, 400 wide over the canvas: works on a board this window holds or on one Home's menu names.
 */

import { useEffect } from "react";
import { ConfirmDialog, showToast } from "../components";
import { toUnfoldError, useEngineQuery } from "../ipc";
import { deleteBoard, useOpenBoard, type BoardRef } from "../state/boards";

interface BoardDialogProps {
  readonly board: BoardRef;
  readonly onClose: () => void;
}

/**
 * The board's title: from the open session when this window holds it, else from `list_boards` (Home's
 * menus act on boards that aren't open). Undefined while it loads; the dialog closes when the board is
 * gone.
 */
function useBoardTitleOrClose(ref: BoardRef, onClose: () => void): string | undefined {
  const list = useEngineQuery("list_boards", { project: ref.project });
  const summary = list.data?.boards.find((board) => board.id === ref.id);
  const open = useOpenBoard(ref, (board) => board.document.title);
  const missing = open === undefined && !summary && !list.isPending;
  useEffect(() => {
    if (missing) {
      onClose();
    }
  }, [missing, onClose]);
  return open ?? (summary ? (summary.title ?? summary.name) : undefined);
}

export function DeleteBoardDialog({ board, onClose }: BoardDialogProps) {
  const title = useBoardTitleOrClose(board, onClose);
  if (title === undefined) {
    return null;
  }
  return (
    <ConfirmDialog
      open
      danger
      title={`Delete “${title}”?`}
      message={`This deletes ${board.id}. It can't be undone.`}
      confirmLabel="Delete"
      placement="canvas"
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        deleteBoard(board).then(
          () => {
            showToast({ title: `Deleted ${title}` });
          },
          (error: unknown) => {
            showToast({ title: `Couldn't delete ${title}`, body: toUnfoldError(error).message, tone: "danger" });
          },
        );
      }}
    />
  );
}
