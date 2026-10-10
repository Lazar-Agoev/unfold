/**
 * Notices at the top of the canvas: an outside edit made while this window had unsaved changes (Keep
 * mine or Take theirs; Keep mine or Close the board when the file was deleted), and a save that failed
 * (Retry). Neither is modal, and neither covers a card's first row for long: they sit over the stage's
 * top edge.
 */

import { useState } from "react";
import { Button, showToast } from "../components";
import { Icon } from "../icons";
import { toUnfoldError } from "../ipc";
import { closeBoard, resolveConflict, saveBoard, useOpenBoard, type BoardRef } from "../state/boards";

export function BoardBanner({ board }: { readonly board: BoardRef }) {
  const conflict = useOpenBoard(board, (open) => open.conflict) ?? null;
  const saveError = useOpenBoard(board, (open) => open.saveError) ?? null;
  const saving = useOpenBoard(board, (open) => open.saving) ?? false;
  const [busy, setBusy] = useState(false);

  const choose = async (keep: "mine" | "theirs") => {
    setBusy(true);
    try {
      await resolveConflict(board, keep);
    } catch (error) {
      showToast({ title: keep === "mine" ? "Couldn't keep your version" : "Couldn't load the file", body: toUnfoldError(error).message, tone: "danger" });
    } finally {
      setBusy(false);
    }
  };

  if (conflict) {
    const deleted = conflict.diskRevision === null;
    return (
      <div className="board-banner conflict" role="alert">
        <Icon name="warning-triangle" size={14} />
        <span className="board-banner-text">
          {deleted
            ? "This board's file was deleted outside Unfold while you had unsaved changes."
            : "This board changed on disk (an agent or a teammate edited it) while you had unsaved changes."}
        </span>
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => void choose("mine")}>
          {deleted ? "Keep mine (write it again)" : "Keep mine"}
        </Button>
        {deleted ? (
          <Button variant="secondary" size="sm" disabled={busy} onClick={() => void closeBoard(board).catch(() => undefined)}>
            Close the board
          </Button>
        ) : (
          <Button variant="secondary" size="sm" disabled={busy} onClick={() => void choose("theirs")}>
            Take theirs
          </Button>
        )}
      </div>
    );
  }
  if (saveError !== null) {
    return (
      <div className="board-banner" role="status">
        <Icon name="warning-circle" size={14} />
        <span className="board-banner-text">Not saved: {saveError}</span>
        <Button variant="secondary" size="sm" busy={saving} onClick={() => void saveBoard(board)}>
          Retry
        </Button>
      </div>
    );
  }
  return null;
}
