/**
 * The notice at the top of the canvas when the board isn't saving. Not modal, and it sits over the
 * stage's top edge; the work stays in the window.
 *
 * - The last save failed: "Not saved · Retry". Retry (or the next edit) writes the work over the file.
 * - The file on disk isn't a valid board (a merge conflict): saving stopped, and Save my version writes
 *   the board over it. The board takes the file by itself once it is a valid board again.
 * - The file changed while the board had edits of its own: Keep my edits writes them over the file, and
 *   Load the file takes the file's version.
 */

import { TextButton } from "../components";
import { Icon } from "../icons";
import { keepMine, loadFile, saveBoard, useOpenBoard, type BoardRef } from "../state/boards";

export function BoardBanner({ board }: { readonly board: BoardRef }) {
  const saveError = useOpenBoard(board, (open) => open.saveError) ?? null;
  const saving = useOpenBoard(board, (open) => open.saving) ?? false;
  const paused = useOpenBoard(board, (open) => open.paused) ?? null;
  if (paused?.kind === "invalid") {
    return (
      <div className="board-banner" role="status">
        <Icon name="warning-circle" size={14} />
        <span className="board-banner-text" title={saveError ?? paused.message}>
          The board's file on disk isn't a valid board, so saving stopped.
        </span>
        <TextButton disabled={saving} onClick={() => void keepMine(board)}>
          Save my version
        </TextButton>
      </div>
    );
  }
  if (paused?.kind === "changed") {
    return (
      <div className="board-banner" role="status">
        <Icon name="warning-circle" size={14} />
        <span className="board-banner-text" title={saveError ?? undefined}>
          The board's file changed on disk.
        </span>
        <TextButton disabled={saving} onClick={() => void keepMine(board)}>
          Keep my edits
        </TextButton>
        <span aria-hidden="true">·</span>
        <TextButton disabled={saving} onClick={() => void loadFile(board)}>
          Load the file
        </TextButton>
      </div>
    );
  }
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
