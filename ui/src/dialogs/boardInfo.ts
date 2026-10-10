/**
 * What the board dialogs say about a board: from the open session when this window holds it, else
 * from `list_boards` (Home's menus act on boards that aren't open).
 */

import { useEngineQuery } from "../ipc";
import { isDirty, useOpenBoard, type BoardRef } from "../state/boards";

export interface BoardInfo {
  readonly title: string;
  readonly place: "published" | "draft";
  /** Unpublished changes as last saved (0 for a draft). */
  readonly changes: number;
  /** The open session has edits not saved yet (Publish saves them first). */
  readonly dirty: boolean;
  /** The published file changed since the changes started from it (known for an open board). */
  readonly baseChanged: boolean;
  /** When the working file last changed, if `list_boards` has said. */
  readonly updatedAtMs: number | null;
  readonly open: boolean;
}

/** The board's facts; undefined while they load, null when the board is gone. */
export function useBoardInfo(ref: BoardRef): BoardInfo | null | undefined {
  const list = useEngineQuery("list_boards", { project: ref.project });
  const summary = list.data?.boards.find((board) => board.id === ref.id);
  const title = useOpenBoard(ref, (board) => board.document.title);
  const place = useOpenBoard(ref, (board) => board.place);
  const published = useOpenBoard(ref, (board) => board.published);
  const dirty = useOpenBoard(ref, isDirty) ?? false;
  if (title === undefined && !summary) {
    return list.isPending ? undefined : null;
  }
  return {
    title: title ?? summary?.title ?? summary?.name ?? "Untitled board",
    place: place ?? summary?.place ?? "draft",
    // `list_boards` follows every save; the session's count is as of its last save.
    changes: summary?.changes ?? published?.changes ?? 0,
    dirty,
    baseChanged: published?.baseChanged ?? false,
    updatedAtMs: summary?.updatedAtMs ?? null,
    open: title !== undefined,
  };
}
