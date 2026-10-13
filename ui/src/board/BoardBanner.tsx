/**
 * The notice at the top of the canvas when the board's last save failed: "Not saved · Retry". The
 * work stays in the window; Retry (or the next edit) writes it over the board's file. Not modal, and
 * it sits over the stage's top edge.
 */

import { TextButton } from "../components";
import { Icon } from "../icons";
import { saveBoard, useOpenBoard, type BoardRef } from "../state/boards";

export function BoardBanner({ board }: { readonly board: BoardRef }) {
  const saveError = useOpenBoard(board, (open) => open.saveError) ?? null;
  const saving = useOpenBoard(board, (open) => open.saving) ?? false;
  if (saveError === null) {
    return null;
  }
  return (
    <div className="board-banner" role="status">
      <Icon name="warning-circle" size={14} />
      <span className="board-banner-text" title={saveError}>
        Not saved ·
      </span>
      <TextButton disabled={saving} onClick={() => void saveBoard(board)}>
        Retry
      </TextButton>
    </div>
  );
}
