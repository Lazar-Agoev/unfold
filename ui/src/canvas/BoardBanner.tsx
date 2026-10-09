/**
 * Quiet notices at the top of a board tab, over no card (they sit above the canvas's first row of
 * space and never block it): an outside edit made while this window had unsaved changes ("Keep
 * mine" or "Take theirs"), and a save that failed (with Retry). Neither is modal.
 */

import { AlertTriangle } from "lucide-react";
import { useState } from "react";
import { showToast } from "../components";
import { toUnfoldError } from "../ipc/unfold";
import { closeBoard, resolveConflict, saveBoard, useBoards } from "../state/boards/store";

export function BoardBanner({ boardId }: { readonly boardId: string }) {
  const conflict = useBoards((state) => state.boards[boardId]?.conflict ?? null);
  const saveError = useBoards((state) => state.boards[boardId]?.saveError ?? null);
  const saving = useBoards((state) => state.boards[boardId]?.saving ?? false);
  const [busy, setBusy] = useState(false);

  const choose = async (keep: "mine" | "theirs") => {
    setBusy(true);
    try {
      await resolveConflict(boardId, keep);
    } catch (error) {
      showToast({ message: `Couldn't ${keep === "mine" ? "keep your version" : "load the file"}: ${toUnfoldError(error).message}`, tone: "danger" });
    } finally {
      setBusy(false);
    }
  };

  if (conflict) {
    const deleted = conflict.diskRevision === null;
    return (
      <div className="board-banner conflict" role="alert">
        <AlertTriangle size={14} aria-hidden="true" />
        <span className="board-banner-text">
          {deleted
            ? "This board's file was deleted outside Unfold while you had unsaved changes."
            : "This board changed on disk (an agent or a teammate edited it) while you had unsaved changes."}
        </span>
        <button type="button" className="board-banner-action" disabled={busy} onClick={() => void choose("mine")}>
          {deleted ? "Keep mine (write it again)" : "Keep mine"}
        </button>
        {deleted ? (
          <button type="button" className="board-banner-action" disabled={busy} onClick={() => void closeBoard(boardId).catch(() => undefined)}>
            Close the board
          </button>
        ) : (
          <button type="button" className="board-banner-action" disabled={busy} onClick={() => void choose("theirs")}>
            Take theirs
          </button>
        )}
      </div>
    );
  }
  if (saveError !== null) {
    return (
      <div className="board-banner quiet" role="status">
        <span className="board-banner-text">Not saved: {saveError}</span>
        <button type="button" className="board-banner-action" disabled={saving} onClick={() => void saveBoard(boardId)}>
          Retry
        </button>
      </div>
    );
  }
  return null;
}
