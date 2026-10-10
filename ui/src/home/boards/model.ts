/** Board lists on Home (V-49, V-65): filtering, sorting, and how a board card describes its board. */

import type { BoardSummary } from "../../ipc";
import { plural, relativeTime } from "../../lib/format";
import type { BoardsFilter, BoardsSort } from "../../state/layout";

/** Natural, case-insensitive name order ("Page 2" before "Page 10"), as the sidebar tree sorts (V-51). */
export const byName = new Intl.Collator("en", { numeric: true, sensitivity: "base" }).compare;

/** The title a board shows: its title, else its file name. */
export function boardTitle(board: Pick<BoardSummary, "title" | "name">): string {
  return board.title ?? board.name;
}

export function filterBoards(boards: readonly BoardSummary[], filter: BoardsFilter): BoardSummary[] {
  return filter === "all" ? [...boards] : boards.filter((board) => (filter === "drafts" ? board.place === "draft" : board.place === "published"));
}

/** Last edited (newest first) or by title; ties fall back to the other order. */
export function sortBoards(boards: readonly BoardSummary[], sort: BoardsSort): BoardSummary[] {
  return [...boards].sort((a, b) => {
    const title = byName(boardTitle(a), boardTitle(b));
    const edited = b.updatedAtMs - a.updatedAtMs;
    return sort === "edited" ? edited || title : title || edited;
  });
}

/** A board id under `.unfold/local/drafts/` is a draft (contract §1.3). */
export function isDraftId(id: string): boolean {
  return id.startsWith(".unfold/local/drafts/");
}

/** The subline's words before any "with <project>": "3 pages · Published · edited 2 hours ago". */
export function boardSubline(board: Pick<BoardSummary, "pages" | "place" | "changes" | "updatedAtMs" | "projects">, nowMs?: number): string {
  const parts = [plural(board.pages, "page"), board.place === "draft" ? "Draft" : "Published"];
  if (board.place === "published" && board.changes > 0) {
    parts.push(plural(board.changes, "unpublished change"));
  }
  if (board.projects.length === 0) {
    parts.push(`edited ${relativeTime(board.updatedAtMs, nowMs)}`);
  }
  return parts.join(" · ");
}
