/** How Home and the sidebar name boards, and how a board row on Home describes its board. */

import type { BoardSummary } from "../../ipc";
import { relativeTime } from "../../lib/format";

/** Natural, case-insensitive name order ("Page 2" before "Page 10"), as the sidebar tree sorts. */
export const byName = new Intl.Collator("en", { numeric: true, sensitivity: "base" }).compare;

/** The title a board shows: its title, else its file name. */
export function boardTitle(board: Pick<BoardSummary, "title" | "name">): string {
  return board.title ?? board.name;
}

/** When a board was last edited, as its row on Home says it: "edited 2 minutes ago", "edited Oct 9". */
export function editedText(board: Pick<BoardSummary, "updatedAtMs">, nowMs?: number): string {
  return `edited ${relativeTime(board.updatedAtMs, nowMs)}`;
}
