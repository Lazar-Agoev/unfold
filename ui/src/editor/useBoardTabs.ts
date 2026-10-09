/**
 * Keeps board tabs and the board store in step: a board that opens gets a tab, a board whose ID
 * changes (publish, rename) keeps its tab, a board that closes or moves away loses it, activating a
 * board tab makes it the active board, and closing a board tab saves and releases the board.
 */

import { useEffect, useRef } from "react";
import { closeBoard, setActiveBoard, subscribeBoardOpened, useOpenBoardIds } from "../state/boards/api";
import { boardTabId, openBoardTab, removeTab, renameBoardTab, setBoardTabCloser, useActiveTab } from "../state/tabs";

/**
 * How the open boards changed between two orders: IDs that moved to a new ID in the same place
 * (renamed), and IDs that are gone.
 */
export function diffBoardOrder(previous: readonly string[], next: readonly string[]): { renamed: [string, string][]; gone: string[] } {
  const nextSet = new Set(next);
  const previousSet = new Set(previous);
  const renamed: [string, string][] = [];
  const gone: string[] = [];
  previous.forEach((id, index) => {
    if (nextSet.has(id)) {
      return;
    }
    const replacement = next[index];
    if (replacement !== undefined && !previousSet.has(replacement)) {
      renamed.push([id, replacement]);
    } else {
      gone.push(id);
    }
  });
  return { renamed, gone };
}

export function useBoardTabs(): void {
  const order = useOpenBoardIds();
  const previous = useRef<readonly string[]>(order);
  const active = useActiveTab();

  useEffect(() => subscribeBoardOpened(openBoardTab), []);

  useEffect(() => {
    const { renamed, gone } = diffBoardOrder(previous.current, order);
    previous.current = order;
    for (const [from, to] of renamed) {
      renameBoardTab(from, to);
    }
    for (const id of gone) {
      removeTab(boardTabId(id));
    }
  }, [order]);

  const activeBoard = active?.kind === "board" ? active.boardId : null;
  useEffect(() => {
    if (activeBoard !== null) {
      setActiveBoard(activeBoard);
    }
  }, [activeBoard]);

  useEffect(
    () =>
      setBoardTabCloser(async (boardId) => {
        await closeBoard(boardId);
        return true;
      }),
    [],
  );
}
