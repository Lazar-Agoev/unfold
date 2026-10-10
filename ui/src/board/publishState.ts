/**
 * The board's publish state as the top bar shows it: "Draft", "Published", or "Published · N
 * unpublished changes", then "· uses <other projects>" for a board whose cards come from other
 * projects. The dot is faint for a draft, the accent for unpublished changes, and absent for a plain
 * published board.
 */

import { useMemo } from "react";
import type { BoardSummary } from "../ipc/generated";
import { useEngineQuery } from "../ipc";
import { plural } from "../lib/format";
import { useOpenBoard, type BoardRef } from "../state/boards";

export interface PublishState {
  readonly text: string;
  readonly dot: "faint" | "accent" | null;
  /** Whether Publish has something to do (a draft, or unpublished changes). */
  readonly publishable: boolean;
}

/** The state of a board from its place, unpublished changes, and the other projects it uses. */
export function publishState(place: "published" | "draft", changes: number, projects: readonly string[]): PublishState {
  const uses = projects.length > 0 ? ` · uses ${projects.join(", ")}` : "";
  if (place === "draft") {
    return { text: `Draft${uses}`, dot: "faint", publishable: true };
  }
  if (changes > 0) {
    return { text: `Published · ${plural(changes, "unpublished change")}${uses}`, dot: "accent", publishable: true };
  }
  return { text: `Published${uses}`, dot: null, publishable: false };
}

/** The open board's summary in its project's board list (folder, publish state), once listed. */
export function useBoardSummary(ref: BoardRef): BoardSummary | undefined {
  const list = useEngineQuery("list_boards", { project: ref.project });
  return useMemo(() => list.data?.boards.find((board) => board.id === ref.id), [list.data, ref.id]);
}

/** The top bar's publish state: from `list_boards` (kept current by `boards_changed`), else the session. */
export function usePublishState(ref: BoardRef): PublishState {
  const summary = useBoardSummary(ref);
  const place = useOpenBoard(ref, (board) => board.place) ?? "draft";
  const changes = useOpenBoard(ref, (board) => board.published?.changes ?? 0) ?? 0;
  const projects = useOpenBoard(ref, (board) => board.document.projects);
  return useMemo(
    () => (summary ? publishState(summary.place, summary.changes, summary.projects) : publishState(place, changes, projects ?? [])),
    [summary, place, changes, projects],
  );
}
