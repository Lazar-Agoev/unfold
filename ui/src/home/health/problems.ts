/**
 * Health rows: each problem as a title, an explanation, a location, and the one action that
 * deals with it (show the file, or open the settings file in the editor); and boards with health items.
 * Unresolved calls are facts about the code, never problems, so they never appear here.
 */

import type { BoardHealthCount, ProblemCategory, ProblemItem } from "../../ipc";
import { plural } from "../../lib/format";

export type HealthAction = { readonly kind: "open"; readonly path: string; readonly line: number | null } | { readonly kind: "edit"; readonly path: string };

export interface HealthRow {
  readonly id: string;
  readonly title: string;
  readonly explanation: string;
  readonly location: string;
  readonly action: HealthAction | null;
}

/** `services/payment.ts:42`, or the path alone, or nothing. */
export function problemLocation(item: Pick<ProblemItem, "path" | "line">): string {
  if (item.path === "") {
    return "";
  }
  return item.line === null ? item.path : `${item.path}:${item.line}`;
}

const TITLES: Readonly<Record<ProblemCategory, string>> = {
  compile_failed: "Compile failed",
  settings: "Settings can't be used",
  doesnt_parse: "Doesn't parse",
  cant_read: "Can't be read",
  discovery: "Part of the folder can't be read",
  input: "Configuration input",
  cache: "Cache",
  board: "Board",
};

function explanation(item: ProblemItem): string {
  switch (item.category) {
    case "doesnt_parse":
    case "cant_read":
      return item.fileState === "last_good" ? "Showing its last version that parsed." : "It isn't on the graph until it parses.";
    case "compile_failed":
      return "The previous graph stays. Compile again once it's fixed.";
    case "settings":
      return "Unfold uses the defaults until the settings file is fixed.";
    case "discovery":
      return "Files there aren't on the graph.";
    case "input":
      return "Entrances that depend on it may be missing.";
    case "cache":
    case "board":
      return "";
  }
}

function action(item: ProblemItem): HealthAction | null {
  switch (item.category) {
    case "settings":
      return { kind: "edit", path: item.path === "" ? "unfold.toml" : item.path };
    case "discovery":
      return { kind: "edit", path: "unfold.toml" };
    case "doesnt_parse":
    case "cant_read":
    case "input":
      return item.path === "" ? null : { kind: "open", path: item.path, line: item.line };
    case "compile_failed":
    case "cache":
    case "board":
      return null;
  }
}

/** The Health list: every problem except board health (which has its own rows), in the engine's order. */
export function healthRows(problems: readonly ProblemItem[]): HealthRow[] {
  return problems.flatMap((item, index): HealthRow[] => {
    if (item.category === "board") {
      return [];
    }
    const file = item.category === "doesnt_parse" || item.category === "cant_read";
    const what = file && item.line !== null ? `line ${item.line}: ${item.message}` : item.message;
    return [
      {
        id: `${index}`,
        title: file || item.category === "compile_failed" ? `${TITLES[item.category]} — ${what}` : what,
        explanation: explanation(item),
        location: problemLocation(item),
        action: action(item),
      },
    ];
  });
}

export interface BoardHealthRow {
  readonly id: string;
  readonly board: string;
  /** "1 item needs review". */
  readonly title: string;
  /** The first item's explanation, from the Problems list. */
  readonly explanation: string;
}

/** "1 item needs review", "3 items need review". */
export function reviewWords(items: number): string {
  return `${plural(items, "item")} ${items === 1 ? "needs" : "need"} review`;
}

/**
 * Board rows from `project_summary`, explained by the board's first problem. The engine words a board
 * problem "<title>: <what>: <explanation>".
 */
export function boardHealthRows(boards: readonly BoardHealthCount[], problems: readonly ProblemItem[]): BoardHealthRow[] {
  return boards.map((board) => {
    const first = problems.find((item) => item.category === "board" && item.path === board.id);
    let words = first?.message ?? "";
    if (words.startsWith(`${board.title}: `)) {
      words = words.slice(board.title.length + 2);
    }
    const colon = words.indexOf(": ");
    const text = colon < 0 ? words : words.slice(colon + 2);
    return { id: board.id, board: board.title, title: reviewWords(board.items), explanation: text };
  });
}
