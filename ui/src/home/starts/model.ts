/**
 * How it starts, as rows (entrance-definitions.md, "Home: How it starts"): each app, then its entrances,
 * in file order, with what starts it, what a click does, and its problem. A problem is the entry's own
 * (it then stays out of the graph), else the first of its `code` keys that isn't there: missing, not
 * analyzed, or not a start. Everything comes from `entrances`; no key is read.
 */

import type { AppView, CodeFix, EntranceView, EntrancesView, Key, SettingsEdit } from "../../ipc/generated";

/** Search appears once the list has more than this many entrances. */
export const SEARCH_THRESHOLD = 15;

/** One `code` key of an app (`entrance` null) or an entrance, by its place in the list. */
export interface CodeSlot {
  readonly app: AppView;
  readonly entrance: EntranceView | null;
  readonly index: number;
}

/** What is wrong with a row. */
export type RowProblem =
  | { readonly kind: "entry"; readonly message: string; readonly line: number | undefined }
  | { readonly kind: "missing"; readonly slot: CodeSlot; readonly fix: CodeFix | undefined }
  | { readonly kind: "not_analyzed"; readonly slot: CodeSlot }
  | { readonly kind: "not_a_start"; readonly slot: CodeSlot };

/** What a problem's Fix does: write the one candidate, choose code by name, or open `unfold.toml` (where `[scope]` is). */
export type RowFix =
  | { readonly kind: "rewrite"; readonly slot: CodeSlot; readonly code: Key }
  | { readonly kind: "choose"; readonly slot: CodeSlot }
  | { readonly kind: "config" };

/** The fix a problem offers; an entry's own problem has none. */
export function fixOf(problem: RowProblem): RowFix | null {
  switch (problem.kind) {
    case "missing":
      return problem.fix ? { kind: "rewrite", slot: problem.slot, code: problem.fix.code } : { kind: "choose", slot: problem.slot };
    case "not_a_start":
      return { kind: "choose", slot: problem.slot };
    case "not_analyzed":
      return { kind: "config" };
    case "entry":
      return null;
  }
}

/** What clicking a row does: trace it onto a new board, open its lines in `unfold.toml`, or nothing. */
type RowAction = { readonly kind: "trace"; readonly key: Key } | { readonly kind: "file"; readonly line: number } | null;

export interface StartRow {
  readonly id: string;
  readonly kind: "app" | "entrance";
  readonly name: string;
  /** An app's `run`, an entrance's `on`. */
  readonly command: string | undefined;
  readonly action: RowAction;
  readonly problem: RowProblem | null;
}

/** One app's rows: the app, then the entrances listed. */
export interface AppRows {
  readonly id: string;
  readonly app: StartRow;
  readonly entrances: readonly StartRow[];
}

/** The first code key that isn't there, as a problem. */
function codeProblem(app: AppView, entrance: EntranceView | null): RowProblem | null {
  const codes = entrance ? entrance.code : app.code ? [app.code] : [];
  for (const [index, code] of codes.entries()) {
    const slot = { app, entrance, index };
    switch (code.state) {
      case "missing":
        return { kind: "missing", slot, fix: code.fix };
      case "not_analyzed":
        return { kind: "not_analyzed", slot };
      case "not_a_start":
        return { kind: "not_a_start", slot };
      case "found":
      case "stale":
      case undefined:
        break;
    }
  }
  return null;
}

/** A row's click: trace what the graph holds; open the lines of an entry the graph left out. */
function rowAction(key: Key | undefined, inGraph: boolean, traceable: boolean, problem: RowProblem | null, lines: [number, number] | undefined): RowAction {
  if (inGraph) {
    return traceable && key !== undefined ? { kind: "trace", key } : null;
  }
  const line = (problem?.kind === "entry" ? problem.line : undefined) ?? lines?.[0];
  return line !== undefined ? { kind: "file", line } : null;
}

/** The rows of every app, in file order. */
export function startRows(view: EntrancesView): AppRows[] {
  return view.apps.map((app, appIndex) => {
    const appProblem: RowProblem | null = app.problem ? { kind: "entry", message: app.problem.message, line: app.problem.line } : codeProblem(app, null);
    const appRow: StartRow = {
      id: `app-${appIndex}`,
      kind: "app",
      name: app.name ?? "Unnamed app",
      command: app.run,
      action: rowAction(app.key, app.problem === undefined, app.code !== undefined, appProblem, app.lines),
      problem: appProblem,
    };
    const entrances = app.entrances.map((entrance, index): StartRow => {
      const problem: RowProblem | null = entrance.problem
        ? { kind: "entry", message: entrance.problem.message, line: entrance.problem.line }
        : codeProblem(app, entrance);
      const inGraph = app.problem === undefined && entrance.problem === undefined;
      return {
        id: `app-${appIndex}-${index}`,
        kind: "entrance",
        name: entrance.name ?? "Unnamed entrance",
        command: entrance.on,
        action: rowAction(entrance.key, inGraph, true, problem, entrance.lines),
        problem,
      };
    });
    return { id: appRow.id, app: appRow, entrances };
  });
}

/** How many entrances the file lists. */
export function entranceCount(view: EntrancesView): number {
  return view.apps.reduce((count, app) => count + app.entrances.length, 0);
}

function matches(row: StartRow, words: string): boolean {
  return row.name.toLowerCase().includes(words) || (row.command?.toLowerCase().includes(words) ?? false);
}

/**
 * The rows that match a search by name and what starts them: an app that matches keeps all its
 * entrances, any other keeps those that match, and an app with neither is left out.
 */
export function filterRows(rows: readonly AppRows[], query: string): readonly AppRows[] {
  const words = query.trim().toLowerCase();
  if (words === "") {
    return rows;
  }
  return rows.flatMap((group) => {
    if (matches(group.app, words)) {
      return [group];
    }
    const entrances = group.entrances.filter((row) => matches(row, words));
    return entrances.length > 0 ? [{ ...group, entrances }] : [];
  });
}

/**
 * The edit that writes `code` in place of one code key: the entrance's `set_entrance`, or the app's
 * `set_app`, with every field as the list shows it (null for one it doesn't have).
 */
export function rewriteCode({ app, entrance, index }: CodeSlot, code: Key): SettingsEdit | null {
  if (app.name === undefined) {
    return null;
  }
  if (entrance === null) {
    return { op: "set_app", app: { name: app.name, run: app.run ?? null, code, about: app.about ?? null } };
  }
  if (entrance.name === undefined) {
    return null;
  }
  return {
    op: "set_entrance",
    app: app.name,
    entrance: {
      name: entrance.name,
      on: entrance.on ?? null,
      code: entrance.code.map((item, at) => (at === index ? code : item.key)),
      about: entrance.about ?? null,
    },
  };
}

/** What Copy agent prompt copies: a short request for Claude Code or Codex. */
export const AGENT_PROMPT = [
  "Set up how this project starts in Unfold.",
  "Read its routes, workers, schedules, and scripts.",
  "Write its [[app]] tables in unfold.toml with set_app and set_entrance: an app for each thing that runs, with its run command and code, and an entrance for each route, queue, schedule, or command it handles.",
  "Then check them with list_entrances, and fix any code that isn't found.",
].join(" ");
